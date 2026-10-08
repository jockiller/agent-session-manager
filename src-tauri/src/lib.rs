pub mod adapters;
pub mod commands;
pub mod models;
pub mod services;

use commands::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            scan_all_sessions,
            get_session_messages,
            plan_cleanup_cmd,
            execute_cleanup_cmd,
            reveal_path,
            open_terminal,
            run_in_terminal,
            calculate_global_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
