use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::config::agents::dsh;
use crate::config::apply::{self, ApplyOutcome, ApplyScope, DiscoveredDiskMcp};
use crate::config::catalog_cache;
use crate::config::catalog_client;
use crate::config::detector::AgentDetector;
use crate::config::fileio;
use crate::config::instructions::{
    self, InstructionBackupEntry, InstructionSyncOutcome, InstructionsState, TargetScanRow,
};
use crate::config::mcp_parser;
use crate::config::models::{AgentKind, AppDocument, MCPServer, ModelEntry, Provider};
use crate::config::profile;

static APPLY_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize)]
pub struct DetectedAgent {
    pub kind: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyResult {
    pub document: AppDocument,
    pub written: Vec<String>,
    pub backups: Vec<String>,
}

fn current_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn config_state_path(app_data: &Path) -> PathBuf {
    app_data.join("config-state.json")
}

fn load_from_dir(app_data: &Path) -> AppDocument {
    let mut document = fileio::load_document(&config_state_path(app_data));
    document.ensure_agent_defaults();
    document
}

fn sanitize_retired_provider(mut provider: Provider) -> Provider {
    provider.api_key.clear();
    provider.catalog_url.clear();
    provider.enabled = true;
    provider
}

fn reconcile_retired_providers(previous: &AppDocument, document: &mut AppDocument) {
    let active_ids: std::collections::HashSet<&str> = document
        .providers
        .iter()
        .map(|provider| provider.id.as_str())
        .collect();
    let mut by_id: std::collections::BTreeMap<String, Provider> = std::collections::BTreeMap::new();
    for provider in previous
        .retired_providers
        .iter()
        .chain(previous.providers.iter())
        .chain(document.retired_providers.iter())
    {
        if active_ids.contains(provider.id.as_str()) {
            continue;
        }
        by_id.insert(provider.id.clone(), sanitize_retired_provider(provider.clone()));
    }
    document.retired_providers = by_id.into_values().collect();
}

fn save_to_dir(app_data: &Path, document: &mut AppDocument) -> Result<(), String> {
    let state_path = config_state_path(app_data);
    if state_path.exists() {
        let previous = fileio::load_document(&state_path);
        reconcile_retired_providers(&previous, document);
    } else {
        reconcile_retired_providers(&AppDocument::default(), document);
    }
    fileio::write_document(document, &state_path).map_err(|err| err.message())
}

fn paths_as_strings(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

fn apply_blocking(
    app_data: PathBuf,
    kind: String,
    mut document: AppDocument,
) -> Result<ApplyResult, String> {
    let _guard = APPLY_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let parsed = AgentKind::from_raw(&kind).ok_or_else(|| format!("未知的 Agent：{kind}"))?;
    if !parsed.supports_providers() {
        return Err(format!(
            "{} 不支持自定义 Provider 导出，请在 MCP 或指令集栏目中同步",
            parsed.raw()
        ));
    }
    save_to_dir(&app_data, &mut document)?;
    let home = current_home();
    let env: HashMap<String, String> = std::env::vars().collect();
    let backups_root = app_data.join("backups");
    let ApplyOutcome { written, backups } =
        apply::apply_with_scope(&document, parsed, ApplyScope::ProvidersOnly, &home, &env, &backups_root)
            .map_err(|err| err.message())?;
    let stamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    document
        .applied_at
        .insert(parsed.raw().to_string(), stamp);
    save_to_dir(&app_data, &mut document)?;
    Ok(ApplyResult {
        document,
        written: paths_as_strings(&written),
        backups: paths_as_strings(&backups),
    })
}

fn sync_mcp_blocking(
    app_data: PathBuf,
    mut document: AppDocument,
) -> Result<ApplyResult, String> {
    let _guard = APPLY_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let state_path = config_state_path(&app_data);
    let prev_mcp_ids: Vec<String> = if state_path.exists() {
        fileio::load_document(&state_path)
            .library
            .mcp
            .into_iter()
            .map(|server| server.id)
            .collect()
    } else {
        Vec::new()
    };
    save_to_dir(&app_data, &mut document)?;
    let home = current_home();
    let env: HashMap<String, String> = std::env::vars().collect();
    let backups_root = app_data.join("backups");
    let ApplyOutcome { written, backups } =
        apply::sync_mcp_all(&document, &prev_mcp_ids, &home, &env, &backups_root)
            .map_err(|err| err.message())?;
    Ok(ApplyResult {
        document,
        written: paths_as_strings(&written),
        backups: paths_as_strings(&backups),
    })
}

pub async fn pull_catalog_for(
    provider: Provider,
    cache_dir: &Path,
) -> Result<Vec<ModelEntry>, String> {
    let raw_url = if provider.catalog_url.is_empty() {
        provider.base_url.as_str()
    } else {
        provider.catalog_url.as_str()
    };
    let auth_key = provider.api_key.trim();
    if auth_key.is_empty() {
        return Err("先填 API Key".to_string());
    }
    let models = catalog_client::fetch_models(raw_url, auth_key, auth_key, provider.api)
        .await
        .map_err(|err| err.message())?;
    catalog_cache::global().refresh_if_needed(cache_dir).await;
    let index = catalog_cache::global().snapshot();
    let previous: HashMap<String, ModelEntry> = provider
        .models
        .iter()
        .map(|model| (model.id.clone(), model.clone()))
        .collect();
    Ok(models
        .into_iter()
        .map(|fetched| {
            let enriched = profile::enrich(&fetched, &index);
            match previous.get(&fetched.id) {
                Some(existing) => profile::merge(&enriched, existing),
                None => enriched,
            }
        })
        .collect())
}

#[tauri::command]
pub fn config_load(app: AppHandle) -> Result<AppDocument, String> {
    let app_data = app.path().app_data_dir().map_err(|err| err.to_string())?;
    let mut document = load_from_dir(&app_data);
    let env: HashMap<String, String> = std::env::vars().collect();
    apply::reconcile_disk_mcp(&mut document, &current_home(), &env);
    Ok(document)
}

#[tauri::command]
pub fn config_save(app: AppHandle, mut document: AppDocument) -> Result<(), String> {
    let app_data = app.path().app_data_dir().map_err(|err| err.to_string())?;
    save_to_dir(&app_data, &mut document)
}

#[tauri::command]
pub async fn config_apply(
    app: AppHandle,
    kind: String,
    document: AppDocument,
) -> Result<ApplyResult, String> {
    let app_data = app.path().app_data_dir().map_err(|err| err.to_string())?;
    tokio::task::spawn_blocking(move || apply_blocking(app_data, kind, document))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn config_sync_mcp(
    app: AppHandle,
    document: AppDocument,
) -> Result<ApplyResult, String> {
    let app_data = app.path().app_data_dir().map_err(|err| err.to_string())?;
    tokio::task::spawn_blocking(move || sync_mcp_blocking(app_data, document))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub fn config_scan_disk_mcp(document: AppDocument) -> Vec<DiscoveredDiskMcp> {
    let env: HashMap<String, String> = std::env::vars().collect();
    apply::scan_all_disk_mcp(&document, &current_home(), &env)
}

#[tauri::command]
pub fn config_detect() -> Vec<DetectedAgent> {
    let detector = AgentDetector::live();
    detector
        .ordered()
        .into_iter()
        .map(|kind| DetectedAgent {
            kind: kind.raw().to_string(),
            installed: detector.is_installed(kind),
        })
        .collect()
}

#[tauri::command]
pub fn config_dsh_profiles() -> Vec<String> {
    dsh::discovered(&current_home())
}

#[tauri::command]
pub async fn config_pull_catalog(
    app: AppHandle,
    provider: Provider,
) -> Result<Vec<ModelEntry>, String> {
    let cache_dir = app.path().app_cache_dir().map_err(|err| err.to_string())?;
    pull_catalog_for(provider, &cache_dir).await
}

#[tauri::command]
pub async fn config_search_models(
    app: AppHandle,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<ModelEntry>, String> {
    let cache_dir = app.path().app_cache_dir().map_err(|err| err.to_string())?;
    catalog_cache::global().refresh_if_needed(&cache_dir).await;
    Ok(catalog_cache::global().search(&query, limit.unwrap_or(50).clamp(1, 200)))
}

#[tauri::command]
pub async fn config_refresh_models_dev(app: AppHandle) -> Result<usize, String> {
    let cache_dir = app.path().app_cache_dir().map_err(|err| err.to_string())?;
    catalog_cache::global().force_refresh(&cache_dir).await
}

#[tauri::command]
pub async fn config_check_compat(provider: Provider) -> Result<String, String> {
    catalog_client::check_compat(&provider).await
}

#[tauri::command]
pub fn config_parse_mcp(text: String) -> Result<Vec<MCPServer>, String> {
    mcp_parser::parse(&text).map_err(|err| err.message().to_string())
}

#[tauri::command]
pub fn instructions_scan_targets(state: InstructionsState) -> Vec<TargetScanRow> {
    let home = current_home();
    let env: HashMap<String, String> = std::env::vars().collect();
    instructions::scan_targets(&state, &home, &env)
}

#[tauri::command]
pub fn instructions_read_target(
    state: InstructionsState,
    target_id: String,
) -> Result<String, String> {
    let home = current_home();
    let env: HashMap<String, String> = std::env::vars().collect();
    instructions::read_target_file(&state, &target_id, &home, &env)
}

#[tauri::command]
pub fn instructions_sync_targets(
    app: AppHandle,
    mut document: AppDocument,
    target_ids: Vec<String>,
) -> Result<(AppDocument, InstructionSyncOutcome), String> {
    let _guard = APPLY_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let app_data = app.path().app_data_dir().map_err(|err| err.to_string())?;
    let home = current_home();
    let env: HashMap<String, String> = std::env::vars().collect();
    let backups_root = app_data.join("backups");
    let outcome = instructions::sync_targets(
        &mut document.instructions,
        &target_ids,
        &home,
        &env,
        &backups_root,
    )?;
    // Sync legacy library.instructions if a default preset exists for clean backward compat
    if let Some(def) = document.instructions.presets.first() {
        document.library.instructions = def.content.clone();
    }
    save_to_dir(&app_data, &mut document)?;
    Ok((document, outcome))
}

#[tauri::command]
pub fn instructions_list_backups(
    app: AppHandle,
    state: InstructionsState,
    target_id: String,
) -> Result<Vec<InstructionBackupEntry>, String> {
    let app_data = app.path().app_data_dir().map_err(|err| err.to_string())?;
    let home = current_home();
    let env: HashMap<String, String> = std::env::vars().collect();
    let backups_root = app_data.join("backups");
    Ok(instructions::list_backups(
        &state,
        &target_id,
        &home,
        &env,
        &backups_root,
    ))
}

#[tauri::command]
pub fn instructions_restore_backup(
    app: AppHandle,
    state: InstructionsState,
    target_id: String,
    backup_path: String,
) -> Result<String, String> {
    let _guard = APPLY_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let app_data = app.path().app_data_dir().map_err(|err| err.to_string())?;
    let home = current_home();
    let env: HashMap<String, String> = std::env::vars().collect();
    let backups_root = app_data.join("backups");
    instructions::restore_backup(
        &state,
        &target_id,
        Path::new(&backup_path),
        &home,
        &env,
        &backups_root,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::AgentKind;
    use std::path::{Path, PathBuf};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_test_{}_{}",
                name,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let _ = std::fs::create_dir_all(&path);
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn load_missing_ensures_agent_defaults() {
        let dir = TestDir::new("cfg_load");
        let document = load_from_dir(dir.path());
        for kind in AgentKind::ALL {
            assert!(
                document.agents.contains_key(kind.raw()),
                "{}",
                kind.raw()
            );
        }
    }

    #[test]
    fn load_broken_file_ensures_defaults() {
        let dir = TestDir::new("cfg_broken");
        std::fs::write(dir.path().join("config-state.json"), "not-json").unwrap();
        let document = load_from_dir(dir.path());
        assert_eq!(document.agents.len(), AgentKind::ALL.len());
    }

    #[test]
    fn parse_mcp_empty_message() {
        let err = config_parse_mcp(String::new()).unwrap_err();
        assert_eq!(err, "输入内容为空");
    }

    #[tokio::test]
    async fn pull_catalog_requires_api_key() {
        let provider = Provider::new("g", "GJ", "https://example.invalid");
        let err = pull_catalog_for(provider, Path::new("/tmp")).await.unwrap_err();
        assert_eq!(err, "先填 API Key");
    }

    #[test]
    fn save_tracks_removed_providers_in_retired_providers() {
        let dir = TestDir::new("cfg_retired_track");
        let mut initial = AppDocument::default();
        let mut removed = Provider::new("old_p", "Old", "https://old.example/v1");
        removed.api_key = "secret".into();
        let kept = Provider::new("kept", "Kept", "https://kept.example/v1");
        initial.providers = vec![removed, kept.clone()];
        save_to_dir(dir.path(), &mut initial).unwrap();

        let mut next = AppDocument::default();
        next.providers = vec![kept];
        save_to_dir(dir.path(), &mut next).unwrap();

        let reloaded = load_from_dir(dir.path());
        assert_eq!(reloaded.providers.len(), 1);
        assert_eq!(reloaded.retired_providers.len(), 1);
        assert_eq!(reloaded.retired_providers[0].id, "old_p");
        assert!(reloaded.retired_providers[0].api_key.is_empty());
    }
}
