use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::Value;

use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary, ToolCallItem};

pub struct GrokAdapter;

impl GrokAdapter {
    fn home_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("GROK_HOME") {
            PathBuf::from(dir)
        } else if let Some(home) = crate::adapters::user_home() {
            home.join(".grok")
        } else {
            PathBuf::from(".grok")
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

impl AgentAdapter for GrokAdapter {
    fn platform_id(&self) -> &'static str {
        "grok"
    }

    fn display_name(&self) -> &'static str {
        "xAI Grok"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let home = Self::home_dir();
        let sessions_dir = home.join("sessions");
        if !sessions_dir.exists() {
            return Vec::new();
        }

        let mut summaries = Vec::new();
        let mut subagent_parent_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        if let Ok(ws_entries) = fs::read_dir(sessions_dir) {
            for ws_entry in ws_entries.flatten() {
                let ws_path = ws_entry.path();
                if !ws_path.is_dir() || ws_path.file_name().map_or(false, |n| n.to_string_lossy().starts_with('.')) {
                    continue;
                }

                let ws_decoded = urlencoding::decode(&ws_path.file_name().unwrap().to_string_lossy()).unwrap_or_default().to_string();

                if let Ok(s_entries) = fs::read_dir(&ws_path) {
                    for s_entry in s_entries.flatten() {
                        let spath = s_entry.path();
                        if !spath.is_dir() {
                            continue;
                        }

                        let sid = spath.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                        let summary_file = spath.join("summary.json");
                        let chat_file = spath.join("chat_history.jsonl");
                        let usage_file = spath.join("usage.json");
                        let prompt_ctx_file = spath.join("prompt_context.json");

                        let mut title = String::new();
                        let cwd = ws_decoded.clone();
                        let mut model = "grok-build".to_string();
                        let mut created_at = 0i64;
                        let mut updated_at = 0i64;
                        let mut turn_count = 0u32;
                        let mut is_subagent = false;

                        if summary_file.exists() {
                            if let Ok(text) = fs::read_to_string(&summary_file) {
                                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                                    if let Some(t) = v.get("generated_title").or_else(|| v.get("session_summary")).and_then(Value::as_str) {
                                        title = t.trim().to_string();
                                    }
                                    if let Some(m) = v.get("current_model_id").and_then(Value::as_str) {
                                        model = m.to_string();
                                    }
                                    turn_count = v.get("num_messages").and_then(Value::as_u64).unwrap_or(0) as u32;
                                    if v.get("session_kind").and_then(Value::as_str) == Some("subagent") {
                                        is_subagent = true;
                                    }
                                }
                            }
                        }

                        if !is_subagent && prompt_ctx_file.exists() {
                            if let Ok(text) = fs::read_to_string(&prompt_ctx_file) {
                                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                                    if v.get("audience").and_then(Value::as_str) == Some("subagent") {
                                        is_subagent = true;
                                    }
                                }
                            }
                        }

                        // If non-subagent, check chat history for spawned subagent IDs
                        if !is_subagent && chat_file.exists() {
                            if let Ok(f) = File::open(&chat_file) {
                                let reader = BufReader::new(f);
                                let re_sub1 = regex::Regex::new(r"subagent_id:\s*([0-9a-fA-F-]+)").unwrap();
                                let re_sub2 = regex::Regex::new(r#""task_ids":\s*\[\s*"([0-9a-fA-F-]+)""#).unwrap();
                                for line in reader.lines().flatten() {
                                    if line.contains("subagent") || line.contains("task_ids") {
                                        for cap in re_sub1.captures_iter(&line) {
                                            if let Some(sub_id) = cap.get(1) {
                                                subagent_parent_map.insert(sub_id.as_str().to_string(), sid.clone());
                                            }
                                        }
                                        for cap in re_sub2.captures_iter(&line) {
                                            if let Some(sub_id) = cap.get(1) {
                                                subagent_parent_map.insert(sub_id.as_str().to_string(), sid.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Parse token stats from usage.json
                        let mut token_stats = None;
                        if usage_file.exists() {
                            if let Ok(text) = fs::read_to_string(&usage_file) {
                                if let Ok(uv) = serde_json::from_str::<Value>(&text) {
                                    if let Some(sess) = uv.get("session") {
                                        let in_tokens = sess.get("inputTokens").and_then(Value::as_u64).unwrap_or(0);
                                        let out_tokens = sess.get("outputTokens").and_then(Value::as_u64).unwrap_or(0);
                                        let cache_read = sess.get("cachedReadTokens").and_then(Value::as_u64).unwrap_or(0);
                                        let reasoning = sess.get("reasoningTokens").and_then(Value::as_u64).unwrap_or(0);
                                        let total_tokens = sess.get("totalTokens").and_then(Value::as_u64).unwrap_or(in_tokens + out_tokens);
                                        token_stats = Some(crate::services::token_calc::make_stats_extended(
                                            &model,
                                            in_tokens,
                                            out_tokens,
                                            cache_read,
                                            0,
                                            reasoning,
                                            None,
                                        ));
                                        if let Some(ref mut ts) = token_stats {
                                            ts.total_tokens = total_tokens;
                                        }
                                    }
                                }
                            }
                        }

                        if title.is_empty() {
                            title = "(无标题)".to_string();
                        }
                        if updated_at == 0 {
                            if let Ok(m) = spath.metadata() {
                                if let Ok(mtime) = m.modified() {
                                    if let Ok(d) = mtime.duration_since(std::time::UNIX_EPOCH) {
                                        updated_at = d.as_millis() as i64;
                                        created_at = updated_at;
                                    }
                                }
                            }
                        }

                        let total_size = Self::path_size(&spath);
                        let main_path = if chat_file.exists() { chat_file.to_string_lossy().to_string() } else { spath.to_string_lossy().to_string() };

                        summaries.push(SessionSummary {
                            id: sid.clone(),
                            platform: "grok".to_string(),
                            flavor: model,
                            dirname: sid,
                            main_path,
                            all_paths: vec![spath.to_string_lossy().to_string()],
                            cwd,
                            title,
                            created_at,
                            updated_at,
                            size_bytes: total_size,
                            turn_count,
                            is_subagent,
                            parent_id: None,
                            is_running: false,
                            has_transcript: chat_file.exists(),
                            token_stats,
                        });
                    }
                }
            }
        }

        // Link parent_id for subagents
        for s in &mut summaries {
            if s.is_subagent && s.parent_id.is_none() {
                if let Some(pid) = subagent_parent_map.get(&s.id) {
                    s.parent_id = Some(pid.clone());
                }
            }
        }

        summaries
    }

    fn load_messages(&self, session: &SessionSummary, max_msgs: usize) -> Vec<ChatMessage> {
        let fpath = Path::new(&session.main_path);
        if !fpath.exists() {
            return Vec::new();
        }

        let mut msgs = Vec::new();
        if let Ok(file) = File::open(fpath) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                if let Ok(val) = serde_json::from_str::<Value>(&line) {
                    let role = val.get("type").or_else(|| val.get("role")).and_then(Value::as_str).unwrap_or("user");
                    let mut text = String::new();
                    let mut tool_calls = Vec::new();

                    if let Some(content) = val.get("content") {
                        if let Some(s) = content.as_str() {
                            text = s.to_string();
                        } else if let Some(arr) = content.as_array() {
                            for item in arr {
                                if let Some(t) = item.get("text").and_then(Value::as_str) {
                                    if !text.is_empty() {
                                        text.push('\n');
                                    }
                                    text.push_str(t);
                                } else if item.get("type").and_then(Value::as_str) == Some("tool_use") {
                                    let name = item.get("name").and_then(Value::as_str).unwrap_or("tool").to_string();
                                    tool_calls.push(ToolCallItem { name, args: None, output: None });
                                }
                            }
                        }
                    }

                    if !text.is_empty() || !tool_calls.is_empty() {
                        msgs.push(ChatMessage {
                            role: role.to_string(),
                            text,
                            time: "".to_string(),
                            msg_type: role.to_string(),
                            thinking: None,
                            tool_calls,
                        });
                        if msgs.len() >= max_msgs {
                            break;
                        }
                    }
                }
            }
        }

        msgs
    }

    fn prune_indexes(&self, session_ids: &[String]) -> usize {
        let home = Self::home_dir();
        let db_path = home.join("sessions").join("session_search.sqlite");
        let mut pruned = 0;

        if db_path.exists() {
            if let Ok(conn) = Connection::open(&db_path) {
                for sid in session_ids {
                    if let Ok(count) = conn.execute("DELETE FROM session_docs WHERE session_id = ?", [sid]) {
                        pruned += count;
                    }
                }
                let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
            }
        }

        pruned
    }
}
