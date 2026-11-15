use crate::auth::token::AccountView;
use crate::error::{AppError, AppResult};
use crate::process::{self, IdeKind};
use crate::state::AppState;
use serde::Serialize;
use tauri::State;

#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> AppResult<Vec<AccountView>> {
    Ok(state.token_manager.list_views().await)
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

/// 只把账号标记为「当前使用账号」，不碰 IDE。
#[tauri::command]
pub async fn switch_account(state: State<'_, AppState>, id: String) -> AppResult<SwitchInfo> {
    if !state.token_manager.set_current(&id).await {
        return Err(AppError::AccountNotFound(id));
    }
    let account = state
        .token_manager
        .get(&id)
        .await
        .ok_or_else(|| AppError::AccountNotFound(id.clone()))?;
    Ok(SwitchInfo {
        id: account.id,
        label: account.label,
        email: account.email,
    })
}

#[derive(Serialize)]
pub struct ActivateInfo {
    pub id: String,
    pub label: String,
    pub email: String,
    pub ide_kind: String,
    /// true = 用的是完整登录数据（含 refreshToken）；false = 用旧版 token 拼的最小数据
    pub full_credentials: bool,
    /// 启动后回读 auth.v1.dat，确认 IDE 里确实是这个账号
    pub verified: bool,
    pub message: String,
}

/// 真正切换账号：退出对应 IDE → 写入该账号的登录态 → 重新打开。
/// 这是唯一会修改 IDE 文件的操作，写入前会把原 auth.v1.dat 备份到
/// ~/Library/Application Support/Qoder/auth-backup-<kind>.dat。
#[tauri::command]
pub async fn activate_account(state: State<'_, AppState>, id: String) -> AppResult<ActivateInfo> {
    let account = state
        .token_manager
        .get(&id)
        .await
        .ok_or_else(|| AppError::AccountNotFound(id.clone()))?;

    let kind_raw = if !account.ide_kind.is_empty() {
        account.ide_kind.clone()
    } else {
        "cn".to_string()
    };
    let ide = IdeKind::parse(&kind_raw)
        .ok_or_else(|| AppError::Config(format!("未知的 IDE 版本: {}", kind_raw)))?;

    if !process::is_installed(ide) {
        return Err(AppError::Config(format!(
            "未安装 {}，无法切换",
            ide.app_name()
        )));
    }

    // 只接受完整登录数据（含 refreshToken）。实测证明：用 token 拼的「最小数据」会被
    // IDE AuthStore 启动校验判为 invalid 并直接清除（日志: Credential storage diagnostic
    // result:invalid stage:validate），表现为切换后掉登录、要重新登录，比不切换更糟。
    let (blob, _full_credentials) = if !account.auth_blob.is_empty() {
        (account.auth_blob.clone(), true)
    } else {
        return Err(AppError::Auth(format!(
            "「{}」没有完整登录数据（缺少 refreshToken），写入后 IDE 会判定无效并要求重新登录。\
请先在 Qoder IDE 里登录该账号，再回到本工具点「读取当前登录账号」补齐，之后即可一键切换。",
            account.label
        )));
    };
    let full_credentials = true;

    let kind_for_task = kind_raw.clone();
    let label = account.label.clone();
    let app_name = ide.app_name().to_string();

    let write_result = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        // IDE 必须先退出，否则它会在退出时用自己的状态覆盖我们写入的文件
        if !process::quit(ide) {
            return Err(format!(
                "{} 未能退出（可能被未保存的对话框阻塞），已取消切换",
                ide.app_name()
            ));
        }
        crate::local_account::write_auth(&kind_for_task, &blob)?;
        process::launch(ide)
    })
    .await
    .map_err(|e| AppError::Config(format!("切换任务执行失败: {}", e)))?;

    if let Err(e) = write_result {
        return Err(AppError::Auth(e));
    }

    state.token_manager.set_current(&id).await;

    // 等 IDE 起来后回读 auth.v1.dat，确认切换真的生效（也顺手收下 IDE 可能刷新过的新 token）
    tokio::time::sleep(std::time::Duration::from_secs(6)).await;
    let mut verified = false;
    let mut message = format!("已切换到「{}」并重启 {}", label, app_name);

    if let Some(latest) = crate::local_account::read_account_for(&kind_raw) {
        let same_user = !latest.user_id.is_empty() && latest.user_id == account.user_id;
        let same_token = !latest.token.is_empty() && latest.token == account.token;
        if same_user || same_token {
            verified = true;
            let expire = latest.expires_at.chars().take(10).collect::<String>();
            state
                .token_manager
                .refresh_credentials(
                    &id,
                    latest.token.clone(),
                    expire,
                    latest.auth_json.clone(),
                    kind_raw.clone(),
                )
                .await;
        } else {
            message = format!(
                "已写入「{}」的登录态并重启 {}，但 IDE 里仍显示另一个账号（{}）。可能被 IDE 的会话缓存覆盖，可用「恢复切换前的登录态」回退后重试。",
                label,
                app_name,
                if latest.name.is_empty() { "未知".to_string() } else { latest.name }
            );
        }
    } else {
        message = format!(
            "已写入「{}」的登录态并重启 {}（暂时读不到 IDE 的登录文件，无法确认，请看一眼 IDE 右上角账号）",
            label, app_name
        );
    }

    log::info!(
        "[activate] 已切换到 {}（{}，full_credentials={}，verified={}）",
        label,
        kind_raw,
        full_credentials,
        verified
    );

    Ok(ActivateInfo {
        id: account.id,
        label: label.clone(),
        email: account.email,
        ide_kind: kind_raw,
        full_credentials,
        verified,
        message,
    })
}

/// 读取本机 Qoder IDE 当前登录账号并导入（同 user_id 则刷新 token 与登录数据）
#[derive(Serialize)]
pub struct LocalAccountInfo {
    pub label: String,
    pub user_id: String,
    pub name: String,
    pub expires_at: String,
    pub ide_kind: String,
    pub updated: bool,
}

#[tauri::command]
pub async fn import_local_account(state: State<'_, AppState>) -> AppResult<LocalAccountInfo> {
    let acc = crate::local_account::read_current_account().map_err(crate::error::AppError::Auth)?;

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
            acc.auth_json,
            acc.ide_kind.clone(),
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
        "[local_account] 导入账号 label={}, user_id={}, kind={}, updated={}",
        label,
        acc.user_id,
        acc.ide_kind,
        updated
    );

    Ok(LocalAccountInfo {
        label,
        user_id: acc.user_id,
        name: acc.name,
        expires_at: acc.expires_at,
        ide_kind: acc.ide_kind,
        updated,
    })
}
