use sha2::{Digest,Sha256};
use crate::{account::AccountStore, pool_math::{self, Balance, PoolMember, SourcePolicy}, relay_catalog, AppState};
use tauri::{Emitter, State};

#[tauri::command]
pub fn set_source_policy(state: State<AppState>, app: tauri::AppHandle, id: String, policy: SourcePolicy) -> Result<(),String> {
    policy.validate()?;
    let mut store=state.store.lock().map_err(|e|e.to_string())?;
    let account=store.accounts.get(&id).ok_or("Unknown account")?;
    if !account.is_relay() && (policy.probe_models || policy.balance_adapter!="off") {return Err("Automatic probes are API-only; OAuth ownership has not been migrated".into());}
    let before=store.settings.pool_settings.sources.insert(id.clone(),policy);
    if let Err(e)=store.save(){
        match before {Some(p)=>{store.settings.pool_settings.sources.insert(id,p);},None=>{store.settings.pool_settings.sources.remove(&id);}}
        return Err(e);
    }
    drop(store);let _=app.emit("accounts-updated",());Ok(())
}

#[tauri::command]
pub fn get_pool_dashboard(state: State<AppState>) -> Result<serde_json::Value,String> {
    let store=state.store.lock().map_err(|e|e.to_string())?;
    let candidates=relay_catalog::candidates(&store);
    let mut members=Vec::new();
    for a in store.accounts.values() {
        let policy=crate::source_probe::policy_for(&store,&a.id);
        let q=a.cached_quota.as_ref();
        let subscription=a.is_chatgpt_oauth();
        let short_limited=q.and_then(|q| match q.primary_window_seconds {
            Some(18000)=>Some(true),
            Some(seconds) if seconds>=604800 && q.secondary_window_seconds.is_none()=>Some(false),
            _=>None,
        });
        let short_left=q.filter(|q|q.primary_window_seconds==Some(18000)).map(|q|q.five_hour_left);
        let weekly_left=q.and_then(|q| if q.secondary_window_seconds.is_some_and(|s|(604800..2592000).contains(&s)) {Some(q.weekly_left)}
            else if q.primary_window_seconds.is_some_and(|s|(604800..2592000).contains(&s)) {Some(q.five_hour_left)} else {None});
        let weekly_supported=q.and_then(|q|if weekly_left.is_some(){Some(true)}
            else if q.primary_window_seconds.is_some_and(|s|s>=2592000)&&q.secondary_window_seconds.is_none(){Some(false)}else{None});
        let balance=a.relay_usage_cache.as_ref().map(|b|Balance {amount:b.remaining,unit:b.unit.clone(),
            kind:if policy.balance_adapter=="new_api_budget" || a.relay_usage_preset.as_deref()==Some("new_api_dashboard") {"budget"} else if ["USD","CNY","USDcent","EUR"].contains(&b.unit.as_str()) {"balance"} else {"quota"}.into(),updated_at:b.updated_at.timestamp(),
            estimated:policy.balance_reference.is_some() || b.balance_capacity_estimated || b.balance_capacity.is_none(),
            capacity:policy.balance_reference.or(b.balance_capacity).or_else(||b.windows.iter().find(|w|w.label=="balance").and_then(|w|w.remaining_percent).filter(|p|*p>0.0 && *p<=100.0).map(|p|b.remaining*100.0/p)).or_else(||(b.remaining>0.0).then_some(b.remaining))});
        let unavailable=a.is_banned || a.is_logged_out || a.is_token_invalid || a.relay_usage_cache.as_ref().is_some_and(|b|!b.is_active);
        let mut models:Vec<_>=candidates.iter().filter(|m|m.account_id==a.id && relay_catalog::enabled(&store,m)).map(|m|relay_catalog::normalized_upstream(m).to_owned()).collect();
        models.extend(a.relay_model_map.iter().flat_map(|map|map.keys()).filter(|model|
            store.settings.model_route_policies.get(*model).is_none_or(|p|p.enabled)
            && relay_catalog::resolve_on_account(&store,model,&a.id).is_some()).cloned());
        members.push(PoolMember {id:a.id.clone(),name:a.name.clone(),subscription,
            identity:if a.is_relay() {
                let root=a.relay_base_url.as_deref().and_then(|u|relay_catalog::models_url(u).ok()).unwrap_or_default();
                format!("api:{:x}",Sha256::digest(format!("{root}\0{}",AccountStore::extract_access_token(&a.auth_json).unwrap_or_default()).as_bytes()))
            } else {pool_math::subscription_identity(AccountStore::extract_account_id(&a.auth_json).as_deref(),
                AccountStore::extract_openai_user_id(&a.auth_json).or_else(||AccountStore::extract_email(&a.auth_json).map(|s|s.to_lowercase())).as_deref(),&a.id)},
            plan:q.map(|q|q.plan_type.clone()).unwrap_or_default(),policy,short_limited,short_left,weekly_supported,weekly_left,
            updated_at:q.map(|q|q.updated_at.timestamp()),balance,available_models:models,unavailable});
    }
    members.sort_by(|a,b|a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    let now=chrono::Utc::now().timestamp();
    let rows:Vec<_>=members.iter().map(|m|{
        let mut own=m.clone();own.policy.included=true;
        serde_json::json!({"id":m.id,"name":m.name,"subscription":m.subscription,"plan":m.plan,"policy":m.policy,
            "default_weight":pool_math::default_weight(&m.plan),"unavailable":m.unavailable,
            "probe_supported":store.accounts.get(&m.id).is_some_and(|a|a.is_relay()),
            "updated_at":m.updated_at,"balance_updated_at":m.balance.as_ref().map(|b|b.updated_at),
            "summary":pool_math::aggregate(&[own],now)})
    }).collect();
    let activity=crate::pet::get_pet_activity();
    let model=relay_catalog::resolve(&store,&activity.model).map(|m|relay_catalog::normalized_upstream(&m).to_owned())
        .unwrap_or_else(||crate::model_routes::logical_model(&activity.model).to_owned());
    let scoped=pool_math::members_for_model(&members,&model);
    let mut google=std::collections::BTreeMap::new();
    for a in store.accounts.values().filter(|a|a.is_antigravity_oauth() && !a.is_logged_out && !a.is_banned && !a.is_token_invalid && crate::source_probe::policy_for(&store,&a.id).included) {
        if let Some(q)=crate::model_routes::google_upstream(&store,&a.id,&model,None)
            .and_then(|wire|crate::antigravity::quota::read_model_quotas(&a.auth_json).remove(&wire)) {
            let identity=format!("{:x}",Sha256::digest(format!("{}\0{}",a.auth_json["project_id"],
                a.auth_json.pointer("/tokens/refresh_token").and_then(|v|v.as_str()).unwrap_or(&a.id)).as_bytes()));
            let replace=google.get(&identity).is_none_or(|old:&crate::antigravity::quota::ModelQuota|q.updated_at>old.updated_at);
            if replace{google.insert(identity,q);}
        }
    }
    let mut model_quota=pool_math::WindowTotal::default();
    let mut model_short=pool_math::WindowTotal::default();
    let mut model_weekly=pool_math::WindowTotal::default();
    for q in google.values() {
        model_quota.capacity+=100.0;
        let fresh=chrono::DateTime::parse_from_rfc3339(&q.updated_at).ok().is_some_and(|t|now-t.timestamp()<=1800 && t.timestamp()<=now+60);
        if fresh && q.remaining_fraction.is_finite(){model_quota.known+=1;model_quota.remaining+=q.remaining_fraction.clamp(0.0,1.0)*100.0;}
        else{model_quota.unknown+=1;}
        for (window,total) in [(&q.five_hour,&mut model_short),(&q.weekly,&mut model_weekly)] {
            if let Some(window)=window {
                total.capacity+=100.0;
                if fresh&&window.remaining_fraction.is_finite(){total.known+=1;total.remaining+=window.remaining_fraction.clamp(0.0,1.0)*100.0;}
                else{total.unknown+=1;}
            }
        }
    }
    Ok(serde_json::json!({"summary":pool_math::aggregate(&members,now),"model":model,"model_summary":pool_math::aggregate(&scoped,now),"model_quota":model_quota,"model_short":model_short,"model_weekly":model_weekly,"accounts":rows,"probes":crate::source_probe::snapshot(),"now":now}))
}
