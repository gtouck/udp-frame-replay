pub mod config;
pub mod engine;
pub mod filter;
pub mod guess;
pub mod log;
pub mod mutate;
pub mod net;
pub mod parse;
pub mod portable;
pub mod preflight;
pub mod source;

mod commands;
mod state;

use state::AppState;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        // 窗口在这儿建而不是让 Tauri 照配置自动建（`"create": false`），
        // 只为了插一句 data_directory —— 数据目录只能在建 webview 时指定。
        .setup(|app| {
            let config = app
                .config()
                .app
                .windows
                .first()
                .cloned()
                .expect("tauri.conf.json 里缺少窗口配置");

            let mut builder = tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?;
            if let Some(dir) = portable::data_dir() {
                builder = builder.data_directory(dir);
            }
            builder.build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_file,
            commands::close_file,
            commands::file_info,
            commands::preview,
            commands::guess_parse,
            commands::network_interfaces,
            commands::start_send,
            commands::pause_send,
            commands::resume_send,
            commands::step_send,
            commands::stop_send,
            commands::engine_status,
            commands::recent_frames,
            commands::log_entries,
            commands::error_groups,
            commands::clear_log,
            commands::preflight_check,
            commands::save_profile,
            commands::load_profile,
            commands::app_dir,
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 应用启动失败");
}
