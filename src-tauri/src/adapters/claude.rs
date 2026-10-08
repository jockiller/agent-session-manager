use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary, ToolCallItem};
use crate::services::token_calc::make_stats;

crate::lazy_regex!(
    CLAUDE_UUID_RE,
    r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
);

pub struct ClaudeAdapter;

impl ClaudeAdapter {
    fn home_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
            PathBuf::from(dir)
        } else if let Some(home) = crate::adapters::user_home() {
            home.join(".claude")
        } else {
            PathBuf::from(".claude")
        }
    }

    fn is_uuid(s: &str) -> bool {
        CLAUDE_UUID_RE.is_match(s)
    }

    fn load_live_session_ids(home: &Path) -> HashSet<String> {
        let mut live = HashSet::new();
        let sessions_dir = home.join("sessions");
        if !sessions_dir.exists() {
            return live;
        }

        if let Ok(entries) = fs::read_dir(sessions_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "json") {
                    if let Ok(text) = fs::read_to_string(&path) {
                        if let Ok(v) = serde_json::from_str::<Value>(&text) {
                            if let (Some(sid), Some(pid)) = (v.get("sessionId").and_then(Value::as_str), v.get("pid").and_then(Value::as_i64)) {
                                #[cfg(unix)]
                                unsafe {
                                    if libc::kill(pid as i32, 0) == 0 {
                                        live.insert(sid.to_string());
                                    }
                                }
                                #[cfg(windows)]
                                {
                                    let is_alive = std::process::Command::new("cmd")
                                        .args(["/c", &format!("tasklist /FI \"PID eq {}\" /NH", pid)])
                                        .output()
                                        .map(|out| {
                                            let s = String::from_utf8_lossy(&out.stdout);
                                            s.contains(&pid.to_string())
                                        })
                                        .unwrap_or(false);
                                    if is_alive {
                                        live.insert(sid.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        live
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

impl AgentAdapter for ClaudeAdapter {
    fn platform_id(&self) -> &'static str {
        "claude"
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let home = Self::home_dir();
        if !home.exists() {
            return Vec::new();
        }

        let live_ids = Self::load_live_session_ids(&home);
        let projects_dir = home.join("projects");
        let mut summaries = Vec::new();
        let mut seen = HashSet::new();

        if projects_dir.exists() {
            if let Ok(entries) = fs::read_dir(projects_dir) {
                for entry in entries.flatten() {
                    let project_path = entry.path();
                    if !project_path.is_dir() {
                        continue;
                    }

                    if let Ok(p_files) = fs::read_dir(&project_path) {
                        for pf in p_files.flatten() {
                            let fpath = pf.path();
                            if fpath.is_file() && fpath.extension().map_or(false, |e| e == "jsonl") {
                                let sid = fpath.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                                if !Self::is_uuid(sid) || seen.contains(sid) {
                                    continue;
                                }

                                seen.insert(sid.to_string());
                                let mut all_paths = vec![fpath.to_string_lossy().to_string()];
                                let session_dir = project_path.join(sid);
                                if session_dir.is_dir() {
                                    all_paths.push(session_dir.to_string_lossy().to_string());
                                }

                                // Sidecars
                                for sidecar in &["file-history", "session-env", "session-data", "todos", "tasks", "plans"] {
                                    let c = home.join(sidecar).join(sid);
                                    if c.exists() {
                                        all_paths.push(c.to_string_lossy().to_string());
                                    }
                                }

                                let mut title = String::new();
                                let mut cwd = String::new();
                                let mut model = String::new();
                                let mut created_at = 0i64;
                                let mut updated_at = 0i64;
                                let mut turn_count = 0u32;
                                let mut prompt_tokens = 0u64;
                                let mut completion_tokens = 0u64;
                                let mut cache_read_tokens = 0u64;
                                let mut cache_write_tokens = 0u64;

                                if let Ok(file) = File::open(&fpath) {
                                    let reader = BufReader::new(file);
                                    for line in reader.lines().flatten() {
                                        if line.trim().is_empty() {
                                            continue;
                                        }
                                        if let Ok(val) = serde_json::from_str::<Value>(&line) {
                                            if let Some(ts) = val.get("timestamp").and_then(Value::as_str) {
                                                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
                                                    let ms = dt.timestamp_millis();
                                                    if created_at == 0 {
                                                        created_at = ms;
                                                    }
                                                    updated_at = ms;
                                                }
                                            }

                                            if cwd.is_empty() {
                                                if let Some(c) = val.get("cwd").and_then(Value::as_str) {
                                                    cwd = c.to_string();
                                                }
                                            }

                                            let mtype = val.get("type").and_then(Value::as_str).unwrap_or("");
                                            if mtype == "ai-title" {
                                                if let Some(t) = val.get("aiTitle").and_then(Value::as_str) {
                                                    title = t.trim().to_string();
                                                }
                                            } else if mtype == "user" && title.is_empty() {
                                                turn_count += 1;
                                                if let Some(msg) = val.get("message") {
                                                    if let Some(c) = msg.get("content").and_then(Value::as_str) {
                                                        let s = c.trim();
                                                        if !s.starts_with('<') {
                                                            title = s.chars().take(80).collect();
                                                        }
                                                    }
                                                }
                                            } else if mtype == "assistant" {
                                                if let Some(msg) = val.get("message") {
                                                    if model.is_empty() {
                                                        if let Some(m) = msg.get("model").and_then(Value::as_str) {
                                                            if m != "<synthetic>" {
                                                                model = m.to_string();
                                                            }
                                                        }
                                                    }
                                                    if let Some(usage) = msg.get("usage") {
                                                        prompt_tokens += usage.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
                                                        completion_tokens += usage.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
                                                        cache_read_tokens += usage.get("cache_read_input_tokens").and_then(Value::as_u64).unwrap_or(0);
                                                        cache_write_tokens += usage.get("cache_creation_input_tokens").and_then(Value::as_u64).unwrap_or(0);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                if title.is_empty() {
                                    title = "(无标题)".to_string();
                                }
                                if model.is_empty() {
                                    model = "claude".to_string();
                                }

                                let total_size: u64 = all_paths.iter().map(|p| Self::path_size(Path::new(p))).sum();
                                let is_running = live_ids.contains(sid);

                                let token_stats = if prompt_tokens > 0 || completion_tokens > 0 {
                                    Some(make_stats(&model, prompt_tokens, completion_tokens, cache_read_tokens, cache_write_tokens))
                                } else {
                                    None
                                };

                                summaries.push(SessionSummary {
                                    id: sid.to_string(),
                                    platform: "claude".to_string(),
                                    flavor: model,
                                    dirname: sid.to_string(),
                                    main_path: fpath.to_string_lossy().to_string(),
                                    all_paths,
                                    cwd: cwd.clone(),
                                    title,
                                    created_at,
                                    updated_at,
                                    size_bytes: total_size,
                                    turn_count,
                                    is_subagent: false,
                                    parent_id: None,
                                    is_running,
                                    has_transcript: true,
                                    token_stats,
                                });

                                // Discover subagents inside session_dir
                                if session_dir.is_dir() {
                                    let mut sub_files = Vec::new();
                                    let sub_dir = session_dir.join("subagents");
                                    if sub_dir.is_dir() {
                                        if let Ok(entries) = fs::read_dir(sub_dir) {
                                            for e in entries.flatten() {
                                                let p = e.path();
                                                if p.is_file() && p.extension().map_or(false, |ext| ext == "jsonl") {
                                                    sub_files.push(p);
                                                }
                                            }
                                        }
                                    }
                                    if let Ok(entries) = fs::read_dir(&session_dir) {
                                        for e in entries.flatten() {
                                            let p = e.path();
                                            if p.is_file() && p.file_name().map_or(false, |n| n.to_string_lossy().starts_with("agent-") && n.to_string_lossy().ends_with(".jsonl")) {
                                                sub_files.push(p);
                                            }
                                        }
                                    }

                                    for sub_path in sub_files {
                                        let sub_sid = sub_path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                                        if sub_sid.is_empty() || seen.contains(&sub_sid) {
                                            continue;
                                        }
                                        seen.insert(sub_sid.clone());
                                        let sub_size = Self::path_size(&sub_path);
                                        summaries.push(SessionSummary {
                                            id: sub_sid.clone(),
                                            platform: "claude".to_string(),
                                            flavor: "claude-subagent".to_string(),
                                            dirname: sub_sid.clone(),
                                            main_path: sub_path.to_string_lossy().to_string(),
                                            all_paths: vec![sub_path.to_string_lossy().to_string()],
                                            cwd: cwd.clone(),
                                            title: format!("子代理: {}", sub_sid),
                                            created_at,
                                            updated_at,
                                            size_bytes: sub_size,
                                            turn_count: 1,
                                            is_subagent: true,
                                            parent_id: Some(sid.to_string()),
                                            is_running: false,
                                            has_transcript: true,
                                            token_stats: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
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
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<Value>(&line) {
                    let mtype = val.get("type").and_then(Value::as_str).unwrap_or("");
                    if mtype != "user" && mtype != "assistant" {
                        continue;
                    }

                    let role = if mtype == "user" { "user" } else { "assistant" };
                    let mut text = String::new();
                    let mut thinking = None;
                    let mut tool_calls = Vec::new();

                    if let Some(msg) = val.get("message") {
                        if let Some(content) = msg.get("content") {
                            if let Some(s) = content.as_str() {
                                text = s.to_string();
                            } else if let Some(arr) = content.as_array() {
                                for item in arr {
                                    let kind = item.get("type").and_then(Value::as_str).unwrap_or("");
                                    if kind == "text" {
                                        if let Some(t) = item.get("text").and_then(Value::as_str) {
                                            if !text.is_empty() {
                                                text.push('\n');
                                            }
                                            text.push_str(t);
                                        }
                                    } else if kind == "thinking" {
                                        if let Some(th) = item.get("thinking").and_then(Value::as_str) {
                                            thinking = Some(th.to_string());
                                        }
                                    } else if kind == "tool_use" {
                                        let name = item.get("name").and_then(Value::as_str).unwrap_or("tool").to_string();
                                        let args = item.get("input").map(|i| serde_json::to_string_pretty(i).unwrap_or_default());
                                        tool_calls.push(ToolCallItem { name, args, output: None });
                                    }
                                }
                            }
                        }
                    }

                    let time_str = val.get("timestamp").and_then(Value::as_str).unwrap_or("").to_string();
                    if !text.is_empty() || !tool_calls.is_empty() || thinking.is_some() {
                        msgs.push(ChatMessage {
                            role: role.to_string(),
                            text,
                            time: time_str,
                            msg_type: mtype.to_string(),
                            thinking,
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
        let mut pruned = 0;
        let set: HashSet<&str> = session_ids.iter().map(|s| s.as_str()).collect();

        let history_file = home.join("history.jsonl");
        if history_file.exists() {
            if let Ok(content) = fs::read_to_string(&history_file) {
                let mut kept = Vec::new();
                for line in content.lines() {
                    if let Ok(v) = serde_json::from_str::<Value>(line) {
                        if let Some(sid) = v.get("sessionId").and_then(Value::as_str) {
                            if set.contains(sid) {
                                pruned += 1;
                                continue;
                            }
                        }
                    }
                    kept.push(line);
                }
                let _ = fs::write(&history_file, kept.join("\n") + "\n");
            }
        }

        pruned
    }
}
