use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary, ToolCallItem};
use crate::services::token_calc::make_stats;

crate::lazy_regex!(
    CODEX_UUID_RE,
    r"([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})"
);

pub struct CodexAdapter;

impl CodexAdapter {
    fn get_homes() -> Vec<PathBuf> {
        let mut homes = Vec::new();
        if let Some(home) = crate::adapters::user_home() {
            let p1 = home.join(".codex");
            if p1.exists() {
                homes.push(p1);
            }
            let p2 = home.join(".tcodex");
            if p2.exists() && p2 != home.join(".codex") {
                homes.push(p2);
            }
        }
        homes
    }

    fn extract_codex_id(name: &str) -> String {
        CODEX_UUID_RE.find_iter(name).last().map(|m| m.as_str().to_string()).unwrap_or_default()
    }

    fn load_index(home: &Path) -> HashMap<String, (String, i64)> {
        let mut map = HashMap::new();
        let idx_path = home.join("session_index.jsonl");
        if !idx_path.exists() {
            return map;
        }

        if let Ok(file) = File::open(idx_path) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    if let Some(id) = v.get("id").or_else(|| v.get("session_id")).and_then(Value::as_str) {
                        let title = v.get("thread_name").or_else(|| v.get("title")).and_then(Value::as_str).unwrap_or("").to_string();
                        let ts = v.get("updated_at").and_then(Value::as_i64).unwrap_or(0);
                        map.insert(id.to_string(), (title, ts));
                    }
                }
            }
        }
        map
    }
}

