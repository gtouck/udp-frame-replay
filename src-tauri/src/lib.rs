pub mod config;
pub mod engine;
pub mod filter;
pub mod guess;
pub mod log;
pub mod mutate;
pub mod net;
pub mod parse;
pub mod preflight;
pub mod source;

mod commands;
mod state;

use state::AppState;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
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
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 应用启动失败");
}
