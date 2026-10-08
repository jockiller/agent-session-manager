use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary, ToolCallItem};
use crate::services::token_calc::make_stats_extended;

pub struct PiAdapter {
    pub is_omp: bool,
}

impl PiAdapter {
    pub fn new_pi() -> Self {
        Self { is_omp: false }
    }

    pub fn new_omp() -> Self {
        Self { is_omp: true }
    }

    fn home_dir(&self) -> PathBuf {
        if self.is_omp {
            if let Ok(dir) = std::env::var("OMP_AGENT_DIR") {
                PathBuf::from(dir)
            } else if let Some(home) = crate::adapters::user_home() {
                home.join(".omp").join("agent")
            } else {
                PathBuf::from(".omp/agent")
            }
        } else {
            if let Ok(dir) = std::env::var("PI_CODING_AGENT_DIR") {
                PathBuf::from(dir)
            } else if let Some(home) = crate::adapters::user_home() {
                home.join(".pi").join("agent")
            } else {
                PathBuf::from(".pi/agent")
            }
        }
    }

    fn path_size(p: &Path) -> u64 {
        if p.is_file() {
            return p.metadata().map(|m| m.len()).unwrap_or(0);
        }
        if p.is_dir() {
            let mut total = 0;
            for entry in walkdir::WalkDir::new(p).into_iter().flatten() {
                if entry.file_type().is_file() {
                    total += entry.metadata().map(|m| m.len()).unwrap_or(0);
                }
            }
            return total;
        }
        0
    }
}

