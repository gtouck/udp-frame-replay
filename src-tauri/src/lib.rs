pub mod config;
pub mod parse;
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
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 应用启动失败");
}
