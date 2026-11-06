//! Reauthorization is the same identity, not cross-account failover.
use serde_json::Value;

pub fn same_identity(a: &Value, b: &Value) -> bool {
    let field = |v: &Value, key: &str| {
        v.get(key).and_then(Value::as_str).filter(|s| !s.trim().is_empty()).map(str::to_owned)
    };
    a["provider"] == "antigravity" && b["provider"] == "antigravity"
        && field(a, "project_id").is_some()
        && field(a, "project_id") == field(b, "project_id")
        && field(a, "email").is_some()
        && field(a, "email").map(|s| s.to_ascii_lowercase())
            == field(b, "email").map(|s| s.to_ascii_lowercase())
}

/// Only supersede an expired imported record with a healthy, newer login.
pub fn supersedes(old: &Value, new: &Value, now: chrono::DateTime<chrono::Utc>) -> bool {
    let expiry = |v: &Value| v.pointer("/tokens/expires_at").and_then(Value::as_str)
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
    same_identity(old, new)
        && expiry(old).is_some_and(|t| t <= now)
        && expiry(new).is_some_and(|t| t > now + chrono::Duration::minutes(5))
        && new.pointer("/tokens/access_token").and_then(Value::as_str).is_some_and(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn reauthorization_requires_both_project_and_identity() {
        let a=json!({"provider":"antigravity","email":"a@example.invalid","project_id":"p"});
        assert!(same_identity(&a,&a));
        for b in [
            json!({"provider":"antigravity","email":"b@example.invalid","project_id":"p"}),
            json!({"provider":"antigravity","email":"a@example.invalid","project_id":"q"}),
            json!({"provider":"antigravity","email":"","project_id":"p"}),
            json!({"provider":"other","email":"a@example.invalid","project_id":"p"}),
        ] {assert!(!same_identity(&a,&b));}
    }
    #[test]
    fn expired_import_can_only_be_replaced_by_fresh_login() {
        let now=chrono::DateTime::parse_from_rfc3339("2026-11-06T04:00:00Z").unwrap().with_timezone(&chrono::Utc);
        let mut old=json!({"provider":"antigravity","email":"a@example.invalid","project_id":"p",
            "tokens":{"access_token":"synthetic","expires_at":"2026-11-06T03:00:00Z"}});
        let mut new=old.clone();new["tokens"]["expires_at"]=json!("2026-11-06T05:00:00Z");
        assert!(supersedes(&old,&new,now));
        assert!(!supersedes(&new,&old,now));
        old["tokens"]["expires_at"]=json!("2026-11-06T04:30:00Z");
        assert!(!supersedes(&old,&new,now));
    }
}
