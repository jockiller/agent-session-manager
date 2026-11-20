use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use crate::config::skills::{
    self, DiscoveredDiskSkill, SkillEntry, SkillSyncOutcome,
};
use crate::config::skills_remote::{
    self, DiscoverOutcome, RemoteSkillCandidate, SkillRepo, SkillUpdateOutcome,
};

static SKILLS_LOCK: Mutex<()> = Mutex::new(());

fn current_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn all_roots(app: &AppHandle) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf), String> {
    let app_data = app.path().app_data_dir().map_err(|err| err.to_string())?;
    let cache_root = app.path().app_cache_dir().map_err(|err| err.to_string())?;
    Ok((
        app_data.clone(),
        cache_root,
        app_data.join("skills"),
        app_data.join("backups"),
    ))
}

#[tauri::command]
pub fn skills_list_repos(app: AppHandle) -> Result<Vec<SkillRepo>, String> {
    let app_data = app.path().app_data_dir().map_err(|err| err.to_string())?;
    Ok(skills_remote::load_repos(&app_data))
}

#[tauri::command]
pub fn skills_save_repos(
    app: AppHandle,
    repos: Vec<SkillRepo>,
) -> Result<Vec<SkillRepo>, String> {
    let app_data = app.path().app_data_dir().map_err(|err| err.to_string())?;
    skills_remote::save_repos(&app_data, repos)
}

#[tauri::command]
pub async fn skills_discover_remote(
    app: AppHandle,
    force_refresh: bool,
) -> Result<DiscoverOutcome, String> {
    let (app_data, cache_root, ssot_root, _) = all_roots(&app)?;
    tokio::task::spawn_blocking(move || {
        skills_remote::discover_remote_skills(&app_data, &cache_root, &ssot_root, force_refresh)
    })
    .await
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn skills_fetch_remote_url(
    app: AppHandle,
    input: String,
) -> Result<Vec<RemoteSkillCandidate>, String> {
    let (_, cache_root, ssot_root, _) = all_roots(&app)?;
    skills_remote::fetch_remote_url(&cache_root, &ssot_root, &input).await
}

#[tauri::command]
pub async fn skills_install_remote(
    app: AppHandle,
    candidate: RemoteSkillCandidate,
    target_agents: Vec<String>,
) -> Result<(SkillEntry, SkillSyncOutcome), String> {
    let (_, cache_root, ssot_root, backups_root) = all_roots(&app)?;
    tokio::task::spawn_blocking(move || {
        let _guard = SKILLS_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let env: HashMap<String, String> = std::env::vars().collect();
        skills_remote::install_remote_skill(
            &ssot_root,
            &cache_root,
            &current_home(),
            &env,
            &backups_root,
            &candidate,
            &target_agents,
        )
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn skills_update_remote(
    app: AppHandle,
    id: String,
) -> Result<SkillUpdateOutcome, String> {
    let (_, cache_root, ssot_root, backups_root) = all_roots(&app)?;
    let env: HashMap<String, String> = std::env::vars().collect();
    skills_remote::update_remote_skill(
        &ssot_root,
        &cache_root,
        &current_home(),
        &env,
        &backups_root,
        &id,
    )
    .await
}

fn skills_roots(app: &AppHandle) -> Result<(PathBuf, PathBuf), String> {
    let app_data = app.path().app_data_dir().map_err(|err| err.to_string())?;
    Ok((app_data.join("skills"), app_data.join("backups")))
}

#[tauri::command]
pub fn skills_list(app: AppHandle) -> Result<Vec<SkillEntry>, String> {
    let (ssot_root, _) = skills_roots(&app)?;
    let env: HashMap<String, String> = std::env::vars().collect();
    Ok(skills::list_skills(&ssot_root, &current_home(), &env))
}

#[tauri::command]
pub fn skills_save(app: AppHandle, id: String, content: String) -> Result<SkillEntry, String> {
    let _guard = SKILLS_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let (ssot_root, _) = skills_roots(&app)?;
    let env: HashMap<String, String> = std::env::vars().collect();
    skills::save_skill(&ssot_root, &current_home(), &env, &id, &content)
}

#[tauri::command]
pub fn skills_sync_links(
    app: AppHandle,
    id: String,
    target_agents: Vec<String>,
) -> Result<(SkillEntry, SkillSyncOutcome), String> {
    let _guard = SKILLS_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let (ssot_root, backups_root) = skills_roots(&app)?;
    let env: HashMap<String, String> = std::env::vars().collect();
    skills::sync_skill_links(
        &ssot_root,
        &current_home(),
        &env,
        &backups_root,
        &id,
        &target_agents,
    )
}

#[tauri::command]
pub fn skills_delete(app: AppHandle, id: String) -> Result<Vec<String>, String> {
    let _guard = SKILLS_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let (ssot_root, _) = skills_roots(&app)?;
    let env: HashMap<String, String> = std::env::vars().collect();
    skills::delete_skill(&ssot_root, &current_home(), &env, &id, true)
}

#[tauri::command]
pub fn skills_scan_disk(app: AppHandle) -> Result<Vec<DiscoveredDiskSkill>, String> {
    let (ssot_root, _) = skills_roots(&app)?;
    let env: HashMap<String, String> = std::env::vars().collect();
    Ok(skills::scan_disk_skills(&ssot_root, &current_home(), &env))
}

#[tauri::command]
pub fn skills_import_dir(
    app: AppHandle,
    source_path: String,
    skill_id: Option<String>,
    link_agents: Vec<String>,
) -> Result<(SkillEntry, SkillSyncOutcome), String> {
    let _guard = SKILLS_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let (ssot_root, backups_root) = skills_roots(&app)?;
    let env: HashMap<String, String> = std::env::vars().collect();
    skills::import_skill_dir(
        &ssot_root,
        &current_home(),
        &env,
        &backups_root,
        &source_path,
        skill_id.as_deref(),
        &link_agents,
    )
}
