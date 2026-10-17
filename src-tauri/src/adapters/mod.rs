use std::path::PathBuf;
use serde_json::Value;
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

/// Normalizes any timestamp (seconds or milliseconds) to millisecond precision
#[inline]
pub fn to_millis(ts: i64) -> i64 {
    if ts <= 0 {
        return 0;
    }
    if ts < 100_000_000_000 {
        ts * 1000
    } else {
        ts
    }
}

/// Max characters kept when storing the first user prompt on a SessionSummary
const FIRST_PROMPT_MAX_CHARS: usize = 200;

/// Extracts a bounded, cleaned first-user-prompt snippet from raw message text.
/// Returns None when nothing searchable remains after stripping wrapper noise.
pub fn make_first_prompt(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Prefer the tagged question when platforms compose system context around it
    let body = tagged_query(trimmed)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(trimmed);
    // Drop leading wrapper blocks (system-reminder / user_info / user_instructions / environment_context / rules)
    let body = WRAPPER_BLOCK_RE
        .captures(body)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(body);
    let no_tags = WRAPPER_TAG_RE.replace_all(body, " ");
    let normalized = no_tags.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() || normalized == "code" {
        return None;
    }
    Some(normalized.chars().take(FIRST_PROMPT_MAX_CHARS).collect())
}

fn tagged_query(text: &str) -> Option<&str> {
    let caps = USER_QUERY_RE.captures(text)?;
    if let Some(m) = caps.get(1) {
        if !m.as_str().trim().is_empty() {
            return Some(m.as_str());
        }
    }
    caps.get(2).map(|m| m.as_str())
}

/// Collects the first user prompt across a session's user messages.
/// A message carrying an explicit <user_query>/<user_request> tag wins over
/// earlier plain messages that turned out to be system-context wrappers.
#[derive(Default)]
pub struct FirstPromptTracker {
    plain: String,
    tagged: String,
    fed: usize,
}

/// Stop after this many user messages; the real question is always near the top
const FIRST_PROMPT_MAX_FEEDS: usize = 5;

impl FirstPromptTracker {
    /// Returns true once a tagged prompt has been captured or the feed cap is reached.
    pub fn feed(&mut self, raw: &str) -> bool {
        if self.tagged.is_empty() && self.fed < FIRST_PROMPT_MAX_FEEDS {
            self.fed += 1;
            if let Some(inner) = tagged_query(raw) {
                if let Some(fp) = make_first_prompt(inner) {
                    self.tagged = fp;
                    return true;
                }
            }
            if self.plain.is_empty() {
                if let Some(fp) = make_first_prompt(raw) {
                    self.plain = fp;
                }
            }
        }
        self.is_done()
    }

    pub fn is_done(&self) -> bool {
        !self.tagged.is_empty() || self.fed >= FIRST_PROMPT_MAX_FEEDS
    }

    pub fn finish(&self) -> String {
        if !self.tagged.is_empty() {
            self.tagged.clone()
        } else {
            self.plain.clone()
        }
    }
}

