use crate::error::AppResult;
use crate::quota::{self, QuotaResult};
use crate::state::AppState;
use tauri::State;

fn to_result(
    account_id: String,
    label: String,
    snapshot: Result<quota::UsageSnapshot, String>,
) -> QuotaResult {
    match snapshot {
        Ok(s) => QuotaResult {
            account_id,
            label,
            plan_name: s.plan_name,
            user_type: s.user_type,
            quota_used: s.used,
            quota_total: s.total,
            quota_remaining: s.remaining,
            quota_unit: s.unit,
            usage_percentage: s.percentage,
            is_exceeded: s.is_exceeded,
            expire_date: s.expire_date,
            add_on_quota: s.add_on,
            api_base: s.api_base,
            error: None,
        },
        Err(e) => QuotaResult {
            account_id,
            label,
            plan_name: String::new(),
            user_type: String::new(),
            quota_used: 0.0,
            quota_total: 0.0,
            quota_remaining: 0.0,
            quota_unit: String::new(),
            usage_percentage: 0.0,
            is_exceeded: false,
            expire_date: String::new(),
            add_on_quota: None,
            api_base: String::new(),
            error: Some(e),
        },
    }
}

#[tauri::command]
pub async fn check_quota(state: State<'_, AppState>, id: String) -> AppResult<QuotaResult> {
    // 凭证快过期就先刷新（失败不阻断查询，旧凭证通常仍可用）
    if let Some(msg) = crate::auth::session::ensure_fresh(
        &state.token_manager,
        &state.refresh_locks,
        &id,
    )
    .await
    {
        log::info!("[quota] 查询前已自动刷新凭证: {}", msg);
    }

    let accounts = state.token_manager.list().await;
    let account = accounts
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| crate::error::AppError::AccountNotFound(id.clone()))?;

    let snapshot = quota::fetch_usage(&account.token).await;

    if let Ok(s) = &snapshot {
        let used = s.used.round() as u64;
        let total = s.total.round() as u64;
        state
            .token_manager
            .update_account_info(&account.id, s.user_type.clone(), s.expire_date.clone())
            .await;
        state
            .token_manager
            .update_account_quota(&account.id, Some(used), Some(total))
            .await;
    }

    Ok(to_result(account.id.clone(), account.label.clone(), snapshot))
}

#[tauri::command]
pub async fn check_all_quotas(state: State<'_, AppState>) -> AppResult<Vec<QuotaResult>> {
    let accounts = state.token_manager.list().await;
    let mut results = Vec::new();

    for account in &accounts {
        // 逐个查询，避免并发过多请求被限流
        // 每个账号查询前先兜底刷新（内部按账号加锁，不会并发轮换）
        if let Some(msg) = crate::auth::session::ensure_fresh(
            &state.token_manager,
            &state.refresh_locks,
            &account.id,
        )
        .await
        {
            log::info!("[quota] 账号 {} 查询前已自动刷新: {}", account.label, msg);
        }
        // 刷新会改写库里的 token，这里必须用刷新后的最新值，否则拿旧 token 查会 401
        let token = state
            .token_manager
            .get(&account.id)
            .await
            .map(|a| a.token)
            .unwrap_or_else(|| account.token.clone());
        let snapshot = quota::fetch_usage(&token).await;
        if let Ok(s) = &snapshot {
            state
                .token_manager
                .update_account_info(&account.id, s.user_type.clone(), s.expire_date.clone())
                .await;
            state
                .token_manager
                .update_account_quota(
                    &account.id,
                    Some(s.used.round() as u64),
                    Some(s.total.round() as u64),
                )
                .await;
        } else if let Err(e) = &snapshot {
            log::warn!("[quota] 账号 {} 查询失败: {}", account.label, e);
        }
        results.push(to_result(account.id.clone(), account.label.clone(), snapshot));
    }

    Ok(results)
}
