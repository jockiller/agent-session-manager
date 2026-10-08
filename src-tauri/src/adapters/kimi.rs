use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary};

pub struct KimiAdapter;

impl KimiAdapter {
    fn candidate_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(home) = crate::adapters::user_home() {
            let d1 = home.join(".kimi-code");
            let d2 = home.join(".kimi");
            if d1.exists() {
                dirs.push(d1);
            }
            if d2.exists() {
                dirs.push(d2);
            }
        }
        dirs
    }
}

impl AgentAdapter for KimiAdapter {
    fn platform_id(&self) -> &'static str {
        "kimi"
    }

    fn display_name(&self) -> &'static str {
        "Kimi Code"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let roots = Self::candidate_dirs();
        if roots.is_empty() {
            return Vec::new();
        }

        let mut summaries = Vec::new();
        let mut seen_ids = HashSet::new();

        for root in roots {
            // Check session_index.jsonl
            let index_file = root.join("session_index.jsonl");
            if index_file.exists() {
                if let Ok(file) = File::open(&index_file) {
                    let reader = BufReader::new(file);
                    for line in reader.lines().flatten() {
                        if let Ok(val) = serde_json::from_str::<Value>(&line) {
                            if val.get("deleted").and_then(Value::as_bool).unwrap_or(false) {
                                continue;
                            }
                            let sid = val.get("sessionId").and_then(Value::as_str).unwrap_or("");
                            if sid.is_empty() || seen_ids.contains(sid) {
                                continue;
                            }
                            seen_ids.insert(sid.to_string());

                            let sdir = val.get("sessionDir").and_then(Value::as_str).unwrap_or("");
                            let wdir = val.get("workDir").and_then(Value::as_str).unwrap_or("");
                            let session_path = if !sdir.is_empty() {
                                PathBuf::from(sdir)
                            } else {
                                root.join("sessions").join(sid)
                            };

                            let meta = fs::metadata(&session_path).ok();
                            let file_size = meta.as_ref().map(|m| m.len()).unwrap_or(4096);
                            let mtime_sec = meta
                                .as_ref()
                                .and_then(|m| m.modified().ok())
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|d| d.as_secs() as i64)
                                .unwrap_or(0);

                            let main_path_str = session_path.to_string_lossy().to_string();

                            summaries.push(SessionSummary {
                                id: sid.to_string(),
                                platform: "kimi".to_string(),
                                flavor: "kimi".to_string(),
                                dirname: wdir.to_string(),
                                main_path: main_path_str.clone(),
                                all_paths: vec![main_path_str],
                                cwd: wdir.to_string(),
                                title: format!("Kimi 会话 {}", &sid[..sid.len().min(8)]),
                                created_at: mtime_sec,
                                updated_at: mtime_sec,
                                size_bytes: file_size,
                                turn_count: 1,
                                is_subagent: false,
                                parent_id: None,
                                is_running: false,
                                has_transcript: true,
                                token_stats: None,
                            });
                        }
                    }
                }
            }

            // Also scan sessions/ directory for .jsonl
            let sessions_dir = root.join("sessions");
            if sessions_dir.exists() {
                for entry in walkdir::WalkDir::new(&sessions_dir).into_iter().flatten() {
                    let path = entry.path();
                    if !path.is_file() || path.extension().map_or(true, |ext| ext != "jsonl") {
                        continue;
                    }

                    let sid = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                    if sid.is_empty() || seen_ids.contains(&sid) {
                        continue;
                    }
                    seen_ids.insert(sid.clone());

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
                    let mut turn_count = 0u32;

                    if let Ok(file) = File::open(path) {
                        let reader = BufReader::new(file);
                        for line in reader.lines().flatten() {
                            if let Ok(val) = serde_json::from_str::<Value>(&line) {
                                if let Some(c) = val.get("cwd").or_else(|| val.get("workDir")).and_then(Value::as_str) {
                                    if cwd.is_empty() {
                                        cwd = c.to_string();
                                    }
                                }
                                if val.get("role").and_then(Value::as_str) == Some("user") && title.is_empty() {
                                    if let Some(txt) = val.get("content").or_else(|| val.get("text")).and_then(Value::as_str) {
                                        if !txt.trim().is_empty() {
                                            title = txt.trim().to_string();
                                        }
                                    }
                                }
                                turn_count += 1;
                            }
                        }
                    }

                    if title.is_empty() {
                        title = format!("Kimi 会话 {}", &sid[..sid.len().min(8)]);
                    }

                    let main_path_str = path.to_string_lossy().to_string();
                    summaries.push(SessionSummary {
                        id: sid,
                        platform: "kimi".to_string(),
                        flavor: "kimi".to_string(),
                        dirname: cwd.clone(),
                        main_path: main_path_str.clone(),
                        all_paths: vec![main_path_str],
                        cwd,
                        title,
                        created_at: mtime_sec,
                        updated_at: mtime_sec,
                        size_bytes: file_size,
                        turn_count,
                        is_subagent: false,
                        parent_id: None,
                        is_running: false,
                        has_transcript: true,
                        token_stats: None,
                    });
                }
            }
        }

        summaries
    }

    fn load_messages(&self, session: &SessionSummary, max_msgs: usize) -> Vec<ChatMessage> {
        let path = Path::new(&session.main_path);
        let mut target_file = path.to_path_buf();
        if target_file.is_dir() {
            let candidate1 = target_file.join("wire.jsonl");
            let candidate2 = target_file.join("context.jsonl");
            if candidate1.exists() {
                target_file = candidate1;
            } else if candidate2.exists() {
                target_file = candidate2;
            }
        }

        if !target_file.is_file() {
            return Vec::new();
        }

        let mut msgs = Vec::new();
        if let Ok(file) = File::open(&target_file) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                let line_str = line.trim();
                if line_str.is_empty() {
                    continue;
                }

                if let Ok(val) = serde_json::from_str::<Value>(line_str) {
                    let role = val.get("role").or_else(|| val.get("type")).and_then(Value::as_str).unwrap_or("");
                    if role == "user" || role == "assistant" {
                        let text = val.get("content").or_else(|| val.get("text")).and_then(Value::as_str).unwrap_or("");
                        if !text.is_empty() {
                            msgs.push(ChatMessage {
                                role: role.to_string(),
                                text: text.to_string(),
                                time: String::new(),
                                msg_type: "text".to_string(),
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

        msgs
    }

    fn prune_indexes(&self, _session_ids: &[String]) -> usize {
        0
    }
}
