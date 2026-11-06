//! Imported Google identities share an owner with the original Switcher.
//! Borrow only its current AT in memory; NEVER refresh or write its credentials.
use chrono::{DateTime,Utc};
use base64::Engine;
pub struct Lease {pub access_token:String,pub expires_at:DateTime<Utc>}
pub fn original_owner(id:&str,refresh:Option<&str>,project:&str)->Option<Result<Lease,String>> {
    original_owner_kind(id,refresh,Some(project))
}
pub fn original_openai_owner(id:&str,refresh:Option<&str>)->Option<Result<Lease,String>> {
    original_owner_kind(id,refresh,None)
}
fn original_owner_kind(id:&str,refresh:Option<&str>,project:Option<&str>)->Option<Result<Lease,String>> {
    // Validation must never even inspect a real credential store.
    if std::env::args().any(|a|a=="--validation") {return None;}
    let path=crate::app_paths::readonly_google_owner_store()?;
    if !path.exists(){return None;}
    let read=(||{
        let meta=std::fs::metadata(&path).map_err(|_|"Cannot inspect original Google credential owner")?;
        if meta.len()>32*1024*1024{return Err("Original credential store exceeds safety limit");}
        let data=std::fs::read(&path).map_err(|_|"Cannot read original Google credential owner")?;
        let value:serde_json::Value=serde_json::from_slice(&data).map_err(|_|"Original credential store is being updated; retry shortly")?;
        Ok(value)
    })();
    // Fail closed on read/parse failure: don't race a refresh while ownership is unknown.
    let value=match read{Ok(v)=>v,Err(e)=>return Some(Err(e.into()))};
    let Some(accounts)=value["accounts"].as_object() else {return Some(Err("Original Google credential owner store is invalid".into()));};
    let owner=accounts.iter().find_map(|(key,a)|{
        let auth=&a["auth_json"];
        let google=a["kind"]=="antigravity_oauth" || auth["provider"]=="antigravity";
        if google!=project.is_some() || a["kind"]=="relay" || a["kind"]=="openai_key" {return None;}
        let same_refresh=refresh.filter(|r|!r.is_empty()).is_some_and(|r|
            auth.pointer("/tokens/refresh_token").and_then(|v|v.as_str()).or(a["refresh_token"].as_str())==Some(r));
        ((same_refresh||key==id)&&project.is_none_or(|p|auth["project_id"].as_str()==Some(p))).then_some(auth)
    })?;
    Some((||{
        let access_token=owner.pointer("/tokens/access_token").and_then(|v|v.as_str()).filter(|v|!v.is_empty()).ok_or("Original Google owner has no access token")?.to_owned();
        let expires_at=owner.pointer("/tokens/expires_at").and_then(|v|v.as_str()).and_then(|v|DateTime::parse_from_rfc3339(v).ok()).map(|v|v.with_timezone(&Utc))
            .or_else(||access_token.split('.').nth(1).and_then(|payload|base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload).ok())
                .and_then(|bytes|serde_json::from_slice::<serde_json::Value>(&bytes).ok()).and_then(|v|v["exp"].as_i64()).and_then(|t|DateTime::from_timestamp(t,0)))
            .ok_or("Original OAuth lease has no expiry")?;
        if expires_at<=Utc::now()+chrono::Duration::seconds(30){return Err("Original access token expired; refresh that account in the original Switcher. The fork will not race its refresh token.".into());}
        Ok(Lease{access_token,expires_at})
    })())
}
