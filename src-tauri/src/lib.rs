pub mod auth;
mod checkin;
mod commands;
mod error;
pub mod local_account;
mod process;
pub mod quota;
mod state;
mod tray;

// 供 examples / 集成测试使用
pub use auth::token;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("Qoder");
            std::fs::create_dir_all(&data_dir).ok();

            let app_state = AppState::new(&data_dir);

            // 加载已保存的 tokens
            let tm = app_state.token_manager.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = tm.load().await {
                    log::error!("加载账号失败: {}", e);
                }
            });

            app.manage(app_state);
            tray::setup_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                window.hide().ok();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::account_cmd::list_accounts,
            commands::account_cmd::remove_account,
            commands::account_cmd::switch_account,
            commands::account_cmd::activate_account,
            commands::account_cmd::import_local_account,
            commands::backup_cmd::export_accounts,
            commands::backup_cmd::import_accounts,
            commands::ide_cmd::get_ide_status,
            commands::ide_cmd::restart_ide,
            commands::ide_cmd::restore_auth_backup,
            commands::quota_cmd::check_quota,
            commands::quota_cmd::check_all_quotas,
            commands::refresh_cmd::refresh_token,
            commands::refresh_cmd::refresh_all_tokens,
            commands::checkin_cmd::checkin_account,
            commands::checkin_cmd::checkin_all,
            commands::app_cmd::get_app_status,
            commands::app_cmd::open_external,
            commands::app_cmd::reveal_path,
            commands::app_cmd::copy_to_clipboard,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