impl AgentAdapter for CodexAdapter {
    fn platform_id(&self) -> &'static str {
        "codex"
    }

    fn display_name(&self) -> &'static str {
        "Codex"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let homes = Self::get_homes();
        let mut summaries = Vec::new();
        let mut seen = HashSet::new();

        for home in homes {
            let index = Self::load_index(&home);
            for sub in &["sessions", "archived_sessions"] {
                let dir = home.join(sub);
                if !dir.exists() {
                    continue;
                }

                for entry in walkdir::WalkDir::new(&dir).into_iter().flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().map_or(false, |e| e == "jsonl") {
                        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        let sid = Self::extract_codex_id(file_stem);
                        if sid.is_empty() || seen.contains(&sid) {
                            continue;
                        }

                        seen.insert(sid.clone());

                        let mut title = String::new();
                        let mut cwd = String::new();
                        let mut model = String::new();
                        let mut created_at = 0i64;
                        let mut updated_at = 0i64;
                        let mut turn_count = 0u32;
                        let mut prompt_tokens = 0u64;
                        let mut completion_tokens = 0u64;

                        if let Some((idx_title, idx_ts)) = index.get(&sid) {
                            if !idx_title.is_empty() {
                                title = idx_title.clone();
                            }
                            if *idx_ts > 0 {
                                updated_at = *idx_ts;
                            }
                        }

                        let mut parent_id = None;
                        let mut is_subagent = false;

                        if let Ok(file) = File::open(path) {
                            let reader = BufReader::new(file);
                            for line in reader.lines().flatten() {
                                if let Ok(val) = serde_json::from_str::<Value>(&line) {
                                    let kind = val.get("type").and_then(Value::as_str).unwrap_or("");
                                    let payload = val.get("payload");

                                    if kind == "session_meta" {
                                        if let Some(p) = payload {
                                            if cwd.is_empty() {
                                                if let Some(c) = p.get("cwd").and_then(Value::as_str) {
                                                    cwd = c.to_string();
                                                }
                                            }
                                            if title.is_empty() {
                                                if let Some(t) = p.get("title").and_then(Value::as_str) {
                                                    title = t.trim().to_string();
                                                }
                                            }
                                            if let Some(parent) = p.get("parent_thread_id").and_then(Value::as_str) {
                                                if !parent.is_empty() {
                                                    parent_id = Some(parent.to_string());
                                                    is_subagent = true;
                                                }
                                            }
                                            if let Some(src) = p.get("source") {
                                                if let Some(parent) = src.get("subagent").and_then(|s| s.get("thread_spawn")).and_then(|sp| sp.get("parent_thread_id")).and_then(Value::as_str) {
                                                    if !parent.is_empty() {
                                                        parent_id = Some(parent.to_string());
                                                        is_subagent = true;
                                                    }
                                                }
                                            }
                                            if p.get("thread_source").and_then(Value::as_str) == Some("subagent") {
                                                is_subagent = true;
                                            }
                                        }
                                    } else if kind == "turn_context" {
                                        if let Some(p) = payload {
                                            if model.is_empty() {
                                                if let Some(m) = p.get("model").and_then(Value::as_str) {
                                                    model = m.to_string();
                                                }
                                            }
                                        }
                                    } else if kind == "response_item" {
                                        if let Some(p) = payload {
                                            if p.get("type").and_then(Value::as_str) == Some("message") {
                                                if p.get("role").and_then(Value::as_str) == Some("user") {
                                                    turn_count += 1;
                                                }
                                            }
                                        }
                                    } else if kind == "token_usage" || kind == "usage" {
                                        if let Some(p) = payload {
                                            prompt_tokens += p.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0);
                                            completion_tokens += p.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0);
                                        }
                                    }
                                }
                            }
                        }

                        if title.is_empty() {
                            title = "(无标题)".to_string();
                        }
                        if model.is_empty() {
                            model = "codex".to_string();
                        }

                        let fsize = path.metadata().map(|m| m.len()).unwrap_or(0);
                        if updated_at == 0 {
                            if let Ok(m) = path.metadata() {
                                if let Ok(mtime) = m.modified() {
                                    if let Ok(duration) = mtime.duration_since(std::time::UNIX_EPOCH) {
                                        updated_at = duration.as_millis() as i64;
                                        created_at = updated_at;
                                    }
                                }
                            }
                        }

                        let token_stats = if prompt_tokens > 0 || completion_tokens > 0 {
                            Some(make_stats(&model, prompt_tokens, completion_tokens, 0, 0))
                        } else {
                            None
                        };

                        summaries.push(SessionSummary {
                            id: sid,
                            platform: "codex".to_string(),
                            flavor: model,
                            dirname: file_stem.to_string(),
                            main_path: path.to_string_lossy().to_string(),
                            all_paths: vec![path.to_string_lossy().to_string()],
                            cwd,
                            title,
                            created_at: crate::adapters::to_millis(created_at),
                            updated_at: crate::adapters::to_millis(updated_at),
                            size_bytes: fsize,
                            turn_count,
                            is_subagent,
                            parent_id,
                            is_running: false,
                            has_transcript: true,
                            token_stats,
                        });
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
                if let Ok(val) = serde_json::from_str::<Value>(&line) {
                    if val.get("type").and_then(Value::as_str) == Some("response_item") {
                        if let Some(payload) = val.get("payload") {
                            let role = payload.get("role").and_then(Value::as_str).unwrap_or("");
                            if role != "user" && role != "assistant" {
                                if let Some(ptype) = payload.get("type").and_then(Value::as_str) {
                                    if ptype.contains("call") {
                                        let name = payload.get("name").and_then(Value::as_str).unwrap_or("tool").to_string();
                                        msgs.push(ChatMessage {
                                            role: "assistant".to_string(),
                                            text: format!("[工具调用: {}]", name),
                                            time: "".to_string(),
                                            msg_type: ptype.to_string(),
                                            thinking: None,
                                            tool_calls: vec![ToolCallItem { name, args: None, output: None }],
                                        });
                                    }
                                }
                                continue;
                            }

                            let mut text = String::new();
                            if let Some(content) = payload.get("content") {
                                if let Some(s) = content.as_str() {
                                    text = s.to_string();
                                } else if let Some(arr) = content.as_array() {
                                    for item in arr {
                                        if let Some(t) = item.get("text").and_then(Value::as_str) {
                                            if !text.is_empty() {
                                                text.push('\n');
                                            }
                                            text.push_str(t);
                                        }
                                    }
                                }
                            }

                            if !text.trim().is_empty() && !text.starts_with("<environment_context>") {
                                msgs.push(ChatMessage {
                                    role: role.to_string(),
                                    text,
                                    time: "".to_string(),
                                    msg_type: "message".to_string(),
                                    thinking: None,
                                    tool_calls: Vec::new(),
                                });
                                if msgs.len() >= max_msgs {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        msgs
    }

    fn prune_indexes(&self, session_ids: &[String]) -> usize {
        let set: HashSet<&str> = session_ids.iter().map(|s| s.as_str()).collect();
        let mut pruned = 0;
        for home in Self::get_homes() {
            let index_file = home.join("session_index.jsonl");
            if index_file.exists() {
                if let Ok(content) = fs::read_to_string(&index_file) {
                    let mut kept = Vec::new();
                    for line in content.lines() {
                        if let Ok(v) = serde_json::from_str::<Value>(line) {
                            if let Some(id) = v.get("id").or_else(|| v.get("session_id")).and_then(Value::as_str) {
                                if set.contains(id) {
                                    pruned += 1;
                                    continue;
                                }
                            }
                        }
                        kept.push(line);
                    }
                    let _ = fs::write(&index_file, kept.join("\n") + "\n");
                }
            }
        }
        pruned
    }
}
