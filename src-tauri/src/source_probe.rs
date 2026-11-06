//! Opt-in GET-only probes. No OAuth refresh, completions, tools, or arbitrary URLs.
use crate::{account::{AccountStore, RelayUsageCache}, AppState};
use chrono::Utc;
use futures_util::StreamExt;
use serde::{Serialize, Deserialize};
use sha2::{Digest, Sha256};
use serde_json::Value;
use std::{collections::BTreeMap, sync::{Mutex, OnceLock}, time::Duration};
use tauri::{Emitter, Manager};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProbeStatus { pub last_at: i64, pub next_at: i64, pub status: String, #[serde(default)] pub identity: String }
static STATUS: OnceLock<Mutex<BTreeMap<String, ProbeStatus>>> = OnceLock::new();
static RUNNING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
fn status_path() -> std::path::PathBuf { crate::app_paths::profile_home().unwrap().join(".codex-switcher/probe-status.json") }
fn save_statuses() -> Result<(),String> {
    let data=serde_json::to_vec(&snapshot()).map_err(|e|e.to_string())?;
    let path=status_path(); let tmp=path.with_extension("tmp");
    std::fs::write(&tmp,data).map_err(|e|e.to_string())?;
    std::fs::rename(tmp,path).map_err(|e|e.to_string())
}
pub fn policy_for(store: &AccountStore, id: &str) -> crate::pool_math::SourcePolicy {
    store.settings.pool_settings.sources.get(id).cloned().unwrap_or_else(|| {
        let mut p=crate::pool_math::SourcePolicy::default();
        if store.accounts.get(id).is_some_and(|a|a.is_relay()) {p.probe_models=true;p.balance_adapter="auto".into();}
        p
    })
}
fn statuses() -> &'static Mutex<BTreeMap<String, ProbeStatus>> { STATUS.get_or_init(Default::default) }
pub fn snapshot() -> BTreeMap<String, ProbeStatus> { statuses().lock().map(|s| s.clone()).unwrap_or_default() }

#[derive(Debug)]
struct Failure { code: String, retry: i64 }
impl Failure { fn new(code: &str) -> Self { Self { code:code.into(), retry:900 } } }

fn api_root(base: &str) -> Result<String, Failure> {
    crate::relay_catalog::models_url(base).map_err(|_| Failure::new("invalid_base_url"))?;
    let models=crate::relay_catalog::models_url(base).map_err(|_|Failure::new("invalid_base_url"))?;
    Ok(models.trim_end_matches("/models").to_owned())
}
async fn get(client: &reqwest::Client, url: &str, key: &str) -> Result<Value, Failure> {
    let response = client.get(url).bearer_auth(key).header("Accept","application/json")
        .send().await.map_err(|_| Failure::new("network_error"))?;
    if !response.status().is_success() {
        let status=response.status().as_u16();
        let retry = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| {
            v.parse::<i64>().ok().or_else(|| chrono::DateTime::parse_from_rfc2822(v).ok().map(|t|t.timestamp()-Utc::now().timestamp()))
        }).unwrap_or(900).clamp(1,86400);
        return Err(Failure { code:format!("http_{status}"),retry });
    }
    let mut stream=response.bytes_stream();
    let mut bytes=Vec::new();
    while let Some(chunk)=stream.next().await {
        let chunk=chunk.map_err(|_| Failure::new("read_error"))?;
        if bytes.len()+chunk.len()>2*1024*1024 { return Err(Failure::new("response_too_large")); }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| Failure::new("invalid_json"))
}
async fn balance(client: &reqwest::Client, base: &str, key: &str, adapter: &str) -> Result<(RelayUsageCache,String), Failure> {
    let root=api_root(base)?;
    if adapter=="auto" {
        let host=reqwest::Url::parse(base).ok().and_then(|u|u.host_str().map(str::to_owned)).unwrap_or_default();
        if host=="api.deepseek.com" {return Box::pin(balance(client,base,key,"deepseek")).await;}
        for candidate in ["new_api_budget","openai_usage"] {
            match Box::pin(balance(client,base,key,candidate)).await {
                Ok(cache)=>return Ok(cache),
                Err(e) if ["http_404","http_405","invalid_json","budget_unknown","balance_unknown"].contains(&e.code.as_str())=>{},
                Err(e)=>return Err(e),
            }
        }
        return Err(Failure {code:"balance_unsupported".into(),retry:21600});
    }
    let (amount, unit, active, capacity) = match adapter {
        "deepseek" => {
            let mut url=reqwest::Url::parse(base).map_err(|_|Failure::new("invalid_base_url"))?;
            url.set_path("/user/balance");
            let v=get(client,url.as_str(),key).await?;
            let rows=v.get("balance_infos").and_then(Value::as_array).ok_or_else(||Failure::new("balance_unknown"))?;
            // One currency only; never sum USD and CNY as if they were equal.
            let row=rows.iter().find(|r|r["currency"]=="CNY").or_else(||rows.first()).ok_or_else(||Failure::new("balance_unknown"))?;
            let amount=row["total_balance"].as_f64().or_else(||row["total_balance"].as_str().and_then(|s|s.parse().ok())).ok_or_else(||Failure::new("balance_unknown"))?;
            (amount,row["currency"].as_str().unwrap_or("unknown").into(),v["is_available"].as_bool().unwrap_or(true),None)
        }
        "openai_usage" => {
            let v=get(client,&format!("{root}/usage"),key).await?;
            let amount=v.get("remaining").or_else(||v.pointer("/quota/remaining")).or_else(||v.get("balance"))
                .and_then(Value::as_f64).filter(|v|v.is_finite()).ok_or_else(||Failure::new("balance_unknown"))?;
            // Do not invent USD for an endpoint with no currency metadata.
            let unit=v.get("unit").or_else(||v.pointer("/quota/unit")).and_then(Value::as_str).unwrap_or("unknown").to_owned();
            (amount,unit,v.get("is_active").or_else(||v.get("isValid")).and_then(Value::as_bool).unwrap_or(true),v.get("total").or_else(||v.pointer("/quota/total")).and_then(Value::as_f64))
        }
        "new_api_budget" => {
            let sub=get(client,&format!("{root}/dashboard/billing/subscription"),key).await?;
            let usage=get(client,&format!("{root}/dashboard/billing/usage"),key).await?;
            let limit=sub.get("soft_limit_usd").and_then(Value::as_f64).ok_or_else(||Failure::new("budget_unknown"))?;
            let used=usage.get("total_usage").and_then(Value::as_f64).ok_or_else(||Failure::new("usage_unknown"))?;
            (limit-used/100.0,"USD".into(),true,Some(limit))
        }
        _ => return Err(Failure::new("adapter_disabled")),
    };
    if !amount.is_finite() {return Err(Failure::new("balance_unknown"));}
    Ok((RelayUsageCache { balance_capacity_estimated:false, balance_capacity:capacity.filter(|v|v.is_finite() && *v>0.0), remaining:amount, unit, is_active:active, updated_at:Utc::now(), next_reset_at:None,windows:capacity.filter(|v|v.is_finite() && *v>0.0).map(|total|vec![crate::account::RelayQuotaWindow{label:"balance".into(),remaining_percent:Some((amount/total*100.0).clamp(0.0,100.0)),reset_at:None}]).unwrap_or_default() },adapter.to_owned()))
}

