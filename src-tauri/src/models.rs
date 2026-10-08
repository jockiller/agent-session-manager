use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub platform: String,
    pub flavor: String,
    pub dirname: String,
    pub main_path: String,
    pub all_paths: Vec<String>,
    pub cwd: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub size_bytes: u64,
    pub turn_count: u32,
    pub is_subagent: bool,
    pub parent_id: Option<String>,
    pub is_running: bool,
    pub has_transcript: bool,
    pub token_stats: Option<TokenStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenStats {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
    pub total_tokens: u64,
    pub cache_hit_rate: Option<f64>,
    pub tokens_per_second: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallItem {
    pub name: String,
    pub args: Option<String>,
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub text: String,
    pub time: String,
    pub msg_type: String,
    pub thinking: Option<String>,
    pub tool_calls: Vec<ToolCallItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPlan {
    pub target_ids: Vec<String>,
    pub deletable_count: usize,
    pub skipped_running_count: usize,
    pub total_bytes_to_free: u64,
    pub associated_files_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CleanupResult {
    pub success: bool,
    pub removed_sessions: usize,
    pub failed_sessions: usize,
    pub freed_bytes: u64,
    pub indexes_pruned: usize,
    pub trash_method: String,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalStats {
    pub total_sessions: usize,
    pub main_sessions: usize,
    pub subagents: usize,
    pub total_bytes: u64,
    pub running_count: usize,
    pub orphan_count: usize,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_write_tokens: u64,
    pub total_tokens: u64,
    pub overall_cache_hit_rate: Option<f64>,
    pub platform_distribution: std::collections::HashMap<String, PlatformStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlatformStat {
    pub count: usize,
    pub bytes: u64,
    pub tokens: u64,
}
