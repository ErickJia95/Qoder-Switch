use crate::error::AppResult;
use crate::quota::USAGE_PAGE_URL;
use crate::state::AppState;
use serde::Serialize;
use std::process::Command;
use tauri::State;

#[derive(Serialize)]
pub struct AppStatus {
    pub account_count: usize,
    pub current_account_id: Option<String>,
    pub current_label: Option<String>,
    pub ide_running: bool,
    pub usage_url: String,
}

#[tauri::command]
pub async fn get_app_status(state: State<'_, AppState>) -> AppResult<AppStatus> {
    let accounts = state.token_manager.list().await;
    let current = accounts.iter().find(|a| a.status == "current");
    Ok(AppStatus {
        account_count: accounts.len(),
        current_account_id: current.map(|a| a.id.clone()),
        current_label: current.map(|a| a.label.clone()),
        ide_running: crate::process::is_qoder_ide_running(),
        usage_url: USAGE_PAGE_URL.to_string(),
    })
}

/// 用系统默认浏览器打开外部链接（仅允许 https）
#[tauri::command]
pub fn open_external(url: String) -> AppResult<()> {
    if !url.starts_with("https://") {
        return Err(crate::error::AppError::Config("仅支持打开 https 链接".to_string()));
    }

    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg(&url);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", "", &url]);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(&url);
        c
    };

    cmd.spawn()
        .map_err(|e| crate::error::AppError::Config(format!("打开浏览器失败: {}", e)))?;
    Ok(())
}

/// 在系统文件管理器中定位文件（导出备份后方便用户找到产物）
#[tauri::command]
pub fn reveal_path(path: String) -> AppResult<()> {
    let target = std::path::PathBuf::from(&path);
    if !target.exists() {
        return Err(crate::error::AppError::Config("文件不存在".to_string()));
    }

    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg("-R").arg(&target);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("explorer");
        c.arg(format!("/select,{}", target.display()));
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = Command::new("xdg-open");
        if let Some(dir) = target.parent() {
            c.arg(dir);
        } else {
            c.arg(&target);
        }
        c
    };

    cmd.spawn()
        .map_err(|e| crate::error::AppError::Config(format!("打开文件管理器失败: {}", e)))?;
    Ok(())
}

/// 复制文本到系统剪贴板（用于复制账号邮箱等信息）
#[tauri::command]
pub fn copy_to_clipboard(text: String) -> AppResult<()> {
    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        let mut child = Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| crate::error::AppError::Config(format!("调用 pbcopy 失败: {}", e)))?;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(text.as_bytes()).ok();
        }
        let _ = child.wait();
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = text;
        Err(crate::error::AppError::Config(
            "当前系统暂不支持复制到剪贴板".to_string(),
        ))
    }
}