async fn run(app: &tauri::AppHandle, id: &str, manual: bool, quota_only: bool) -> Result<(), String> {
    let _guard=if quota_only { RUNNING.lock().await } else { RUNNING.try_lock().map_err(|_|"A probe is already running")? };
    let state=app.state::<AppState>();
    let (base,key,policy)= {
        let store=state.store.lock().map_err(|e|e.to_string())?;
        let account=store.accounts.get(id).ok_or("Unknown source")?;
        if !account.is_relay() {return Err("API source needs its provider URL and type; OAuth is never probed".into());}
        if account.is_banned || (account.is_logged_out && !manual) {return Err("Source is disabled; use a manual probe to recheck logged-out API keys".into());}
        let policy=policy_for(&store,id);
        (account.relay_base_url.clone().ok_or("Missing API URL")?,AccountStore::extract_access_token(&account.auth_json).ok_or("Missing API key")?,policy)
    };
    let identity=format!("{:x}",Sha256::digest(format!("{base}\0{key}").as_bytes()));
    let client=reqwest::Client::builder().timeout(Duration::from_secs(15)).redirect(reqwest::redirect::Policy::none()).build().map_err(|_|"Cannot create probe client")?;
    let mut errors=Vec::new();
    let mut ran=false;
    for kind in ["models","balance"] {
        if quota_only && kind=="models" {continue;}
        let enabled=if kind=="models" {manual || policy.probe_models} else {policy.balance_adapter!="off"};
        if !enabled {continue;}
        let status_key=format!("{id}:{kind}");
        let now=Utc::now().timestamp();
        let previous=snapshot().get(&status_key).filter(|s|s.identity==identity).cloned().unwrap_or_default();
        if snapshot().iter().any(|(k,s)| k.starts_with(&format!("{id}:")) && s.identity==identity && s.status=="http_429" && s.next_at>now) {errors.push("rate_limited: wait until the displayed retry time".into());break;}
        // Retry-After applies even to the manual button.
        if previous.next_at>now && (!manual || previous.status.starts_with("http_429")) {continue;}
        // Recheck configuration/identity before each request, not only before a batch.
        let allowed={
            let store=state.store.lock().map_err(|e|e.to_string())?;
            store.accounts.get(id).is_some_and(|a| a.relay_base_url.as_deref()==Some(&base)
                && AccountStore::extract_access_token(&a.auth_json).as_deref()==Some(&key)
                && !a.is_banned && (manual || !a.is_logged_out))
                && {let p=policy_for(&store,id);if kind=="models" {manual || p.probe_models} else {p.balance_adapter==policy.balance_adapter}}
        };
        if !allowed {break;}
        ran=true;
        let result = if kind=="models" {
            match crate::relay_catalog::models_url(&base) {
                Ok(url) => match get(&client,&url,&key).await {
                    Ok(v) => {
                        let models=crate::relay_catalog::parse_models_response(&v);
                        if models.is_empty() {Err(Failure::new("empty_model_catalog"))}
                        else {Ok((Some(models),None))}
                    }
                    Err(e)=>Err(e),
                },
                Err(_)=>Err(Failure::new("invalid_base_url")),
            }
        } else { balance(&client,&base,&key,&policy.balance_adapter).await.map(|cache|(None,Some(cache))) };
        let minutes=if kind=="models" {policy.model_minutes} else {policy.balance_minutes};
        let jitter=id.bytes().fold(0u64,|a,b|a.wrapping_add(b as u64))%16;
        let mut status=ProbeStatus {last_at:now,next_at:now+minutes as i64*60+jitter as i64,status:"ok".into(),identity:identity.clone()};
        match result {
            Ok((models,cache))=> {
                let mut store=state.store.lock().map_err(|e|e.to_string())?;
                let permitted={let p=policy_for(&store,id);if kind=="models" {manual || p.probe_models} else {p.balance_adapter==policy.balance_adapter}};
                if let Some(account)=store.accounts.get_mut(id).filter(|a| permitted && a.relay_base_url.as_deref()==Some(&base)
                    && AccountStore::extract_access_token(&a.auth_json).as_deref()==Some(&key)) {
                    // A verified static key is not an expired OAuth session.
                    account.is_token_invalid=false;account.is_logged_out=false;
                    account.keepalive.last_error=None;
                    if let Some(models)=models {account.relay_model_catalog=models;}
                    if let Some((mut cache,adapter))=cache {
                        cache.preserve_reference(account.relay_usage_cache.as_ref());
                        account.relay_usage_cache=Some(cache);
                        if policy.balance_adapter=="auto" {
                            account.relay_usage_preset=Some(match adapter.as_str(){"new_api_budget"=>"new_api_dashboard","openai_usage"=>"openai_compat",_=>"deepseek"}.into());
                        }
                    }
                    crate::relay_catalog::ensure_currents(&mut store);
                    store.save()?;
                } else {status.status="source_changed".into();}
            }
            Err(e)=> {status.next_at=now+e.retry+jitter as i64;status.status=e.code.clone();errors.push(format!("{kind}: {}",e.code));}
        }
        statuses().lock().map_err(|e|e.to_string())?.insert(status_key,status);
    }
    if ran { save_statuses()?; let _=app.emit("accounts-updated",()); }
    if errors.is_empty(){Ok(())}else{Err(errors.join("; "))}
}

