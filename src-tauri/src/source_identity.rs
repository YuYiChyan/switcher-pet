//! Credential-free presentation of a login identity and its selected workspace.
use serde_json::{json, Value};
use std::hash::{Hash, Hasher};

fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str().map(str::trim).filter(|s| !s.is_empty())
}
fn claims(auth: &Value, key: &str) -> Value {
    use base64::Engine;
    text(&auth["tokens"],key).or_else(||text(auth,key))
        .and_then(|s|s.split('.').nth(1))
        .and_then(|s|base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s.trim_end_matches('=')).ok())
        .and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or(Value::Null)
}
fn short_id(value: &str) -> String {
    let mut hasher=std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);format!("{:012x}",hasher.finish() & 0xffffffffffff)
}
fn date(value: Option<&str>) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    value.and_then(|s|chrono::DateTime::parse_from_rfc3339(s).ok())
}

/// These are cached observations, not proof that billing renewed or was canceled.
/// Access-token expiry and a manually entered expiry date are NOT subscription status.
pub fn describe(auth: &Value, quota: Option<&Value>, fallback: &str, now: i64) -> Value {
    let access=claims(auth,"access_token");let id=claims(auth,"id_token");
    let access_auth=&access["https://api.openai.com/auth"];
    let id_auth=&id["https://api.openai.com/auth"];
    let workspace=text(&auth["tokens"],"account_id")
        .or_else(||text(access_auth,"chatgpt_account_id")).or_else(||text(id_auth,"chatgpt_account_id"));
    // An ID token may describe the personal grant while tokens.account_id routes
    // to a Business workspace. Never attach its plan/expiry to another workspace.
    let belongs=|v:&Value|workspace.is_none() || text(v,"chatgpt_account_id")==workspace;
    let selected_access=if belongs(access_auth){access_auth}else{&Value::Null};
    let selected_id=if belongs(id_auth){id_auth}else{&Value::Null};
    let plan=quota.and_then(|q|text(q,"plan_type")).filter(|p|*p!="unknown")
        .or_else(||text(selected_access,"chatgpt_plan_type"))
        .or_else(||text(selected_id,"chatgpt_plan_type")).unwrap_or("unknown").to_lowercase();
    let plan_label=match plan.as_str() {
        "free"=>"Free","plus"=>"Plus","pro"=>"Pro","team"|"business"=>"Business",
        "enterprise"=>"Enterprise","edu"|"education"=>"Edu","go"=>"Go","unknown"=>"订阅未知",other=>other
    };
    let scope=match plan.as_str() {
        "team"|"business"|"enterprise"|"edu"|"education"=>"工作区",
        "free"|"plus"|"pro"|"go"=>"个人空间",_=>"空间待识别"
    };
    let user=text(access_auth,"chatgpt_user_id").or_else(||text(access_auth,"user_id"))
        .or_else(||text(id_auth,"chatgpt_user_id")).or_else(||text(id_auth,"user_id"))
        .or_else(||text(&access["https://api.openai.com/profile"],"user_id"))
        .or_else(||text(&access,"sub")).or_else(||text(&id,"sub"));
    let email=text(&id,"email").or_else(||text(&access["https://api.openai.com/profile"],"email"));
    let until=text(selected_id,"chatgpt_subscription_active_until");
    let start=date(text(selected_id,"chatgpt_subscription_active_start"));
    let expires=date(until);
    let status=if plan=="free" {"免费"}
        else if start.is_some_and(|d|d.timestamp()>now) {"订阅待生效"}
        else if expires.is_some_and(|d|d.timestamp()<=now) {"订阅日期已到"}
        else if expires.is_some() {"订阅期内"}
        else if plan=="unknown" {"状态待查询"} else {"未返回到期日"};
    let checked=quota.and_then(|q|text(q,"updated_at"))
        .or_else(||text(selected_id,"chatgpt_subscription_last_checked"));
    let stale=date(checked).is_none_or(|d|now-d.timestamp()>24*3600);
    // Organization membership is not necessarily the selected ChatGPT workspace.
    // Only use an organization title when its ID explicitly matches.
    let workspace_name=selected_id["organizations"].as_array().into_iter().flatten()
        .find(|o|workspace.is_some() && text(o,"id")==workspace).and_then(|o|text(o,"title"));
    let workspace_ref=workspace.map(short_id);
    let member_ref=short_id(user.unwrap_or(fallback));
    json!({"plan":plan,"plan_label":plan_label,"scope":scope,"email":email,
        "workspace_name":workspace_name,"workspace_ref":workspace_ref,"member_ref":member_ref,
        "status":status,"subscription_until":until,"checked_at":checked,"stale":stale,
        "context_label":format!("{} · {} · {} · 席位 {}",plan_label,scope,
            workspace_ref.as_deref().unwrap_or("空间未知"),member_ref)})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn token(auth:Value,email:&str)->String {
        use base64::Engine;
        format!("test.{}.test",base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&json!({"https://api.openai.com/auth":auth,"email":email})).unwrap()))
    }
    fn fixture(workspace:&str,user:&str,plan:&str)->Value {
        let token=token(json!({"chatgpt_account_id":workspace,"chatgpt_user_id":user,"chatgpt_plan_type":plan,
            "chatgpt_subscription_active_until":"2099-01-01T00:00:00Z"}),"person@example.invalid");
        json!({"tokens":{"account_id":workspace,"access_token":token,"id_token":token}})
    }
    #[test] fn personal_workspace_and_seats_remain_distinct() {
        let a=describe(&fixture("personal","u1","free"),None,"a",1000);
        let b=describe(&fixture("team","u1","team"),None,"b",1000);
        let c=describe(&fixture("team","u2","team"),None,"c",1000);
        assert_eq!(a["member_ref"],b["member_ref"]);assert_ne!(a["workspace_ref"],b["workspace_ref"]);
        assert_eq!(b["workspace_ref"],c["workspace_ref"]);assert_ne!(b["member_ref"],c["member_ref"]);
        assert_eq!(b["plan_label"],"Business");assert_eq!(b["status"],"订阅期内");
        assert_eq!(a["status"],"免费");
        assert!(!b.to_string().contains("access_token"));assert!(!b.to_string().contains("test."));
    }
    #[test] fn mismatched_id_token_never_assigns_personal_subscription_to_workspace() {
        let mut a=fixture("workspace","u1","team");
        a["tokens"]["id_token"]=fixture("personal","u1","plus")["tokens"]["id_token"].clone();
        let b=describe(&a,None,"a",1000);
        assert_eq!(b["plan"],"team");assert!(b["subscription_until"].is_null());
        assert_eq!(b["status"],"未返回到期日");
    }
    #[test] fn unknown_expiry_and_login_expiry_are_not_subscription_expiry() {
        let b=describe(&json!({"tokens":{"access_token":"expired"}}),None,"a",1000);
        assert_eq!(b["plan"],"unknown");assert_eq!(b["status"],"状态待查询");
        let b=describe(&fixture("team","u1","team"),None,"a",4102444801);
        assert_eq!(b["status"],"订阅日期已到");
    }
}
