use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::config::agents::dsh;
use crate::config::apply::{self, ApplyOutcome};
use crate::config::catalog_cache;
use crate::config::catalog_client;
use crate::config::detector::AgentDetector;
use crate::config::fileio;
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

fn save_to_dir(app_data: &Path, document: &AppDocument) -> Result<(), String> {
    fileio::write_document(document, &config_state_path(app_data)).map_err(|err| err.message())
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
    save_to_dir(&app_data, &document)?;
    let home = current_home();
    let env: HashMap<String, String> = std::env::vars().collect();
    let backups_root = app_data.join("backups");
    let ApplyOutcome { written, backups } =
        apply::apply(&document, parsed, &home, &env, &backups_root).map_err(|err| err.message())?;
    let stamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    document
        .applied_at
        .insert(parsed.raw().to_string(), stamp);
    save_to_dir(&app_data, &document)?;
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
    Ok(load_from_dir(&app_data))
}

#[tauri::command]
pub fn config_save(app: AppHandle, document: AppDocument) -> Result<(), String> {
    let app_data = app.path().app_data_dir().map_err(|err| err.to_string())?;
    save_to_dir(&app_data, &document)
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
pub async fn config_check_compat(provider: Provider) -> Result<String, String> {
    catalog_client::check_compat(&provider).await
}

#[tauri::command]
pub fn config_parse_mcp(text: String) -> Result<Vec<MCPServer>, String> {
    mcp_parser::parse(&text).map_err(|err| err.message().to_string())
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
        assert_eq!(document.agents.len(), 6);
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
}
