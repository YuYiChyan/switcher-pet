//! User-initiated quota refresh. Preserves routing and existing token ownership.
use crate::{account::AccountStore, AppState};
use futures_util::{stream, StreamExt};
use tauri::{Emitter, Manager};

static RUNNING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Default, serde::Serialize)]
pub struct Report {
    pub busy: bool,
    pub refreshed: usize,
    pub failed: usize,
}

#[tauri::command]
pub async fn refresh_pet_quota(app: tauri::AppHandle) -> Result<Report, String> {
    let Ok(_guard) = RUNNING.try_lock() else {
        return Ok(Report { busy: true, ..Default::default() });
    };
    let accounts = {
        let state = app.state::<AppState>();
        let store = state.store.lock().map_err(|_| "额度状态暂时不可读")?;
        store.accounts.values().filter(|a| {
            let policy = crate::source_probe::policy_for(&store, &a.id);
            policy.included && !a.is_banned && !a.is_logged_out
                && ((a.is_relay() && policy.balance_adapter != "off")
                    || a.is_chatgpt_oauth() || a.is_antigravity_oauth())
        }).cloned().collect::<Vec<_>>()
    };
    let results = stream::iter(accounts.into_iter().map(|account| {
        let app = app.clone();
        async move {
            tokio::time::timeout(std::time::Duration::from_secs(45), async {
                if account.is_relay() {
                    return crate::source_probe::refresh_balance(&app, &account.id).await;
                }
                // Synthetic validation accounts must never reach real OAuth services.
                if std::env::args().any(|a| a == "--validation") {
                    return Err("Validation: OAuth quota networking disabled".into());
                }
                let state = app.state::<AppState>();
                if account.is_antigravity_oauth() {
                    return crate::remote_server::refresh_antigravity_quota_local(&state.store, &app, &account.id)
                        .await.map(|_| ());
                }
                // Borrow the owner's fresh access token where applicable; never rotate it.
                let mut token = AccountStore::extract_access_token(&account.auth_json).ok_or("缺少访问令牌")?;
                let refresh = account.auth_json.pointer("/tokens/refresh_token").and_then(|v| v.as_str())
                    .or(account.refresh_token.as_deref());
                if let Some(lease) = crate::antigravity::lease::original_openai_owner(&account.id, refresh) {
                    token = lease?.access_token;
                }
                let (display, _) = crate::usage::UsageFetcher::fetch_usage_direct(
                    token, AccountStore::extract_account_id(&account.auth_json),
                    None, false, Some(account.id.clone()),
                ).await?;
                let mut store = state.store.lock().map_err(|_| "额度缓存暂时不可写")?;
                let current = store.accounts.get_mut(&account.id).ok_or("来源已删除")?;
                if current.auth_json != account.auth_json { return Err("来源已更新，请重试".into()); }
                current.cached_quota = Some(crate::usage_to_cached(&display));
                store.save()
            }).await.is_ok_and(|r| r.is_ok())
        }
    })).buffer_unordered(3).collect::<Vec<_>>().await;
    let report = Report {
        busy: false,
        refreshed: results.iter().filter(|ok| **ok).count(),
        failed: results.iter().filter(|ok| !**ok).count(),
    };
    // Failed queries retain the last known quota, never fabricate a zero balance.
    let _ = app.emit("accounts-updated", ());
    let _ = app.emit("pet-quota-refreshed", &report);
    Ok(report)
}
