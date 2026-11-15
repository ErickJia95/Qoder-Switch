use crate::checkin::{self, CheckinResult};
use crate::error::AppResult;
use crate::state::AppState;
use tauri::State;

fn to_result(account_id: String, label: String, outcome: Result<checkin::CheckinOutcome, String>) -> CheckinResult {
    match outcome {
        Ok(checkin::CheckinOutcome::Claimed { amount, expires_at }) => CheckinResult {
            account_id,
            label,
            status: "claimed".into(),
            amount,
            expires_at,
            message: format!("领取成功 +{} Credits", amount),
            api_base: String::new(),
        },
        Ok(checkin::CheckinOutcome::Already) => CheckinResult {
            account_id,
            label,
            status: "already".into(),
            amount: 0,
            expires_at: String::new(),
            message: "今日已领取".into(),
            api_base: String::new(),
        },
        Ok(checkin::CheckinOutcome::NoCampaign) => CheckinResult {
            account_id,
            label,
            status: "no_campaign".into(),
            amount: 0,
            expires_at: String::new(),
            message: "无可用签到活动（国际版账号可能需先在官方客户端领取一次）".into(),
            api_base: String::new(),
        },
        Err(e) => CheckinResult {
            account_id,
            label,
            status: "error".into(),
            amount: 0,
            expires_at: String::new(),
            message: e,
            api_base: String::new(),
        },
    }
}

/// 对单个账号签到
#[tauri::command]
pub async fn checkin_account(state: State<'_, AppState>, id: String) -> AppResult<CheckinResult> {
    // 凭证快过期就先刷新（失败不阻断签到）
    if let Some(msg) =
        crate::auth::session::ensure_fresh(&state.token_manager, &state.refresh_locks, &id).await
    {
        log::info!("[checkin] 签到前已自动刷新凭证: {}", msg);
    }

    let accounts = state.token_manager.list().await;
    let account = accounts
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| crate::error::AppError::AccountNotFound(id.clone()))?;

    let outcome = checkin::checkin(&account.token).await;
    Ok(to_result(account.id.clone(), account.label.clone(), outcome))
}

/// 对所有账号逐个签到（避免并发限流）
#[tauri::command]
pub async fn checkin_all(state: State<'_, AppState>) -> AppResult<Vec<CheckinResult>> {
    let accounts = state.token_manager.list().await;
    let mut results = Vec::new();

    for account in &accounts {
        // 逐个账号签到前先兜底刷新（按账号加锁，不会并发轮换）
        if let Some(msg) = crate::auth::session::ensure_fresh(
            &state.token_manager,
            &state.refresh_locks,
            &account.id,
        )
        .await
        {
            log::info!("[checkin] 账号 {} 签到前已自动刷新: {}", account.label, msg);
        }
        // 刷新会改写库里的 token，必须取刷新后的最新值
        let token = state
            .token_manager
            .get(&account.id)
            .await
            .map(|a| a.token)
            .unwrap_or_else(|| account.token.clone());
        let outcome = checkin::checkin(&token).await;
        if let Err(e) = &outcome {
            log::warn!("[checkin] 账号 {} 签到失败: {}", account.label, e);
        }
        results.push(to_result(account.id.clone(), account.label.clone(), outcome));
    }

    Ok(results)
}