/// Extracts user-visible text from a message content value (plain string or typed parts).
/// Typed parts accepted: text / input_text; tool result parts are ignored.
pub fn user_text_from_content(content: &Value) -> Option<String> {
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    if let Some(arr) = content.as_array() {
        let mut parts = Vec::new();
        for item in arr {
            let itype = item.get("type").and_then(Value::as_str).unwrap_or("");
            if itype == "text" || itype == "input_text" {
                if let Some(t) = item.get("text").and_then(Value::as_str) {
                    if !t.trim().is_empty() {
                        parts.push(t);
                    }
                }
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n"))
        }
    } else {
        None
    }
}

crate::lazy_regex!(
    USER_QUERY_RE,
    r"(?is)<user_query\b[^>]*>([\s\S]*?)</user_query>|<user_request\b[^>]*>([\s\S]*?)</user_request>"
);
crate::lazy_regex!(
    WRAPPER_BLOCK_RE,
    r"(?is)^[\s\n\r]*(?:<system-reminder\b[^>]*>[\s\S]*?</system-reminder>[\s\n\r]*|<user_info\b[^>]*>[\s\S]*?</user_info>[\s\n\r]*|<user_instructions\b[^>]*>[\s\S]*?</user_instructions>[\s\n\r]*|<environment_context\b[^>]*>[\s\S]*?</environment_context>[\s\n\r]*)*([\s\S]*)$"
);
crate::lazy_regex!(
    WRAPPER_TAG_RE,
    r"</?[a-zA-Z][a-zA-Z0-9_.\-:]{0,60}>"
);

pub trait AgentAdapter: Send + Sync {
    fn platform_id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn scan_sessions(&self) -> Vec<SessionSummary>;
    fn load_messages(&self, session: &SessionSummary, max_msgs: usize) -> Vec<ChatMessage>;

    /// Remove the given sessions from any agent-side index or store.
    ///
    /// Returns how many of the requested sessions are confirmed gone from that store.
    fn prune_indexes(&self, session_ids: &[String]) -> usize;

    /// Whether the sessions live only in the agent's own store (database or index) instead of
    /// files on disk. For such agents the store rows are the only proof a session is gone, so
    /// the cleanup reports a failed deletion when `prune_indexes` does not remove them all.
    fn store_backed(&self) -> bool {
        false
    }

    /// Whether removing a session also removes its child sessions, as OpenCode's own delete does.
    /// The cleanup must not delete a parent while one of those children is still running.
    fn deletes_descendants(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_prompt_plain() {
        let fp = make_first_prompt("帮我修复构建失败\n的第二行").unwrap();
        assert_eq!(fp, "帮我修复构建失败 的第二行");
    }

    #[test]
    fn test_first_prompt_user_query_tag_beats_wrapper_text() {
        let raw = "You are Grok released by xAI. <user_info>\nOS: macos\n</user_info>\n<system-reminder>skills list</system-reminder>\n<user_query>\n把这个签名修好\n</user_query>";
        assert_eq!(make_first_prompt(raw).unwrap(), "把这个签名修好");
    }

    #[test]
    fn test_first_prompt_leading_system_reminder_stripped() {
        let raw = "<system-reminder>The following skills are available...</system-reminder>\n继续之前的工作";
        assert_eq!(make_first_prompt(raw).unwrap(), "继续之前的工作");
    }

    #[test]
    fn test_first_prompt_empty_and_code() {
        assert!(make_first_prompt("   \n\t").is_none());
        assert!(make_first_prompt("code").is_none());
    }

    #[test]
    fn test_first_prompt_truncated_to_200_chars() {
        let raw = "x".repeat(500);
        assert_eq!(make_first_prompt(&raw).unwrap().chars().count(), 200);
    }

    #[test]
    fn test_first_prompt_xml_tags_stripped() {
        assert_eq!(make_first_prompt("<b>bold</b> question").unwrap(), "bold question");
    }

    #[test]
    fn test_user_text_from_content_variants() {
        assert_eq!(user_text_from_content(&Value::String("hi".into())).unwrap(), "hi");
        let arr = serde_json::json!([
            {"type": "text", "text": "part1"},
            {"type": "image", "url": "x"},
            {"type": "input_text", "text": "part2"}
        ]);
        assert_eq!(user_text_from_content(&arr).unwrap(), "part1\npart2");
        let tool_result = serde_json::json!([{"type": "tool_result", "content": "out"}]);
        assert!(user_text_from_content(&tool_result).is_none());
    }

    #[test]
    fn test_first_prompt_tracker_prefers_tagged_over_earlier_wrapper() {
        let mut t = FirstPromptTracker::default();
        assert!(!t.feed("<user_info>OS: macos</user_info>\nsome context but no question"));
        assert!(!t.is_done());
        t.feed("<system-reminder>skills</system-reminder>\n<user_query>真正的问题</user_query>");
        assert!(t.is_done());
        assert_eq!(t.finish(), "真正的问题");
    }

    #[test]
    fn test_first_prompt_tracker_caps_feeds() {
        let mut t = FirstPromptTracker::default();
        for i in 0..4 {
            assert!(!t.feed(&format!("包装消息 {}", i)));
        }
        // 5th feed consumes the last slot and marks the tracker done
        assert!(t.feed("包装消息 4"));
        assert!(t.is_done());
        // Cap reached: a later tagged message is not consumed
        t.feed("<user_query>太晚了</user_query>");
        assert_eq!(t.finish(), "包装消息 0");
    }

    #[test]
    fn test_first_prompt_tracker_plain_fallback() {
        let mut t = FirstPromptTracker::default();
        t.feed("帮我把首页改成暗色主题");
        assert_eq!(t.finish(), "帮我把首页改成暗色主题");
        // A later plain message does not override the captured first one
        t.feed("第二句话");
        assert_eq!(t.finish(), "帮我把首页改成暗色主题");
    }
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
