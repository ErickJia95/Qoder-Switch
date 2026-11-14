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
            api_base: String::new(),
            error: Some(e),
        },
    }
}

#[tauri::command]
pub async fn check_quota(state: State<'_, AppState>, id: String) -> AppResult<QuotaResult> {
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
        let snapshot = quota::fetch_usage(&account.token).await;
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
