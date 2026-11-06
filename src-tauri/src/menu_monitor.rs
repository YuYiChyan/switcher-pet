//! Native macOS menu-bar monitor, using the fork's existing quota snapshot.
//! No helper binary, hard-coded home directory or auth-file writes.
use serde_json::Value;

pub fn title(data: &Value, scope: &str, weekly: bool, active_id: &str) -> String {
    let summary = match scope {
        "account" => data["accounts"].as_array().and_then(|rows| rows.iter().find(|r| r["id"] == active_id))
            .map(|r| &r["summary"]).unwrap_or(&Value::Null),
        "model" => &data["model_summary"],
        _ => &data["summary"],
    };
    let mut window = &summary["weekly"];
    let mut label = "周";
    if !weekly && summary["show_short"] == true && summary["short"]["capacity"].as_f64().unwrap_or(0.0) > 0.0 {
        window = &summary["short"]; label = "5h";
    }
    if scope == "model" && window["capacity"].as_f64().unwrap_or(0.0) == 0.0 {
        let key = if weekly { "model_weekly" } else { "model_short" };
        window = &data[key]; label = if weekly { "周" } else { "5h" };
        if window["capacity"].as_f64().unwrap_or(0.0) == 0.0 {
            window = &data["model_quota"]; label = "模型";
        }
    }
    let mut parts = Vec::new();
    if window["capacity"].as_f64().unwrap_or(0.0) > 0.0 {
        let text = if window["known"].as_u64().unwrap_or(0) > 0 {
            let marker = if window["unknown"].as_u64().unwrap_or(0) > 0 || window["stale"].as_u64().unwrap_or(0) > 0 { "*" } else { "" };
            format!("{label} ≈{:.0}%{marker}", window["remaining"].as_f64().unwrap_or(0.0))
        } else { format!("{label} —%") };
        parts.push(text);
    }
    // Never sum unlike currencies, and never show unrelated API balances in total scope.
    let balances = if scope == "account" { &summary["balances"] } else { &data["model_summary"]["balances"] };
    if let Some(values) = balances.as_object() {
        for (unit, amount) in values.iter().take(2) {
            if let Some(amount) = amount.as_f64() { parts.push(format!("{unit} {amount:.2}")); }
        }
    }
    if parts.is_empty() { parts.push("额度 —".into()); }
    parts.join(" · ")
}

#[cfg(target_os = "macos")]
pub fn start(app: tauri::AppHandle) {
    use tauri::Manager;
    tauri::async_runtime::spawn(async move {
        loop {
            let text = {
                let state = app.state::<crate::AppState>();
                let text = state.store.lock().ok().map(|store| {
                    let prefs = &store.settings.pet_preferences;
                    let active = crate::pet::get_pet_activity();
                    let id = if active.account_id.is_empty() { store.current.as_deref().unwrap_or("") } else { &active.account_id };
                    title(&crate::pool_api::dashboard(&store), &prefs.menu_scope, prefs.menu_weekly, id)
                });
                text
            };
            // The store lock must be dropped before calling Cocoa.
            if let (Some(tray), Some(text)) = (app.tray_by_id("main"), text) {
                let _ = tray.set_title(Some(&text));
                let _ = tray.set_tooltip(Some("Switcher Pet · * 表示部分未知或上次记录；右键切换统计范围"));
            }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn large_totals_and_five_hour_priority() {
        let d = json!({"summary":{"show_short":true,"short":{"capacity":500,"known":2,"remaining":420},"weekly":{"capacity":200,"known":2,"remaining":180}},
            "model_summary":{"balances":{"USD":12.5}}});
        assert_eq!(title(&d,"pool",false,""),"5h ≈420% · USD 12.50");
        assert_eq!(title(&d,"pool",true,""),"周 ≈180% · USD 12.50");
    }
    #[test]
    fn unrestricted_pool_weekly_and_account_isolation() {
        let d = json!({"summary":{"show_short":false,"weekly":{"capacity":2000,"known":2,"remaining":1500}},
            "model_summary":{"balances":{}},"accounts":[{"id":"one","summary":{"weekly":{"capacity":100,"known":1,"remaining":70},"balances":{"CNY":3}}}]});
        assert_eq!(title(&d,"pool",false,""),"周 ≈1500%");
        assert_eq!(title(&d,"account",false,"one"),"周 ≈70% · CNY 3.00");
        assert_eq!(title(&d,"account",false,"missing"),"额度 —");
    }
    #[test]
    fn unknown_is_not_zero_and_google_model_fallback() {
        let d = json!({"summary":{"weekly":{"capacity":100,"known":0,"unknown":1}},
            "model_quota":{"capacity":200,"known":2,"remaining":160}});
        assert_eq!(title(&d,"pool",false,""),"周 —%");
        assert_eq!(title(&d,"model",false,""),"模型 ≈160%");
    }
}
