use crate::auth::session::{self, RefreshOutcome};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use tauri::State;

/// 刷新单个账号的凭证（force=false 时仅在即将过期才真刷）
#[tauri::command]
pub async fn refresh_token(
    state: State<'_, AppState>,
    id: String,
    force: Option<bool>,
) -> AppResult<RefreshOutcome> {
    let force = force.unwrap_or(false);
    log::info!("[refresh] 收到刷新请求 account_id={} force={}", id, force);

    session::refresh_account(&state.token_manager, &state.refresh_locks, &id, force)
        .await
        .map_err(|e| {
            log::error!("[refresh] 账号 {} 刷新失败: {}", id, e);
            AppError::Auth(e)
        })
    .inspect(|outcome| {
        log::info!(
            "[refresh] 账号 {} 结果 {}（{}）",
            id, outcome.status, outcome.expire_date
        );
    })
}

/// 批量刷新所有账号凭证
#[tauri::command]
pub async fn refresh_all_tokens(
    state: State<'_, AppState>,
    force: Option<bool>,
) -> AppResult<Vec<RefreshOutcome>> {
    let force = force.unwrap_or(false);
    log::info!("[refresh] 收到批量刷新请求 force={}", force);
    Ok(session::refresh_all(&state.token_manager, &state.refresh_locks, force).await)
}
