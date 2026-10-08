use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary};

pub struct GeminiAdapter;

impl GeminiAdapter {
    fn candidate_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(home) = crate::adapters::user_home() {
            let hp = home.join(".gemini");
            let d1 = hp.join("cli");
            let d2 = hp.join("sessions");
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

impl AgentAdapter for GeminiAdapter {
    fn platform_id(&self) -> &'static str {
        "gemini"
    }

    fn display_name(&self) -> &'static str {
        "Gemini CLI"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let roots = Self::candidate_dirs();
        if roots.is_empty() {
            return Vec::new();
        }

        let mut summaries = Vec::new();
        let mut seen_ids = HashSet::new();

        for root in roots {
            for entry in walkdir::WalkDir::new(&root).into_iter().flatten() {
                let path = entry.path();
                if !path.is_file() || path.extension().map_or(true, |ext| ext != "json" && ext != "jsonl") {
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
                            turn_count += 1;
                            if let Some(c) = val.get("cwd").or_else(|| val.get("project_root")).and_then(Value::as_str) {
                                if cwd.is_empty() {
                                    cwd = c.to_string();
                                }
                            }
                            if let Some(s) = val.get("summary").and_then(Value::as_str) {
                                if title.is_empty() && !s.trim().is_empty() {
                                    title = s.trim().to_string();
                                }
                            }
                            if title.is_empty() && val.get("type").and_then(Value::as_str) == Some("user") {
                                if let Some(text) = val.get("content").or_else(|| val.get("text")).and_then(Value::as_str) {
                                    if !text.trim().is_empty() {
                                        title = text.trim().to_string();
                                    }
                                }
                            }
                        }
                    }
                }

                if title.is_empty() {
                    title = format!("Gemini 会话 {}", &sid[..sid.len().min(8)]);
                }

                let main_path_str = path.to_string_lossy().to_string();
                summaries.push(SessionSummary {
                    id: sid,
                    platform: "gemini".to_string(),
                    flavor: "gemini".to_string(),
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

        summaries
    }

    fn load_messages(&self, session: &SessionSummary, max_msgs: usize) -> Vec<ChatMessage> {
        let path = Path::new(&session.main_path);
        if !path.is_file() {
            return Vec::new();
        }

        let mut msgs = Vec::new();
        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                let line_str = line.trim();
                if line_str.is_empty() {
                    continue;
                }

                if let Ok(val) = serde_json::from_str::<Value>(line_str) {
                    let role = val.get("type").or_else(|| val.get("role")).and_then(Value::as_str).unwrap_or("");
                    if role == "user" || role == "gemini" || role == "assistant" {
                        let text = val.get("content").or_else(|| val.get("text")).and_then(Value::as_str).unwrap_or("");
                        if !text.is_empty() {
                            let r = if role == "gemini" { "assistant" } else { role };
                            msgs.push(ChatMessage {
                                role: r.to_string(),
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
