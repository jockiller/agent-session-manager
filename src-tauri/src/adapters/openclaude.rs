use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary, ToolCallItem};
use crate::services::token_calc::make_stats;

lazy_regex!(OPENCLAUDE_UUID_RE, r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$");

pub struct OpenClaudeAdapter;

impl OpenClaudeAdapter {
    fn home_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("OPENCLAUDE_CONFIG_DIR") {
            PathBuf::from(dir)
        } else if let Some(home) = crate::adapters::user_home() {
            home.join(".openclaude")
        } else {
            PathBuf::from(".openclaude")
        }
    }

    fn is_uuid(s: &str) -> bool {
        OPENCLAUDE_UUID_RE.is_match(s)
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

impl AgentAdapter for OpenClaudeAdapter {
    fn platform_id(&self) -> &'static str {
        "openclaude"
    }

    fn display_name(&self) -> &'static str {
        "OpenClaude"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let home = Self::home_dir();
        if !home.exists() {
            return Vec::new();
        }

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
                                            let mtype = val.get("type").and_then(Value::as_str).unwrap_or("");
                                            if mtype == "last-prompt" {
                                                if let Some(p) = val.get("lastPrompt").and_then(Value::as_str) {
                                                    if title.is_empty() && !p.trim().is_empty() {
                                                        title = p.trim().to_string();
                                                    }
                                                }
                                            }

                                            if let Some(ts) = val.get("timestamp").and_then(Value::as_str) {
                                                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
                                                    let millis = dt.timestamp_millis();
                                                    if created_at == 0 || millis < created_at {
                                                        created_at = millis;
                                                    }
                                                    if millis > updated_at {
                                                        updated_at = millis;
                                                    }
                                                }
                                            }

                                            if cwd.is_empty() {
                                                if let Some(c) = val.get("cwd").and_then(Value::as_str) {
                                                    cwd = c.to_string();
                                                }
                                            }

                                            if mtype == "user" {
                                                turn_count += 1;
                                                if title.is_empty() {
                                                    if let Some(msg) = val.get("message") {
                                                        if let Some(content) = msg.get("content").and_then(Value::as_str) {
                                                            title = content.lines().next().unwrap_or("").trim().to_string();
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
                                    model = "openclaude".to_string();
                                }

                                let total_size: u64 = all_paths.iter().map(|p| Self::path_size(Path::new(p))).sum();

                                let token_stats = if prompt_tokens > 0 || completion_tokens > 0 {
                                    Some(make_stats(&model, prompt_tokens, completion_tokens, cache_read_tokens, cache_write_tokens))
                                } else {
                                    None
                                };

                                summaries.push(SessionSummary {
                                    id: sid.to_string(),
                                    platform: "openclaude".to_string(),
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
                                    is_running: false,
                                    has_transcript: true,
                                    token_stats,
                                });

                                // Discover subagents inside session_dir
                                if session_dir.is_dir() {
                                    let sub_dir = session_dir.join("subagents");
                                    if sub_dir.is_dir() {
                                        if let Ok(subs) = fs::read_dir(sub_dir) {
                                            for sub in subs.flatten() {
                                                let sub_path = sub.path();
                                                if sub_path.is_file() && sub_path.extension().map_or(false, |e| e == "jsonl") {
                                                    let sub_id = sub_path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                                                    if seen.contains(&sub_id) {
                                                        continue;
                                                    }
                                                    seen.insert(sub_id.clone());

                                                    let mut sub_title = String::new();
                                                    let mut sub_model = "openclaude-subagent".to_string();
                                                    let mut sub_created = 0i64;
                                                    let mut sub_updated = 0i64;
                                                    let mut sub_turns = 0u32;
                                                    let mut s_in = 0u64;
                                                    let mut s_out = 0u64;
                                                    let mut s_cread = 0u64;
                                                    let mut s_cwrite = 0u64;

                                                    if let Ok(sf) = File::open(&sub_path) {
                                                        let s_reader = BufReader::new(sf);
                                                        for s_line in s_reader.lines().flatten() {
                                                            if let Ok(s_val) = serde_json::from_str::<Value>(&s_line) {
                                                                if let Some(ts) = s_val.get("timestamp").and_then(Value::as_str) {
                                                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
                                                                        let sec = dt.timestamp();
                                                                        if sub_created == 0 || sec < sub_created {
                                                                            sub_created = sec;
                                                                        }
                                                                        if sec > sub_updated {
                                                                            sub_updated = sec;
                                                                        }
                                                                    }
                                                                }

                                                                let stype = s_val.get("type").and_then(Value::as_str).unwrap_or("");
                                                                if stype == "user" {
                                                                    sub_turns += 1;
                                                                    if sub_title.is_empty() {
                                                                        if let Some(msg) = s_val.get("message") {
                                                                            if let Some(cnt) = msg.get("content").and_then(Value::as_str) {
                                                                                sub_title = cnt.lines().next().unwrap_or("").trim().to_string();
                                                                            }
                                                                        }
                                                                    }
                                                                } else if stype == "assistant" {
                                                                    if let Some(msg) = s_val.get("message") {
                                                                        if let Some(m) = msg.get("model").and_then(Value::as_str) {
                                                                            if m != "<synthetic>" {
                                                                                sub_model = m.to_string();
                                                                            }
                                                                        }
                                                                        if let Some(usage) = msg.get("usage") {
                                                                            s_in += usage.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
                                                                            s_out += usage.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
                                                                            s_cread += usage.get("cache_read_input_tokens").and_then(Value::as_u64).unwrap_or(0);
                                                                            s_cwrite += usage.get("cache_creation_input_tokens").and_then(Value::as_u64).unwrap_or(0);
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }

                                                    if sub_created == 0 {
                                                        sub_created = created_at / 1000;
                                                        sub_updated = updated_at / 1000;
                                                    }

                                                    if sub_title.is_empty() {
                                                        sub_title = "(子任务)".to_string();
                                                    }

                                                    let sub_size = Self::path_size(&sub_path);
                                                    let sub_stats = if s_in > 0 || s_out > 0 {
                                                        Some(make_stats(&sub_model, s_in, s_out, s_cread, s_cwrite))
                                                    } else {
                                                        None
                                                    };

                                                    summaries.push(SessionSummary {
                                                        id: sub_id.clone(),
                                                        platform: "openclaude".to_string(),
                                                        flavor: sub_model,
                                                        dirname: sub_id,
                                                        main_path: sub_path.to_string_lossy().to_string(),
                                                        all_paths: vec![sub_path.to_string_lossy().to_string()],
                                                        cwd: cwd.clone(),
                                                        title: sub_title,
                                                        created_at: sub_created,
                                                        updated_at: sub_updated,
                                                        size_bytes: sub_size,
                                                        turn_count: sub_turns,
                                                        is_subagent: true,
                                                        parent_id: Some(sid.to_string()),
                                                        is_running: false,
                                                        has_transcript: true,
                                                        token_stats: sub_stats,
                                                    });
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
                    let mtype = val.get("type").and_then(Value::as_str).unwrap_or("");
                    let time_str = val.get("timestamp").and_then(Value::as_str).unwrap_or("").to_string();

                    if mtype == "user" {
                        let text = val.get("message")
                            .and_then(|m| m.get("content"))
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();

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
                    } else if mtype == "assistant" {
                        let mut text = String::new();
                        let mut thinking = None;
                        let mut tool_calls = Vec::new();

                        if let Some(msg) = val.get("message") {
                            if let Some(content) = msg.get("content") {
                                if let Some(s) = content.as_str() {
                                    text = s.to_string();
                                } else if let Some(arr) = content.as_array() {
                                    for block in arr {
                                        let btype = block.get("type").and_then(Value::as_str).unwrap_or("");
                                        if btype == "text" {
                                            if let Some(t) = block.get("text").and_then(Value::as_str) {
                                                text.push_str(t);
                                                text.push('\n');
                                            }
                                        } else if btype == "thinking" {
                                            if let Some(th) = block.get("thinking").and_then(Value::as_str) {
                                                thinking = Some(th.to_string());
                                            }
                                        } else if btype == "tool_use" {
                                            let name = block.get("name").and_then(Value::as_str).unwrap_or("tool").to_string();
                                            let input = block.get("input").map(|i| serde_json::to_string_pretty(i).unwrap_or_default());
                                            tool_calls.push(ToolCallItem {
                                                name,
                                                args: input,
                                                output: None,
                                            });
                                        }
                                    }
                                }
                            }
                        }

                        if !text.is_empty() || thinking.is_some() || !tool_calls.is_empty() {
                            messages.push(ChatMessage {
                                role: "assistant".to_string(),
                                text: text.trim().to_string(),
                                time: time_str,
                                msg_type: "assistant".to_string(),
                                thinking,
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
