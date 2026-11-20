pub mod db;
pub mod pricing;
pub mod query;
pub mod sync;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEvent {
    pub source_key: String,
    pub platform: String,
    pub session_id: String,
    pub workspace: String,
    pub model_raw: String,
    pub model_norm: String,
    pub ts_ms: i64,
    pub day_key: String,
    pub hour_key: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
    pub granularity: String,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub ttft_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingRate {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingOverrideRow {
    pub model_norm: String,
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrencyPref {
    pub currency: String,
    pub rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageSyncStatus {
    pub scanned_sources: usize,
    pub updated_sources: usize,
    pub pruned_sources: usize,
    pub total_events: usize,
    pub duration_ms: u64,
    pub synced_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageFilter {
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub platform: Option<String>,
    pub model: Option<String>,
    pub workspace: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageKpis {
    pub total_cost: f64,
    pub cache_savings: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
    pub total_tokens: u64,
    pub cache_hit_rate: f64,
    pub session_count: usize,
    pub event_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeseriesPoint {
    pub bucket: String,
    pub cost: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BreakdownRow {
    pub key: String,
    pub label: String,
    pub cost: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub total_tokens: u64,
    pub cache_hit_rate: f64,
    pub session_count: usize,
    pub event_count: usize,
    pub priced: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopSessionRow {
    pub session_id: String,
    pub platform: String,
    pub workspace: String,
    pub model: String,
    pub cost: f64,
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub last_ts_ms: i64,
    pub granularity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsageDetail {
    pub session_id: String,
    pub platform: String,
    pub cost: f64,
    pub cache_savings: f64,
    pub currency: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
    pub total_tokens: u64,
    pub cache_hit_rate: f64,
    pub models: Vec<String>,
    pub event_count: usize,
    pub granularity: String,
    pub priced: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboardData {
    pub kpis: UsageKpis,
    pub hourly: bool,
    pub timeseries: Vec<TimeseriesPoint>,
    pub by_model: Vec<BreakdownRow>,
    pub by_platform: Vec<BreakdownRow>,
    pub by_workspace: Vec<BreakdownRow>,
    pub top_sessions: Vec<TopSessionRow>,
    pub unpriced_models: Vec<String>,
    pub available_models: Vec<String>,
    pub available_workspaces: Vec<String>,
    pub currency: String,
    pub exchange_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayUsageSummary {
    pub day_key: String,
    pub total_cost: f64,
    pub total_tokens: u64,
    pub currency: String,
    pub has_priced_events: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLogRow {
    pub id: i64,
    pub ts_ms: i64,
    pub platform: String,
    pub session_id: String,
    pub workspace: String,
    pub model_raw: String,
    pub model_norm: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
    pub total_tokens: u64,
    pub cache_hit_rate: f64,
    pub cost: f64,
    pub priced: bool,
    pub duration_ms: u64,
    pub ttft_ms: u64,
    pub granularity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLogPage {
    pub items: Vec<UsageLogRow>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub currency: String,
}
