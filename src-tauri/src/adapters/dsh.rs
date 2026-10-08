use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary};

pub struct DshAdapter;

impl DshAdapter {
    fn home_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("DSH_HOME") {
            PathBuf::from(dir)
        } else if let Some(home) = crate::adapters::user_home() {
            home.join(".dsh")
        } else {
            PathBuf::from(".dsh")
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

impl AgentAdapter for DshAdapter {
    fn platform_id(&self) -> &'static str {
        "dsh"
    }

    fn display_name(&self) -> &'static str {
        "DSH"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let home = Self::home_dir();
        let sessions_dir = home.join("sessions");
        if !sessions_dir.exists() {
            return Vec::new();
        }

        let mut summaries = Vec::new();
        if let Ok(ws_entries) = fs::read_dir(sessions_dir) {
            for ws_entry in ws_entries.flatten() {
                let ws_path = ws_entry.path();
                if !ws_path.is_dir() {
                    continue;
                }

                if let Ok(s_entries) = fs::read_dir(&ws_path) {
                    for s_entry in s_entries.flatten() {
                        let spath = s_entry.path();
                        if !spath.is_dir() {
                            continue;
                        }

                        let dirname = spath.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                        let sid = dirname.strip_prefix("session-").unwrap_or(&dirname).to_string();
                        let lock_path = spath.join("session.lock");
                        let is_running = lock_path.exists();

                        // Find log file
                        let log_names = ["session.v3.jsonl.zstd", "session.v2.jsonl.zstd", "session.jsonl.zstd"];
                        let log_file = log_names.iter().map(|n| spath.join(n)).find(|p| p.exists());

                        let mut title = String::new();
                        let mut cwd = String::new();
                        let mut created_at = 0i64;
                        let mut updated_at = 0i64;
                        let mut turn_count = 0u32;

                        let mut parent_id = None;
                        let mut is_subagent = false;

                        if let Some(ref lpath) = log_file {
                            if let Ok(file) = File::open(lpath) {
                                if let Ok(decoder) = zstd::stream::read::Decoder::new(file) {
                                    let reader = BufReader::new(decoder);
                                    for line in reader.lines().flatten() {
                                        if let Ok(val) = serde_json::from_str::<Value>(&line) {
                                            if cwd.is_empty() {
                                                if let Some(c) = val.get("cwd").and_then(Value::as_str) {
                                                    cwd = c.to_string();
                                                }
                                            }
                                            if created_at == 0 {
                                                if let Some(ca) = val.get("createdAt").and_then(Value::as_i64) {
                                                    created_at = ca;
                                                }
                                            }
                                            if let Some(t) = val.get("time").and_then(Value::as_i64) {
                                                updated_at = t;
                                            }

                                            if val.get("origin").and_then(Value::as_str) == Some("subagent") {
                                                is_subagent = true;
                                            }
                                            if let Some(parent) = val.get("parentSession").and_then(Value::as_str) {
                                                let p = parent.strip_prefix("session-").unwrap_or(parent);
                                                parent_id = Some(p.to_string());
                                                is_subagent = true;
                                            }

                                            let etype = val.get("type").and_then(Value::as_str).unwrap_or("");
                                            if etype == "session/title" {
                                                if let Some(data) = val.get("data") {
                                                    if let Some(t) = data.get("title").or_else(|| data.get("value")).and_then(Value::as_str) {
                                                        title = t.trim().to_string();
                                                    }
                                                }
                                            } else if etype == "user/message" {
                                                turn_count += 1;
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
                            if let Ok(meta) = spath.metadata() {
                                if let Ok(mtime) = meta.modified() {
                                    if let Ok(d) = mtime.duration_since(std::time::UNIX_EPOCH) {
                                        updated_at = d.as_millis() as i64;
                                        created_at = updated_at;
                                    }
                                }
                            }
                        }

                        let total_size = Self::path_size(&spath);
                        let main_path = log_file.as_ref().map_or_else(|| spath.to_string_lossy().to_string(), |p| p.to_string_lossy().to_string());

                        summaries.push(SessionSummary {
                            id: sid,
                            platform: "dsh".to_string(),
                            flavor: "dsh".to_string(),
                            dirname,
                            main_path,
                            all_paths: vec![spath.to_string_lossy().to_string()],
                            cwd,
                            title,
                            created_at: crate::adapters::to_millis(created_at),
                            updated_at: crate::adapters::to_millis(updated_at),
                            size_bytes: total_size,
                            turn_count,
                            is_subagent,
                            parent_id,
                            is_running,
                            has_transcript: log_file.is_some(),
                            token_stats: None,
                        });
                    }
                }
            }
        }

        summaries
    }

    fn load_messages(&self, session: &SessionSummary, max_msgs: usize) -> Vec<ChatMessage> {
        let fpath = Path::new(&session.main_path);
        if !fpath.exists() || !fpath.extension().map_or(false, |e| e == "zstd") {
            return Vec::new();
        }

        let mut msgs = Vec::new();
        if let Ok(file) = File::open(fpath) {
            if let Ok(decoder) = zstd::stream::read::Decoder::new(file) {
                let reader = BufReader::new(decoder);
                for line in reader.lines().flatten() {
                    if let Ok(val) = serde_json::from_str::<Value>(&line) {
                        let etype = val.get("type").and_then(Value::as_str).unwrap_or("");
                        let time_str = val.get("time").and_then(Value::as_i64).map(|t| t.to_string()).unwrap_or_default();

                        if etype == "user/message" {
                            let text = val.get("data").and_then(|d| d.get("text").or_else(|| d.get("value"))).and_then(Value::as_str).unwrap_or("").to_string();
                            if !text.is_empty() {
                                msgs.push(ChatMessage {
                                    role: "user".to_string(),
                                    text,
                                    time: time_str,
                                    msg_type: etype.to_string(),
                                    thinking: None,
                                    tool_calls: Vec::new(),
                                });
                            }
                        } else if etype == "bot/message" {
                            let text = val.get("data").and_then(|d| d.get("text")).and_then(Value::as_str).unwrap_or("").to_string();
                            if !text.is_empty() {
                                msgs.push(ChatMessage {
                                    role: "assistant".to_string(),
                                    text,
                                    time: time_str,
                                    msg_type: etype.to_string(),
                                    thinking: None,
                                    tool_calls: Vec::new(),
                                });
                            }
                        }

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
        let projcache_file = home.join("storages").join("session_projcache.json");
        let mut pruned = 0;
        let set: HashSet<&str> = session_ids.iter().map(|s| s.as_str()).collect();

        if projcache_file.exists() {
            if let Ok(text) = fs::read_to_string(&projcache_file) {
                if let Ok(mut v) = serde_json::from_str::<Value>(&text) {
                    if let Some(sessions) = v.get_mut("tables").and_then(|t| t.get_mut("sessions")).and_then(Value::as_object_mut) {
                        let to_remove: Vec<String> = sessions.keys()
                            .filter(|k| {
                                let norm = k.strip_prefix("session-").unwrap_or(k);
                                set.contains(norm)
                            })
                            .cloned()
                            .collect();

                        for k in to_remove {
                            sessions.remove(&k);
                            pruned += 1;
                        }
                    }
                    let _ = fs::write(&projcache_file, serde_json::to_string(&v).unwrap_or_default());
                }
            }
        }

        pruned
    }
}
