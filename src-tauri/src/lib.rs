pub mod adapters;
pub mod commands;
pub mod config;
pub mod config_commands;
pub mod models;
pub mod services;
pub mod skills_commands;
pub mod usage_commands;

use commands::*;
use config_commands::*;
use skills_commands::*;
use usage_commands::*;

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
            open_app,
            open_backups_dir,
            open_terminal,
            run_in_terminal,
            calculate_global_stats,
            config_load,
            config_save,
            config_apply,
            config_sync_mcp,
            config_scan_disk_mcp,
            config_detect,
            config_dsh_profiles,
            config_pull_catalog,
            config_search_models,
            config_refresh_models_dev,
            config_check_compat,
            config_parse_mcp,
            instructions_scan_targets,
            instructions_read_target,
            instructions_sync_targets,
            instructions_list_backups,
            instructions_restore_backup,
            skills_list,
            skills_save,
            skills_sync_links,
            skills_delete,
            skills_scan_disk,
            skills_import_dir,
            skills_list_repos,
            skills_save_repos,
            skills_discover_remote,
            skills_fetch_remote_url,
            skills_install_remote,
            skills_update_remote,
            usage_sync,
            usage_query_dashboard,
            usage_query_logs,
            usage_today_summary,
            usage_session_detail,
            usage_list_pricing_overrides,
            usage_save_pricing_override,
            usage_delete_pricing_override,
            usage_get_prefs,
            usage_set_prefs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
