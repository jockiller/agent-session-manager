use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::DateTime;
use serde_json::Value;

use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary};

pub struct QoderAdapter;

impl QoderAdapter {
    fn qoder_dir() -> PathBuf {
        if let Some(home) = crate::adapters::user_home() {
            home.join(".qoder")
        } else {
            PathBuf::from(".qoder")
        }
    }

    fn parse_iso_ts(ts_str: &str) -> Option<i64> {
        if let Ok(dt) = DateTime::parse_from_rfc3339(ts_str) {
            return Some(dt.timestamp());
        }
        None
    }

    fn strip_wrapper_tags(text: &str) -> String {
        let without_reminder = regex::Regex::new(r"(?i)<system-reminder>[\s\S]*?</system-reminder>")
            .map(|re| re.replace_all(text, "").to_string())
            .unwrap_or_else(|_| text.to_string());
        let without_attached = regex::Regex::new(r"(?i)<attached_files>[\s\S]*?</attached_files>")
            .map(|re| re.replace_all(&without_reminder, "").to_string())
            .unwrap_or(without_reminder);
        if let Ok(re_user) = regex::Regex::new(r"(?i)<user_query>([\s\S]*?)</user_query>") {
            if let Some(caps) = re_user.captures(&without_attached) {
                if let Some(m) = caps.get(1) {
                    return m.as_str().trim().to_string();
                }
            }
        }
        without_attached.trim().to_string()
    }
}

