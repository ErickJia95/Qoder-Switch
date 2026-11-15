use crate::error::{AppError, AppResult};
use crate::process::{self, IdeKind};
use serde::Serialize;

#[derive(Serialize)]
pub struct IdeState {
    pub kind: String,
    pub installed: bool,
    pub running: bool,
}

#[derive(Serialize)]
pub struct IdeStatus {
    pub cn: IdeState,
    pub intl: IdeState,
}

fn state_of(kind: IdeKind) -> IdeState {
    IdeState {
        kind: kind.key().to_string(),
        installed: process::is_installed(kind),
        running: process::is_running(kind),
    }
}

#[tauri::command]
pub fn get_ide_status() -> IdeStatus {
    IdeStatus {
        cn: state_of(IdeKind::Cn),
        intl: state_of(IdeKind::Intl),
    }
}

#[derive(Serialize)]
pub struct RestartResult {
    pub kind: String,
    pub was_running: bool,
    pub stopped: bool,
    pub launched: bool,
    pub message: String,
}

/// 重启（未运行时则启动）指定版本的 Qoder：Qoder CN 传 "cn"，Qoder 传 "intl"。
/// 退出 + 启动全程可能耗时数秒，放到阻塞线程执行，避免卡住 IPC。
#[tauri::command]
pub async fn restart_ide(kind: String) -> AppResult<RestartResult> {
    let ide = IdeKind::parse(&kind)
        .ok_or_else(|| AppError::Config(format!("未知的 Qoder 版本: {}", kind)))?;

    let report = tauri::async_runtime::spawn_blocking(move || process::restart(ide))
        .await
        .map_err(|e| AppError::Config(format!("重启任务执行失败: {}", e)))?;

    log::info!(
        "[ide] kind={} was_running={} stopped={} launched={} msg={}",
        kind,
        report.was_running,
        report.stopped,
        report.launched,
        report.message
    );

    Ok(RestartResult {
        kind,
        was_running: report.was_running,
        stopped: report.stopped,
        launched: report.launched,
        message: report.message,
    })
}

#[derive(Serialize)]
pub struct RestoreResult {
    pub kind: String,
    pub message: String,
}

/// 把切换前的登录态备份写回 IDE 并重启（切换失败时的退路）
#[tauri::command]
pub async fn restore_auth_backup(kind: String) -> AppResult<RestoreResult> {
    let ide = IdeKind::parse(&kind)
        .ok_or_else(|| AppError::Config(format!("未知的 Qoder 版本: {}", kind)))?;

    let result = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        if !process::quit(ide) {
            return Err(format!("{} 未能退出，已取消恢复", ide.app_name()));
        }
        crate::local_account::restore_backup(ide.key())?;
        process::launch(ide)
    })
    .await
    .map_err(|e| AppError::Config(format!("恢复任务执行失败: {}", e)))?;

    result.map_err(AppError::Auth)?;

    Ok(RestoreResult {
        kind,
        message: "已恢复切换前的登录态并重启 IDE".to_string(),
    })
}
