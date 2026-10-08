use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use serde_json::Value;

use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary, ToolCallItem};

pub struct TraeAdapter;

impl TraeAdapter {
    fn candidate_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(home) = crate::adapters::user_home() {
            let d1 = home.join(".trae").join("memory").join("projects");
            let d2 = home.join(".trae-cn").join("memory").join("projects");
            if d1.exists() {
                dirs.push(d1);
            }
            if d2.exists() {
                dirs.push(d2);
            }
        }
        dirs
    }

    fn decode_project_dir(slug: &str) -> String {
        if !slug.starts_with('-') {
            return slug.to_string();
        }
        let replaced = slug.replace('-', "/");
        // Try to find if any prefix or decoded directory exists
        let path = PathBuf::from(&replaced);
        if path.exists() {
            return replaced;
        }

        // Try stripping hash suffix if any (e.g. "--p2-8ff4d5f4605325af34f4")
        if let Some(idx) = replaced.find("//") {
            let prefix = &replaced[..idx];
            if Path::new(prefix).exists() {
                return prefix.to_string();
            }
        }

        // Search prefixes
        let mut curr = Path::new(&replaced);
        while let Some(parent) = curr.parent() {
            if parent.exists() && parent != Path::new("/") && parent != Path::new("") {
                return parent.to_string_lossy().to_string();
            }
            curr = parent;
        }

        replaced
    }

    fn parse_summary_time(time_str: &str) -> Option<i64> {
        // format: "2026-09-01 19:32:55"
        if let Ok(dt) = NaiveDateTime::parse_from_str(time_str, "%Y-%m-%d %H:%M:%S") {
            return Some(dt.and_utc().timestamp());
        }
        None
    }
}

impl AgentAdapter for TraeAdapter {
    fn platform_id(&self) -> &'static str {
        "trae"
    }

    fn display_name(&self) -> &'static str {
        "Trae"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let roots = Self::candidate_dirs();
        if roots.is_empty() {
            return Vec::new();
        }

        let mut summaries = Vec::new();

        for root in roots {
            for entry in walkdir::WalkDir::new(&root).into_iter().flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }

                let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if !file_name.starts_with("session_memory_") || !file_name.ends_with(".jsonl") {
                    continue;
                }

                let raw_id = file_name
                    .trim_start_matches("session_memory_")
                    .trim_end_matches(".jsonl")
                    .to_string();

                // Extract project slug from path: memory/projects/<slug>/...
                let mut project_dir = String::new();
                if let Ok(rel) = path.strip_prefix(&root) {
                    if let Some(first_comp) = rel.components().next() {
                        let slug = first_comp.as_os_str().to_string_lossy();
                        project_dir = Self::decode_project_dir(&slug);
                    }
                }

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
                let mut created_at = mtime_sec;
                let mut updated_at = mtime_sec;
                let mut turn_count = 0u32;

                if let Ok(file) = File::open(path) {
                    let reader = BufReader::new(file);
                    for line in reader.lines().flatten() {
                        let line_str = line.trim();
                        if line_str.is_empty() {
                            continue;
                        }

                        if let Ok(val) = serde_json::from_str::<Value>(line_str) {
                            turn_count += 1;

                            if title.is_empty() {
                                if let Some(intent) = val.get("intent").and_then(Value::as_str) {
                                    if !intent.trim().is_empty() {
                                        title = intent.trim().to_string();
                                    }
                                }
                            }

                            if let Some(t_str) = val.get("message_summary_time").and_then(Value::as_str) {
                                if let Some(sec) = Self::parse_summary_time(t_str) {
                                    if created_at == mtime_sec || sec < created_at {
                                        created_at = sec;
                                    }
                                    if sec > updated_at {
                                        updated_at = sec;
                                    }
                                }
                            }

                            if let Some(created_ms) = val
                                .get("compact_summary_meta")
                                .and_then(|m| m.get("created_at_ms"))
                                .and_then(Value::as_i64)
                            {
                                let sec = created_ms / 1000;
                                if created_at == mtime_sec || sec < created_at {
                                    created_at = sec;
                                }
                                if sec > updated_at {
                                    updated_at = sec;
                                }
                            }
                        }
                    }
                }

                if title.is_empty() {
                    title = format!("Trae 会话 {}", &raw_id[..raw_id.len().min(8)]);
                }

                let main_path_str = path.to_string_lossy().to_string();

                summaries.push(SessionSummary {
                    id: raw_id,
                    platform: "trae".to_string(),
                    flavor: "trae".to_string(),
                    dirname: project_dir.clone(),
                    main_path: main_path_str.clone(),
                    all_paths: vec![main_path_str],
                    cwd: project_dir,
                    title,
                    created_at,
                    updated_at,
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
                let time_str = val
                    .get("message_summary_time")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();

                // User turn from intent
                if let Some(intent) = val.get("intent").and_then(Value::as_str) {
                    if !intent.trim().is_empty() {
                        msgs.push(ChatMessage {
                            role: "user".to_string(),
                            text: intent.trim().to_string(),
                            time: time_str.clone(),
                            msg_type: "text".to_string(),
                            thinking: None,
                            tool_calls: Vec::new(),
                        });
                    }
                }

                // Assistant turn from outcome & actions
                let outcome = val.get("outcome").and_then(Value::as_str).unwrap_or("");
                let mut assistant_text = outcome.to_string();

                let mut tool_calls = Vec::new();
                if let Some(actions) = val.get("actions").and_then(Value::as_array) {
                    for act in actions {
                        if let Some(act_str) = act.as_str() {
                            tool_calls.push(ToolCallItem {
                                name: "action".to_string(),
                                args: Some(act_str.to_string()),
                                output: None,
                            });
                        }
                    }
                }

                if let Some(learned) = val.get("learned").and_then(Value::as_array) {
                    if !learned.is_empty() {
                        let learned_items: Vec<String> = learned
                            .iter()
                            .filter_map(|l| l.as_str())
                            .map(|s| format!("- {}", s))
                            .collect();
                        if !learned_items.is_empty() {
                            if !assistant_text.is_empty() {
                                assistant_text.push_str("\n\n");
                            }
                            assistant_text.push_str("学习总结:\n");
                            assistant_text.push_str(&learned_items.join("\n"));
                        }
                    }
                }

                if !assistant_text.is_empty() || !tool_calls.is_empty() {
                    msgs.push(ChatMessage {
                        role: "assistant".to_string(),
                        text: assistant_text,
                        time: time_str,
                        msg_type: "text".to_string(),
                        thinking: None,
                        tool_calls,
                    });
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
