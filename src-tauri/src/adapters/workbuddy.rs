use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::DateTime;
use serde_json::Value;

use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary};
use crate::services::token_calc::make_stats_extended;

pub struct WorkBuddyAdapter;

impl WorkBuddyAdapter {
    fn home_dir() -> PathBuf {
        if let Some(home) = crate::adapters::user_home() {
            home.join(".workbuddy")
        } else {
            PathBuf::from(".workbuddy")
        }
    }

    fn extract_text_from_content(content_val: &Value) -> String {
        if let Some(s) = content_val.as_str() {
            return s.to_string();
        }
        if let Some(arr) = content_val.as_array() {
            let mut parts = Vec::new();
            for item in arr {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        parts.push(text);
                    }
                }
            }
            return parts.join("\n");
        }
        String::new()
    }
}

impl AgentAdapter for WorkBuddyAdapter {
    fn platform_id(&self) -> &'static str {
        "workbuddy"
    }

    fn display_name(&self) -> &'static str {
        "WorkBuddy"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let home = Self::home_dir();
        if !home.exists() {
            return Vec::new();
        }

        let mut summaries = Vec::new();
        let mut seen_ids = HashSet::new();

        let scan_roots = vec![home.join("projects"), home.clone()];

        for root in scan_roots {
            if !root.exists() {
                continue;
            }

            for entry in walkdir::WalkDir::new(&root).into_iter().flatten() {
                let path = entry.path();
                if !path.is_file() || path.extension().map_or(true, |ext| ext != "jsonl") {
                    continue;
                }

                // Check if it's subagent
                let path_str = path.to_string_lossy();
                let is_subagent = path_str.contains("/subagents/") || path_str.contains("\\subagents\\");
                let parent_id = if is_subagent {
                    path.parent()
                        .and_then(|p| p.parent())
                        .and_then(|p| p.file_name())
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                } else {
                    None
                };

                let raw_id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                if raw_id.is_empty() {
                    continue;
                }

                let session_unique_id = if is_subagent {
                    format!("{}:subagent:{}", parent_id.clone().unwrap_or_default(), raw_id)
                } else {
                    raw_id.clone()
                };

                if seen_ids.contains(&session_unique_id) {
                    continue;
                }
                seen_ids.insert(session_unique_id.clone());

                let meta = match fs::metadata(path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let file_size = meta.len();
                let mtime_sec = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);

                let mut title = String::new();
                let mut cwd = String::new();
                let mut created_at = mtime_sec;
                let mut updated_at = mtime_sec;
                let mut turn_count = 0u32;
                let mut prompt_tokens = 0u64;
                let mut completion_tokens = 0u64;
                let mut cache_read_tokens = 0u64;
                let mut cache_write_tokens = 0u64;
                let mut reasoning_tokens = 0u64;

                if let Ok(file) = File::open(path) {
                    let reader = BufReader::new(file);
                    for line in reader.lines().flatten() {
                        let line_str = line.trim();
                        if line_str.is_empty() {
                            continue;
                        }

                        if let Ok(val) = serde_json::from_str::<Value>(line_str) {
                            if let Some(c) = val.get("cwd").or_else(|| val.get("projectPath")).and_then(Value::as_str) {
                                if cwd.is_empty() {
                                    cwd = c.to_string();
                                }
                            }

                            if let Some(ts_val) = val.get("timestamp") {
                                if let Some(ms) = ts_val.as_i64() {
                                    let sec = ms / 1000;
                                    if created_at == mtime_sec || sec < created_at {
                                        created_at = sec;
                                    }
                                    if sec > updated_at {
                                        updated_at = sec;
                                    }
                                } else if let Some(s) = ts_val.as_str() {
                                    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                                        let sec = dt.timestamp();
                                        if created_at == mtime_sec || sec < created_at {
                                            created_at = sec;
                                        }
                                        if sec > updated_at {
                                            updated_at = sec;
                                        }
                                    }
                                }
                            }

                            let typ = val.get("type").and_then(Value::as_str).unwrap_or("");
                            if typ == "ai-title" {
                                if let Some(ai_title) = val.get("aiTitle").and_then(Value::as_str) {
                                    if !ai_title.trim().is_empty() {
                                        title = ai_title.trim().to_string();
                                    }
                                }
                            } else if typ == "message" {
                                turn_count += 1;
                                let role = val.get("role").and_then(Value::as_str).unwrap_or("");
                                if role == "user" && title.is_empty() {
                                    if let Some(content) = val.get("content") {
                                        let text = Self::extract_text_from_content(content);
                                        let trimmed = text.trim();
                                        if !trimmed.is_empty() {
                                            title = trimmed.to_string();
                                        }
                                    }
                                }
                            }

                            let usage_node = val.get("usage").or_else(|| {
                                val.get("providerData").and_then(|p| p.get("usage"))
                            });
                            if let Some(u) = usage_node {
                                prompt_tokens += u.get("input").or_else(|| u.get("promptTokens")).and_then(Value::as_u64).unwrap_or(0);
                                completion_tokens += u.get("output").or_else(|| u.get("completionTokens")).and_then(Value::as_u64).unwrap_or(0);
                                cache_read_tokens += u.get("cacheRead").or_else(|| u.get("cachedInputTokens")).and_then(Value::as_u64).unwrap_or(0);
                                cache_write_tokens += u.get("cacheWrite").or_else(|| u.get("cacheCreationInputTokens")).and_then(Value::as_u64).unwrap_or(0);
                                reasoning_tokens += u.get("reasoning").or_else(|| u.get("reasoningTokens")).and_then(Value::as_u64).unwrap_or(0);
                            }
                        }
                    }
                }

                if turn_count == 0 {
                    continue;
                }

                if title.is_empty() {
                    title = format!("WorkBuddy 会话 {}", &raw_id[..raw_id.len().min(8)]);
                }

                let token_stats = if prompt_tokens + completion_tokens > 0 {
                    Some(make_stats_extended("workbuddy", prompt_tokens, completion_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens, None))
                } else {
                    None
                };

                let main_path_str = path.to_string_lossy().to_string();
                summaries.push(SessionSummary {
                    id: session_unique_id,
                    platform: "workbuddy".to_string(),
                    flavor: "workbuddy".to_string(),
                    dirname: cwd.clone(),
                    main_path: main_path_str.clone(),
                    all_paths: vec![main_path_str],
                    cwd,
                    title,
                    created_at,
                    updated_at,
                    size_bytes: file_size,
                    turn_count,
                    is_subagent,
                    parent_id,
                    is_running: false,
                    has_transcript: true,
                    token_stats,
                });
            }
        }

        summaries
    }

    fn load_messages(&self, session: &SessionSummary, max_msgs: usize) -> Vec<ChatMessage> {
        let path = Path::new(&session.main_path);
        if !path.exists() {
            return Vec::new();
        }

        let mut msgs = Vec::new();
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => return msgs,
        };

        let reader = BufReader::new(file);
        for line in reader.lines().flatten() {
            let line_str = line.trim();
            if line_str.is_empty() {
                continue;
            }

            if let Ok(val) = serde_json::from_str::<Value>(line_str) {
                if val.get("type").and_then(Value::as_str) == Some("message") {
                    let role = val.get("role").and_then(Value::as_str).unwrap_or("");
                    if role == "user" || role == "assistant" {
                        let text = val.get("content").map(Self::extract_text_from_content).unwrap_or_default();
                        if !text.trim().is_empty() {
                            let mut time_str = String::new();
                            if let Some(ts_val) = val.get("timestamp") {
                                if let Some(ms) = ts_val.as_i64() {
                                    if let Some(dt) = DateTime::from_timestamp_millis(ms) {
                                        time_str = dt.to_rfc3339();
                                    }
                                } else if let Some(s) = ts_val.as_str() {
                                    time_str = s.to_string();
                                }
                            }

                            msgs.push(ChatMessage {
                                role: role.to_string(),
                                text,
                                time: time_str,
                                msg_type: "text".to_string(),
                                thinking: None,
                                tool_calls: Vec::new(),
                            });
                        }
                    }
                }

                if msgs.len() >= max_msgs {
                    break;
                }
            }
        }

        msgs
    }

    fn prune_indexes(&self, _session_ids: &[String]) -> usize {
        0
    }
}
