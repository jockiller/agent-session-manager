use std::path::PathBuf;
use crate::models::{ChatMessage, SessionSummary};

#[macro_export]
macro_rules! lazy_regex {
    ($name:ident, $pattern:expr) => {
        static $name: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new($pattern).unwrap());
    };
}

pub fn user_home() -> Option<PathBuf> {
    if let Ok(h) = std::env::var("HOME") {
        if !h.trim().is_empty() {
            return Some(PathBuf::from(h));
        }
    }
    if let Ok(up) = std::env::var("USERPROFILE") {
        if !up.trim().is_empty() {
            return Some(PathBuf::from(up));
        }
    }
    if let (Ok(drive), Ok(path)) = (std::env::var("HOMEDRIVE"), std::env::var("HOMEPATH")) {
        let combined = format!("{}{}", drive, path);
        if !combined.trim().is_empty() {
            return Some(PathBuf::from(combined));
        }
    }
    None
}

pub trait AgentAdapter: Send + Sync {
    fn platform_id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn scan_sessions(&self) -> Vec<SessionSummary>;
    fn load_messages(&self, session: &SessionSummary, max_msgs: usize) -> Vec<ChatMessage>;
    fn prune_indexes(&self, session_ids: &[String]) -> usize;
}

pub mod antigravity;
pub mod claude;
pub mod codebuddy;
pub mod codex;
pub mod cursor;
pub mod deepseek;
pub mod dsh;
pub mod gemini;
pub mod grok;
pub mod hermes;
pub mod kimi;
pub mod openclaw;
pub mod openclaude;
pub mod opencode;
pub mod pi;
pub mod qoder;
pub mod qwen;
pub mod trae;
pub mod workbuddy;
pub mod zcode;