impl AgentAdapter for PiAdapter {
    fn platform_id(&self) -> &'static str {
        if self.is_omp { "omp" } else { "pi" }
    }

    fn display_name(&self) -> &'static str {
        if self.is_omp { "oh-my-pi (omp)" } else { "Pi Coding Agent" }
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let home = self.home_dir();
        let sessions_dir = home.join("sessions");
        if !sessions_dir.exists() {
            return Vec::new();
        }

        let mut summaries = Vec::new();
        let mut seen = HashSet::new();

        for entry in walkdir::WalkDir::new(&sessions_dir).into_iter().flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().map_or(true, |ext| ext != "jsonl") {
                continue;
            }

            let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if file_stem.is_empty() || seen.contains(file_stem) {
                continue;
            }
            seen.insert(file_stem.to_string());

            let mut sid = file_stem.to_string();
            let mut title = String::new();
            let mut cwd = String::new();
            let mut created_at = 0i64;
            let mut updated_at = 0i64;
            let mut turn_count = 0u32;
            let mut in_tokens = 0u64;
            let mut out_tokens = 0u64;
            let mut cache_read = 0u64;
            let mut cache_write = 0u64;
            let mut reasoning = 0u64;
            let mut model = if self.is_omp { "omp".to_string() } else { "pi".to_string() };

            if let Ok(file) = File::open(path) {
                let reader = BufReader::new(file);
                for line in reader.lines().flatten() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if let Ok(val) = serde_json::from_str::<Value>(&line) {
                        let ltype = val.get("type").and_then(Value::as_str).unwrap_or("");
                        if ltype == "session" {
                            if let Some(id) = val.get("id").and_then(Value::as_str) {
                                if !id.is_empty() {
                                    sid = id.to_string();
                                }
                            }
                            if let Some(c) = val.get("cwd").and_then(Value::as_str) {
                                cwd = c.to_string();
                            }
                            if let Some(ts) = val.get("timestamp") {
                                if let Some(n) = ts.as_i64() {
                                    let ms = if n < 10_000_000_000 { n * 1000 } else { n };
                                    created_at = ms;
                                    updated_at = ms;
                                } else if let Some(s) = ts.as_str() {
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                                        created_at = dt.timestamp_millis();
                                        updated_at = created_at;
                                    }
                                }
                            }
                        } else if ltype == "session_info" {
                            if let Some(name) = val.get("name").and_then(Value::as_str) {
                                if !name.trim().is_empty() {
                                    title = name.trim().to_string();
                                }
                            }
                        } else if ltype == "message" {
                            if let Some(msg) = val.get("message") {
                                let role = msg.get("role").and_then(Value::as_str).unwrap_or("");
                                if role == "user" {
                                    turn_count += 1;
                                    if title.is_empty() {
                                        if let Some(cnt) = msg.get("content").and_then(Value::as_str) {
                                            title = cnt.lines().next().unwrap_or("").trim().to_string();
                                        }
                                    }
                                } else if role == "assistant" {
                                    if let Some(m) = msg.get("model").and_then(Value::as_str) {
                                        if !m.is_empty() {
                                            model = m.to_string();
                                        }
                                    }
                                    if let Some(usage) = msg.get("usage") {
                                        in_tokens += usage.get("input").and_then(Value::as_u64).unwrap_or(0);
                                        out_tokens += usage.get("output").and_then(Value::as_u64).unwrap_or(0);
                                        cache_read += usage.get("cacheRead").and_then(Value::as_u64).unwrap_or(0);
                                        cache_write += usage.get("cacheWrite").and_then(Value::as_u64).unwrap_or(0);
                                        reasoning += usage.get("reasoning").and_then(Value::as_u64).unwrap_or(0);
                                    }
                                }
                            }
                            if let Some(ts) = val.get("timestamp").and_then(Value::as_i64) {
                                let ms = if ts < 10_000_000_000 { ts * 1000 } else { ts };
                                if ms > updated_at {
                                    updated_at = ms;
                                }
                            }
                        }
                    }
                }
            }

            if title.is_empty() {
                title = "(无标题)".to_string();
            }
            if updated_at == 0 {
                if let Ok(m) = path.metadata() {
                    if let Ok(mtime) = m.modified() {
                        if let Ok(d) = mtime.duration_since(std::time::UNIX_EPOCH) {
                            updated_at = d.as_millis() as i64;
                            created_at = updated_at;
                        }
                    }
                }
            }

            let size = Self::path_size(path);
            let token_stats = if in_tokens > 0 || out_tokens > 0 {
                Some(make_stats_extended(&model, in_tokens, out_tokens, cache_read, cache_write, reasoning, None))
            } else {
                None
            };

            summaries.push(SessionSummary {
                id: sid.clone(),
                platform: if self.is_omp { "omp".to_string() } else { "pi".to_string() },
                flavor: model,
                dirname: sid,
                main_path: path.to_string_lossy().to_string(),
                all_paths: vec![path.to_string_lossy().to_string()],
                cwd,
                title,
                created_at,
                updated_at,
                size_bytes: size,
                turn_count,
                is_subagent: false,
                parent_id: None,
                is_running: false,
                has_transcript: true,
                token_stats,
            });
        }

        summaries
    }

    fn load_messages(&self, session: &SessionSummary, max_msgs: usize) -> Vec<ChatMessage> {
        let fpath = Path::new(&session.main_path);
        if !fpath.exists() {
            return Vec::new();
        }

        let mut messages = Vec::new();
        if let Ok(file) = File::open(fpath) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<Value>(&line) {
                    if val.get("type").and_then(Value::as_str) == Some("message") {
                        if let Some(msg) = val.get("message") {
                            let role = msg.get("role").and_then(Value::as_str).unwrap_or("");
                            let time_str = val.get("timestamp").and_then(Value::as_str).unwrap_or("").to_string();

                            if role == "user" {
                                let mut text = String::new();
                                if let Some(content) = msg.get("content") {
                                    if let Some(s) = content.as_str() {
                                        text = s.to_string();
                                    } else if let Some(arr) = content.as_array() {
                                        for part in arr {
                                            if let Some(t) = part.get("text").and_then(Value::as_str) {
                                                text.push_str(t);
                                                text.push('\n');
                                            }
                                        }
                                    }
                                }
                                if !text.trim().is_empty() {
                                    messages.push(ChatMessage {
                                        role: "user".to_string(),
                                        text: text.trim().to_string(),
                                        time: time_str,
                                        msg_type: "user".to_string(),
                                        thinking: None,
                                        tool_calls: Vec::new(),
                                    });
                                }
                            } else if role == "assistant" {
                                let mut text = String::new();
                                let mut tool_calls = Vec::new();

                                if let Some(content) = msg.get("content") {
                                    if let Some(s) = content.as_str() {
                                        text = s.to_string();
                                    } else if let Some(arr) = content.as_array() {
                                        for part in arr {
                                            let ptype = part.get("type").and_then(Value::as_str).unwrap_or("");
                                            if ptype == "text" {
                                                if let Some(t) = part.get("text").and_then(Value::as_str) {
                                                    text.push_str(t);
                                                    text.push('\n');
                                                }
                                            } else if ptype == "toolCall" {
                                                let name = part.get("name").and_then(Value::as_str).unwrap_or("tool").to_string();
                                                let args = part.get("arguments").map(|a| serde_json::to_string_pretty(a).unwrap_or_default());
                                                tool_calls.push(ToolCallItem {
                                                    name,
                                                    args,
                                                    output: None,
                                                });
                                            }
                                        }
                                    }
                                }

                                if !text.trim().is_empty() || !tool_calls.is_empty() {
                                    messages.push(ChatMessage {
                                        role: "assistant".to_string(),
                                        text: text.trim().to_string(),
                                        time: time_str,
                                        msg_type: "assistant".to_string(),
                                        thinking: None,
                                        tool_calls,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        if messages.len() > max_msgs {
            messages = messages.split_off(messages.len() - max_msgs);
        }

        messages
    }

    fn prune_indexes(&self, _session_ids: &[String]) -> usize {
        0
    }
}
