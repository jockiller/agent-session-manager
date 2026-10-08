use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary, ToolCallItem};
use crate::services::token_calc::make_stats_extended;

pub struct QwenAdapter;

impl QwenAdapter {
    fn home_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("QWEN_HOME") {
            PathBuf::from(dir)
        } else if let Some(home) = crate::adapters::user_home() {
            home.join(".qwen")
        } else {
            PathBuf::from(".qwen")
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

impl AgentAdapter for QwenAdapter {
    fn platform_id(&self) -> &'static str {
        "qwen"
    }

    fn display_name(&self) -> &'static str {
        "Qwen Code"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let home = Self::home_dir();
        if !home.exists() {
            return Vec::new();
        }

        let mut summaries = Vec::new();
        let mut seen = HashSet::new();

        for entry in walkdir::WalkDir::new(&home).into_iter().flatten() {
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
            let mut model = "qwen-2.5-coder".to_string();
            let mut created_at = 0i64;
            let mut updated_at = 0i64;
            let mut turn_count = 0u32;
            let mut prompt_tokens = 0u64;
            let mut completion_tokens = 0u64;
            let mut cache_read_tokens = 0u64;

            if let Ok(file) = File::open(path) {
                let reader = BufReader::new(file);
                for line in reader.lines().flatten() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if let Ok(val) = serde_json::from_str::<Value>(&line) {
                        if let Some(c) = val.get("cwd").and_then(Value::as_str) {
                            if cwd.is_empty() {
                                cwd = c.to_string();
                            }
                        }
                        if let Some(id) = val.get("sessionId").or_else(|| val.get("id")).and_then(Value::as_str) {
                            sid = id.to_string();
                        }
                        if let Some(m) = val.get("model").and_then(Value::as_str) {
                            if !m.is_empty() {
                                model = m.to_string();
                            }
                        }

                        if let Some(ts) = val.get("timestamp") {
                            if let Some(n) = ts.as_i64() {
                                let ms = if n < 10_000_000_000 { n * 1000 } else { n };
                                if created_at == 0 || ms < created_at {
                                    created_at = ms;
                                }
                                if ms > updated_at {
                                    updated_at = ms;
                                }
                            } else if let Some(s) = ts.as_str() {
                                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                                    let ms = dt.timestamp_millis();
                                    if created_at == 0 || ms < created_at {
                                        created_at = ms;
                                    }
                                    if ms > updated_at {
                                        updated_at = ms;
                                    }
                                }
                            }
                        }

                        if let Some(usage) = val.get("usageMetadata") {
                            let p = usage.get("promptTokenCount").and_then(Value::as_u64).unwrap_or(0);
                            let c = usage.get("candidatesTokenCount").and_then(Value::as_u64).unwrap_or(0);
                            let cr = usage.get("cachedContentTokenCount").and_then(Value::as_u64).unwrap_or(0);
                            if p > prompt_tokens { prompt_tokens = p; }
                            completion_tokens += c;
                            if cr > cache_read_tokens { cache_read_tokens = cr; }
                        }

                        let role = val.get("role").or_else(|| val.get("type")).and_then(Value::as_str).unwrap_or("");
                        if role == "user" {
                            turn_count += 1;
                            if title.is_empty() {
                                if let Some(cnt) = val.get("content").or_else(|| val.get("text")).and_then(Value::as_str) {
                                    title = cnt.lines().next().unwrap_or("").trim().to_string();
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
            let token_stats = if prompt_tokens > 0 || completion_tokens > 0 {
                Some(make_stats_extended(&model, prompt_tokens, completion_tokens, cache_read_tokens, 0, 0, None))
            } else {
                None
            };

            summaries.push(SessionSummary {
                id: sid.clone(),
                platform: "qwen".to_string(),
                flavor: model,
                dirname: sid,
                main_path: path.to_string_lossy().to_string(),
                all_paths: vec![path.to_string_lossy().to_string()],
                cwd,
                title,
                created_at: crate::adapters::to_millis(created_at),
                updated_at: crate::adapters::to_millis(updated_at),
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
                    let role = val.get("role").or_else(|| val.get("type")).and_then(Value::as_str).unwrap_or("");
                    let time_str = val.get("timestamp").and_then(Value::as_str).unwrap_or("").to_string();

                    if role == "user" {
                        let text = val.get("content").or_else(|| val.get("text")).and_then(Value::as_str).unwrap_or("").to_string();
                        if !text.is_empty() {
                            messages.push(ChatMessage {
                                role: "user".to_string(),
                                text,
                                time: time_str,
                                msg_type: "user".to_string(),
                                thinking: None,
                                tool_calls: Vec::new(),
                            });
                        }
                    } else if role == "assistant" || role == "model" {
                        let text = val.get("content").or_else(|| val.get("text")).and_then(Value::as_str).unwrap_or("").to_string();
                        let mut tool_calls = Vec::new();
                        if let Some(calls) = val.get("toolCalls").and_then(Value::as_array) {
                            for c in calls {
                                let name = c.get("name").and_then(Value::as_str).unwrap_or("tool").to_string();
                                let args = c.get("args").map(|a| serde_json::to_string_pretty(a).unwrap_or_default());
                                tool_calls.push(ToolCallItem { name, args, output: None });
                            }
                        }

                        if !text.is_empty() || !tool_calls.is_empty() {
                            messages.push(ChatMessage {
                                role: "assistant".to_string(),
                                text,
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

        if messages.len() > max_msgs {
            messages = messages.split_off(messages.len() - max_msgs);
        }

        messages
    }

    fn prune_indexes(&self, _session_ids: &[String]) -> usize {
        0
    }
}