impl AgentAdapter for QoderAdapter {
    fn platform_id(&self) -> &'static str {
        "qoder"
    }

    fn display_name(&self) -> &'static str {
        "Qoder"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let qoder_home = Self::qoder_dir();
        if !qoder_home.exists() {
            return Vec::new();
        }

        let mut summaries = Vec::new();
        let mut seen_ids = HashSet::new();

        // 1. Scan ~/.qoder/projects
        let projects_dir = qoder_home.join("projects");
        if projects_dir.exists() {
            for entry in walkdir::WalkDir::new(&projects_dir).into_iter().flatten() {
                let path = entry.path();
                if !path.is_file() || path.extension().map_or(true, |ext| ext != "jsonl") {
                    continue;
                }
                let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if file_name.ends_with(".session.execution.jsonl") {
                    continue;
                }

                let raw_id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                if raw_id.is_empty() || seen_ids.contains(&raw_id) {
                    continue;
                }
                seen_ids.insert(raw_id.clone());

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
                let created_at = mtime_sec;
                let updated_at = mtime_sec;
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

                            if let Some(c) = val.get("cwd").or_else(|| val.get("project_path")).and_then(Value::as_str) {
                                if cwd.is_empty() {
                                    cwd = c.to_string();
                                }
                            }

                            if title.is_empty() {
                                if let Some(msg) = val.get("message") {
                                    if let Some(content) = msg.get("content").and_then(Value::as_array) {
                                        for item in content {
                                            if item.get("type").and_then(Value::as_str) == Some("text") {
                                                if let Some(txt) = item.get("text").and_then(Value::as_str) {
                                                    let clean = Self::strip_wrapper_tags(txt);
                                                    if !clean.is_empty() {
                                                        title = clean;
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if title.is_empty() {
                    title = format!("Qoder 会话 {}", &raw_id[..raw_id.len().min(8)]);
                }

                let main_path_str = path.to_string_lossy().to_string();
                summaries.push(SessionSummary {
                    id: raw_id,
                    platform: "qoder".to_string(),
                    flavor: "qoder".to_string(),
                    dirname: cwd.clone(),
                    main_path: main_path_str.clone(),
                    all_paths: vec![main_path_str],
                    cwd,
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

        // 2. Scan ~/.qoder/logs/sessions (active session logs & segments)
        let sessions_dir = qoder_home.join("logs").join("sessions");
        if sessions_dir.exists() {
            for project_entry in fs::read_dir(&sessions_dir).into_iter().flatten().flatten() {
                let project_path = project_entry.path();
                if !project_path.is_dir() {
                    continue;
                }

                let raw_proj = project_path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                let decoded_cwd = if raw_proj.starts_with("-") {
                    raw_proj.replace('-', "/")
                } else if raw_proj == "-" {
                    "/".to_string()
                } else {
                    raw_proj.to_string()
                };

                for sess_entry in fs::read_dir(&project_path).into_iter().flatten().flatten() {
                    let sess_path = sess_entry.path();
                    if !sess_path.is_dir() {
                        continue;
                    }

                    let raw_id = sess_path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                    if raw_id.is_empty() || seen_ids.contains(&raw_id) {
                        continue;
                    }
                    seen_ids.insert(raw_id.clone());

                    // Look into segments/
                    let segments_dir = sess_path.join("segments");
                    let mut total_size = 0u64;
                    let mut all_files = Vec::new();
                    let mut earliest_ts = 0i64;
                    let mut latest_ts = 0i64;
                    let mut title = String::new();
                    let mut resolved_cwd = decoded_cwd.clone();
                    let mut main_file_path = sess_path.to_string_lossy().to_string();
                    let mut turn_count = 0u32;

                    if segments_dir.exists() {
                        for seg_entry in fs::read_dir(&segments_dir).into_iter().flatten().flatten() {
                            let seg_path = seg_entry.path();
                            if seg_path.extension().map_or(true, |ext| ext != "jsonl") {
                                continue;
                            }
                            if let Ok(meta) = fs::metadata(&seg_path) {
                                total_size += meta.len();
                            }
                            let seg_str = seg_path.to_string_lossy().to_string();
                            all_files.push(seg_str.clone());
                            if main_file_path == sess_path.to_string_lossy().to_string() {
                                main_file_path = seg_str;
                            }

                            if let Ok(file) = File::open(&seg_path) {
                                let reader = BufReader::new(file);
                                for line in reader.lines().flatten() {
                                    let line_str = line.trim();
                                    if line_str.is_empty() {
                                        continue;
                                    }
                                    if let Ok(val) = serde_json::from_str::<Value>(line_str) {
                                        turn_count += 1;
                                        if let Some(ts_str) = val.get("ts").and_then(Value::as_str) {
                                            if let Some(sec) = Self::parse_iso_ts(ts_str) {
                                                if earliest_ts == 0 || sec < earliest_ts {
                                                    earliest_ts = sec;
                                                }
                                                if sec > latest_ts {
                                                    latest_ts = sec;
                                                }
                                            }
                                        }

                                        if let Some(data) = val.get("data") {
                                            if let Some(pr) = data.get("project_root").or_else(|| data.get("target_dir")).and_then(Value::as_str) {
                                                if resolved_cwd.is_empty() || resolved_cwd == "/" {
                                                    resolved_cwd = pr.to_string();
                                                }
                                            }
                                            if title.is_empty() {
                                                if let Some(prompt) = data.get("prompt").or_else(|| data.get("query")).or_else(|| data.get("user_query")).and_then(Value::as_str) {
                                                    let clean = Self::strip_wrapper_tags(prompt);
                                                    if !clean.is_empty() {
                                                        title = clean;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if title.is_empty() {
                        title = format!("Qoder 会话 {}", &raw_id[..raw_id.len().min(8)]);
                    }
                    if earliest_ts == 0 {
                        earliest_ts = chrono::Utc::now().timestamp();
                    }
                    if latest_ts == 0 {
                        latest_ts = earliest_ts;
                    }

                    summaries.push(SessionSummary {
                        id: raw_id,
                        platform: "qoder".to_string(),
                        flavor: "qoder".to_string(),
                        dirname: resolved_cwd.clone(),
                        main_path: main_file_path,
                        all_paths: if all_files.is_empty() {
                            vec![sess_path.to_string_lossy().to_string()]
                        } else {
                            all_files
                        },
                        cwd: resolved_cwd,
                        title,
                        created_at: earliest_ts,
                        updated_at: latest_ts,
                        size_bytes: total_size,
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
        let mut msgs = Vec::new();

        for p_str in &session.all_paths {
            let path = Path::new(p_str);
            if !path.is_file() {
                continue;
            }

            if let Ok(file) = File::open(path) {
                let reader = BufReader::new(file);
                for line in reader.lines().flatten() {
                    let line_str = line.trim();
                    if line_str.is_empty() {
                        continue;
                    }

                    if let Ok(val) = serde_json::from_str::<Value>(line_str) {
                        // 1. Claude-Code style record
                        if let Some(role) = val.get("role").or_else(|| val.get("type")).and_then(Value::as_str) {
                            if role == "user" || role == "assistant" {
                                let mut text = String::new();
                                if let Some(msg) = val.get("message") {
                                    if let Some(content) = msg.get("content").and_then(Value::as_array) {
                                        for item in content {
                                            if item.get("type").and_then(Value::as_str) == Some("text") {
                                                if let Some(t) = item.get("text").and_then(Value::as_str) {
                                                    let clean = Self::strip_wrapper_tags(t);
                                                    if !clean.is_empty() {
                                                        text.push_str(&clean);
                                                    }
                                                }
                                            }
                                        }
                                    } else if let Some(t) = msg.get("content").and_then(Value::as_str) {
                                        text = Self::strip_wrapper_tags(t);
                                    }
                                } else if let Some(content) = val.get("content").and_then(Value::as_str) {
                                    text = Self::strip_wrapper_tags(content);
                                }

                                if !text.is_empty() {
                                    let ts = val.get("timestamp").or_else(|| val.get("ts")).and_then(Value::as_str).unwrap_or("").to_string();
                                    msgs.push(ChatMessage {
                                        role: role.to_string(),
                                        text,
                                        time: ts,
                                        msg_type: "text".to_string(),
                                        thinking: None,
                                        tool_calls: Vec::new(),
                                    });
                                }
                            }
                        }

                        // 2. Segment log record (type: session.config.loaded, etc.)
                        if let Some(typ) = val.get("type").and_then(Value::as_str) {
                            if typ.starts_with("session.") || typ.contains("prompt") {
                                if let Some(data) = val.get("data") {
                                    if let Some(prompt) = data.get("prompt").or_else(|| data.get("user_query")).and_then(Value::as_str) {
                                        let clean = Self::strip_wrapper_tags(prompt);
                                        if !clean.is_empty() {
                                            let ts = val.get("ts").and_then(Value::as_str).unwrap_or("").to_string();
                                            msgs.push(ChatMessage {
                                                role: "user".to_string(),
                                                text: clean,
                                                time: ts,
                                                msg_type: "text".to_string(),
                                                thinking: None,
                                                tool_calls: Vec::new(),
                                            });
                                        }
                                    }
                                }
                            }
                        }

                        if msgs.len() >= max_msgs {
                            break;
                        }
                    }
                }
            }

            if msgs.len() >= max_msgs {
                break;
            }
        }

        msgs
    }

    fn prune_indexes(&self, _session_ids: &[String]) -> usize {
        0
    }
}
