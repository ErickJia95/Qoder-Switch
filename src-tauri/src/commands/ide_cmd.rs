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