pub async fn refresh_balance(app: &tauri::AppHandle, id: &str) -> Result<(),String> {run(app,id,true,true).await}

#[tauri::command]
pub async fn probe_source_now(app: tauri::AppHandle, id: String) -> Result<(),String> {run(&app,&id,true,false).await}

pub fn start(app: tauri::AppHandle) {
    // One-time backfill of the fork's existing API observations. A baseline
    // must persist: recomputing it from each smaller balance would show 100% forever.
    {let state=app.state::<AppState>();if let Ok(mut store)=state.store.lock(){
        let mut changed=false;
        for a in store.accounts.values_mut().filter(|a|a.is_relay()) {
            if let Some(cache)=a.relay_usage_cache.as_mut().filter(|b|b.balance_capacity.is_none()&&b.remaining.is_finite()&&b.remaining>0.0) {
                cache.balance_capacity=Some(cache.remaining);cache.balance_capacity_estimated=true;changed=true;
            }
        }
        if changed{let _=store.save();}
    };}
    if let Ok(data)=std::fs::read(status_path()) {
        if let Ok(saved)=serde_json::from_slice::<BTreeMap<String,ProbeStatus>>(&data) {if let Ok(mut statuses)=statuses().lock(){*statuses=saved;}}
    }
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(15)).await;
            let ids= {
                let state=app.state::<AppState>();
                state.store.lock().map(|store|store.accounts.iter()
                    .filter(|(id,a)|{let p=policy_for(&store,id);a.is_relay() && !a.is_banned && !a.is_logged_out && (p.probe_models || p.balance_adapter!="off")}).map(|(id,_)|id.clone()).collect::<Vec<_>>()).unwrap_or_default()
            };
            for id in ids {let _=run(&app,&id,false,false).await;}
        }
    });
}
