use crate::auth::token::AccountToken;
use crate::error::AppResult;
use crate::state::AppState;
use serde::Serialize;
use tauri::State;

#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> AppResult<Vec<AccountToken>> {
    Ok(state.token_manager.list().await)
}

#[tauri::command]
pub async fn remove_account(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.token_manager.remove(&id).await;
    Ok(())
}

#[derive(Serialize)]
pub struct SwitchInfo {
    pub id: String,
    pub label: String,
    pub email: String,
}

/// 标记某个账号为「当前使用账号」。
/// 本应用不再接管 IDE 的网络流量，切换身份需要在 Qoder IDE 里用该账号重新登录。
#[tauri::command]
pub async fn switch_account(state: State<'_, AppState>, id: String) -> AppResult<SwitchInfo> {
    if !state.token_manager.set_current(&id).await {
        return Err(crate::error::AppError::AccountNotFound(id));
    }
    let accounts = state.token_manager.list().await;
    let account = accounts
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| crate::error::AppError::AccountNotFound(id.clone()))?;
    Ok(SwitchInfo {
        id: account.id.clone(),
        label: account.label.clone(),
        email: account.email.clone(),
    })
}

/// 读取本机 Qoder IDE 当前登录账号并导入（同 user_id 则刷新 token）
#[derive(Serialize)]
pub struct LocalAccountInfo {
    pub label: String,
    pub user_id: String,
    pub name: String,
    pub expires_at: String,
    pub updated: bool,
}

#[tauri::command]
pub async fn import_local_account(state: State<'_, AppState>) -> AppResult<LocalAccountInfo> {
    let acc = crate::local_account::read_current_account()
        .map_err(crate::error::AppError::Auth)?;

    let label = if !acc.name.is_empty() {
        format!("本地 · {}", acc.name)
    } else if !acc.user_id.is_empty() {
        let short: String = acc.user_id.chars().take(8).collect();
        format!("本地 · {}", short)
    } else {
        "本地账号".to_string()
    };

    // expiresAt 形如 2026-12-14T13:07:35Z，界面只需要日期部分
    let expire_date: String = acc.expires_at.chars().take(10).collect();

    let updated = state
        .token_manager
        .upsert(
            acc.token,
            label.clone(),
            acc.user_id.clone(),
            acc.email.clone(),
            acc.name.clone(),
            expire_date,
        )
        .await;

    // IDE 里当前登录的就是这个账号，同步标记为「当前使用账号」
    if !acc.user_id.is_empty() {
        if let Some(found) = state
            .token_manager
            .list()
            .await
            .iter()
            .find(|a| a.user_id == acc.user_id)
        {
            state.token_manager.set_current(&found.id).await;
        }
    }

    log::info!(
        "[local_account] 导入账号 label={}, user_id={}, updated={}",
        label,
        acc.user_id,
        updated
    );

    Ok(LocalAccountInfo {
        label,
        user_id: acc.user_id,
        name: acc.name,
        expires_at: acc.expires_at,
        updated,
    })
}
