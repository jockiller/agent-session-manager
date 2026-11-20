use tauri::{AppHandle, Manager};

use crate::config::catalog_cache;
use crate::services::usage::db::{
    delete_pricing_override, get_currency_pref, list_pricing_overrides, open_usage_db,
    save_pricing_override, set_currency_pref, usage_db_path,
};
use crate::services::usage::pricing::normalize_model_id;
use crate::services::usage::query::{
    query_dashboard, query_logs, query_session_detail, query_today_summary,
};
use crate::services::usage::sync::sync_usage;
use crate::services::usage::{
    CurrencyPref, PricingOverrideRow, PricingRate, SessionUsageDetail, TodayUsageSummary,
    UsageDashboardData, UsageFilter, UsageLogPage, UsageSyncStatus,
};

#[tauri::command]
pub async fn usage_sync(app: AppHandle, force: Option<bool>) -> Result<UsageSyncStatus, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    catalog_cache::global().refresh_if_needed(&cache_dir).await;
    let force_flag = force.unwrap_or(false);
    tokio::task::spawn_blocking(move || sync_usage(&app_data, force_flag))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn usage_query_dashboard(
    app: AppHandle,
    filter: UsageFilter,
) -> Result<UsageDashboardData, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    catalog_cache::global().refresh_if_needed(&cache_dir).await;
    tokio::task::spawn_blocking(move || query_dashboard(&app_data, &filter))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn usage_today_summary(app: AppHandle) -> Result<TodayUsageSummary, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(move || query_today_summary(&app_data))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn usage_session_detail(
    app: AppHandle,
    platform: String,
    session_id: String,
) -> Result<Option<SessionUsageDetail>, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(move || query_session_detail(&app_data, &platform, &session_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn usage_query_logs(
    app: AppHandle,
    filter: UsageFilter,
    page: Option<usize>,
    page_size: Option<usize>,
) -> Result<UsageLogPage, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let p = page.unwrap_or(1);
    let ps = page_size.unwrap_or(50);
    tokio::task::spawn_blocking(move || query_logs(&app_data, &filter, p, ps))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn usage_list_pricing_overrides(app: AppHandle) -> Result<Vec<PricingOverrideRow>, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let conn = open_usage_db(&usage_db_path(&app_data))?;
    Ok(list_pricing_overrides(&conn))
}

#[tauri::command]
pub fn usage_save_pricing_override(
    app: AppHandle,
    model: String,
    rate: PricingRate,
) -> Result<(), String> {
    let norm = normalize_model_id(&model);
    if norm.is_empty() {
        return Err("Model ID cannot be empty".to_string());
    }
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let conn = open_usage_db(&usage_db_path(&app_data))?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    save_pricing_override(&conn, &norm, rate, now_ms)
}

#[tauri::command]
pub fn usage_delete_pricing_override(app: AppHandle, model: String) -> Result<(), String> {
    let norm = normalize_model_id(&model);
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let conn = open_usage_db(&usage_db_path(&app_data))?;
    delete_pricing_override(&conn, &norm)
}

#[tauri::command]
pub fn usage_get_prefs(app: AppHandle) -> Result<CurrencyPref, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let conn = open_usage_db(&usage_db_path(&app_data))?;
    Ok(get_currency_pref(&conn))
}

#[tauri::command]
pub fn usage_set_prefs(
    app: AppHandle,
    currency: String,
    rate: Option<f64>,
) -> Result<CurrencyPref, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let conn = open_usage_db(&usage_db_path(&app_data))?;
    set_currency_pref(&conn, &currency, rate)
}
