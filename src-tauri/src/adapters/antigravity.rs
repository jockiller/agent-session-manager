use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::Value;

use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary, ToolCallItem};
use crate::services::token_calc::make_stats;

crate::lazy_regex!(AGY_UUID_RE, r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$");
crate::lazy_regex!(AGY_REQ_RE, r"(?s)<USER_REQUEST>(.*?)</USER_REQUEST>");
crate::lazy_regex!(AGY_XML_RE, r"<[^>]+>");
crate::lazy_regex!(AGY_MENTION_RE, r"^@\[[^\]]+\]\s*");
crate::lazy_regex!(AGY_WS_RE, r"file://(?:/)?([a-zA-Z]:[/\\][^\s\)]+|/(?:Users|home)/[^\s\)]+)");

pub struct AntigravityAdapter;

impl AntigravityAdapter {
    fn gemini_home() -> PathBuf {
        if let Ok(dir) = std::env::var("GEMINI_HOME") {
            PathBuf::from(dir)
        } else if let Some(home) = crate::adapters::user_home() {
            home.join(".gemini")
        } else {
            PathBuf::from(".gemini")
        }
    }

    fn clean_title(raw: &str) -> String {
        let s = raw.trim();
        if s.is_empty() {
            return String::new();
        }

        let content = if let Some(caps) = AGY_REQ_RE.captures(s) {
            caps.get(1).map_or(s, |m| m.as_str()).trim()
        } else {
            s
        };

        let no_xml = AGY_XML_RE.replace_all(content, " ");
        let clean_str = AGY_MENTION_RE.replace(&no_xml, "");
        let normalized = clean_str.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.is_empty() {
            String::new()
        } else {
            normalized.chars().take(120).collect()
        }
    }

    fn load_projects(base: &Path) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let proj_dir = base.join("config").join("projects");
        if !proj_dir.exists() {
            return map;
        }

        if let Ok(entries) = fs::read_dir(proj_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() && p.extension().map_or(false, |e| e == "json") {
                    if let Ok(text) = fs::read_to_string(&p) {
                        if let Ok(v) = serde_json::from_str::<Value>(&text) {
                            let pid = v.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                            if !pid.is_empty() {
                                if let Some(res_arr) = v.get("projectResources").and_then(|r| r.get("resources")).and_then(Value::as_array) {
                                    for res in res_arr {
                                        if let Some(uri) = res.get("gitFolder").and_then(|g| g.get("folderUri")).and_then(Value::as_str) {
                                            let cleaned = if uri.starts_with("file://") { &uri[7..] } else { uri };
                                            map.insert(pid.clone(), cleaned.to_string());
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
        map
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

struct SummaryDbRow {
    title: String,
    cwd: String,
    updated_at: i64,
    steps: u32,
    parent_id: Option<String>,
    is_sub: bool,
    status: String,
}

impl AgentAdapter for AntigravityAdapter {
    fn platform_id(&self) -> &'static str {
        "antigravity"
    }

    fn display_name(&self) -> &'static str {
        "Google Antigravity"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let base = Self::gemini_home();
        if !base.exists() {
            return Vec::new();
        }

        let projects = Self::load_projects(&base);
        let mut summaries = Vec::new();
        let flavors = [("cli", "antigravity-cli"), ("app", "antigravity"), ("ide", "antigravity-ide")];

        for (flavor, dir_name) in flavors {
            let fdir = base.join(dir_name);
            if !fdir.exists() {
                continue;
            }

            // 1. Read SQLite summaries DB if available
            let sum_db = fdir.join("conversation_summaries.db");
            let mut db_meta: HashMap<String, SummaryDbRow> = HashMap::new();
            if sum_db.exists() {
                if let Ok(conn) = Connection::open_with_flags(&sum_db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
                    let query = "SELECT conversation_id, title, preview, workspace_uris, project_id, step_count, last_modified_time, parent_conversation_id, nesting_depth, status FROM conversation_summaries";
                    if let Ok(mut stmt) = conn.prepare(query) {
                        let rows = stmt.query_map([], |row| {
                            let cid: String = row.get(0)?;
                            let t1: Option<String> = row.get(1)?;
                            let t2: Option<String> = row.get(2)?;
                            let ws_raw: Option<String> = row.get(3)?;
                            let pid: Option<String> = row.get(4)?;
                            let steps: Option<u32> = row.get(5)?;
                            let ltime: Option<String> = row.get(6)?;
                            let parent_id: Option<String> = row.get(7)?;
                            let nesting: Option<i32> = row.get(8)?;
                            let status: Option<String> = row.get(9)?;

                            Ok((cid, t1, t2, ws_raw, pid, steps.unwrap_or(0), ltime, parent_id, nesting.unwrap_or(0), status.unwrap_or_default()))
                        });

                        if let Ok(mapped) = rows {
                            for item in mapped.flatten() {
                                let (cid, t1, t2, ws_raw, pid, steps, ltime, parent_id, nesting, status) = item;

                                // Extract title properly from title or preview
                                let raw_title = match t1 {
                                    Some(ref t) if !t.trim().is_empty() => t.clone(),
                                    _ => t2.unwrap_or_default(),
                                };
                                let title = Self::clean_title(&raw_title);

                                let mut cwd = String::new();
                                if let Some(raw) = ws_raw {
                                    if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                                        if let Some(arr) = v.as_array() {
                                            if let Some(first) = arr.first().and_then(Value::as_str) {
                                                cwd = if first.starts_with("file://") { first[7..].to_string() } else { first.to_string() };
                                            }
                                        }
                                    }
                                }
                                if cwd.is_empty() {
                                    if let Some(p) = pid {
                                        if let Some(ws) = projects.get(&p) {
                                            cwd = ws.clone();
                                        }
                                    }
                                }

                                let mut ts = 0i64;
                                if let Some(lt) = ltime {
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&lt) {
                                        ts = dt.timestamp_millis();
                                    }
                                }

                                let clean_parent = parent_id.filter(|p| !p.trim().is_empty());
                                let is_sub = clean_parent.is_some() || nesting > 0;

                                db_meta.insert(cid.clone(), SummaryDbRow {
                                    title,
                                    cwd,
                                    updated_at: ts,
                                    steps,
                                    parent_id: clean_parent,
                                    is_sub,
                                    status,
                                });
                            }
                        }
                    }
                }
            }

            // 2. Discover sessions from brain & conversations directories
            let mut sids = HashSet::new();
            for sub in &["brain", "conversations", "presence"] {
                let sub_dir = fdir.join(sub);
                if let Ok(entries) = fs::read_dir(sub_dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        let sid = if p.is_dir() { p.file_name().and_then(|s| s.to_str()).unwrap_or("") } else { p.file_stem().and_then(|s| s.to_str()).unwrap_or("") };
                        if AGY_UUID_RE.is_match(sid) {
                            sids.insert(sid.to_string());
                        }
                    }
                }
            }

            for sid in db_meta.keys() {
                sids.insert(sid.clone());
            }

            for sid in sids {
                let mut all_paths = Vec::new();
                let brain_dir = fdir.join("brain").join(&sid);
                if brain_dir.exists() {
                    all_paths.push(brain_dir.to_string_lossy().to_string());
                }

                for ext in &[".db", ".db-wal", ".db-shm", ".pb"] {
                    let cp = fdir.join("conversations").join(format!("{}{}", sid, ext));
                    if cp.exists() {
                        all_paths.push(cp.to_string_lossy().to_string());
                    }
                }

                let lock_file = fdir.join("presence").join(format!("{}.lock", sid));
                let presence_running = lock_file.exists();
                if presence_running {
                    all_paths.push(lock_file.to_string_lossy().to_string());
                }

                let transcript_file = brain_dir.join(".system_generated").join("logs").join("transcript.jsonl");
                let has_transcript = transcript_file.exists();

                let mut title = String::new();
                let mut cwd = String::new();
                let mut updated_at = 0i64;
                let mut created_at = 0i64;
                let mut turn_count = 0u32;
                let mut parent_id = None;
                let mut is_subagent = false;
                let mut status_running = false;

                if let Some(row) = db_meta.get(&sid) {
                    title = row.title.clone();
                    cwd = row.cwd.clone();
                    updated_at = row.updated_at;
                    turn_count = row.steps;
                    parent_id = row.parent_id.clone();
                    is_subagent = row.is_sub;
                    status_running = row.status == "CASCADE_RUN_STATUS_RUNNING";
                }

                // Parse transcript.jsonl for tokens, fallback title, and timestamps
                let mut prompt_tokens = 0u64;
                let mut completion_tokens = 0u64;
                let mut cache_read_tokens = 0u64;

                if has_transcript {
                    if let Ok(file) = File::open(&transcript_file) {
                        let reader = BufReader::new(file);
                        for line in reader.lines().flatten() {
                            if line.trim().is_empty() {
                                continue;
                            }
                            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                                let mtype = v.get("type").and_then(Value::as_str).unwrap_or("");
                                if mtype == "USER_INPUT" {
                                    turn_count += 1;
                                    if created_at == 0 {
                                        if let Some(c_at) = v.get("created_at").and_then(Value::as_str) {
                                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(c_at) {
                                                created_at = dt.timestamp_millis();
                                            }
                                        }
                                    }
                                    if title.is_empty() {
                                        if let Some(content) = v.get("content").and_then(Value::as_str) {
                                            title = Self::clean_title(content);
                                            if cwd.is_empty() {
                                                if let Some(caps) = AGY_WS_RE.captures(content) {
                                                    cwd = caps.get(1).map_or("", |m| m.as_str()).to_string();
                                                }
                                            }
                                        }
                                    }
                                } else if mtype == "PLANNER_RESPONSE" {
                                    prompt_tokens += v.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
                                    completion_tokens += v.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
                                    cache_read_tokens += v.get("cache_read_tokens").and_then(Value::as_u64).unwrap_or(0);

                                    if let Some(c_at) = v.get("created_at").and_then(Value::as_str) {
                                        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(c_at) {
                                            updated_at = dt.timestamp_millis();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if title.is_empty() {
                    title = "(无标题会话)".to_string();
                }
                if created_at == 0 {
                    created_at = updated_at;
                }
                if updated_at == 0 {
                    let mtimes: Vec<i64> = all_paths.iter().filter_map(|p| {
                        Path::new(p).metadata().ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as i64)
                    }).collect();
                    updated_at = mtimes.into_iter().max().unwrap_or(0);
                    created_at = updated_at;
                }

                let total_size: u64 = all_paths.iter().map(|p| Self::path_size(Path::new(p))).sum();
                let main_path = if has_transcript {
                    transcript_file.to_string_lossy().to_string()
                } else {
                    all_paths.first().cloned().unwrap_or_default()
                };

                let is_running = presence_running || status_running;
                let token_stats = if prompt_tokens > 0 || completion_tokens > 0 {
                    Some(make_stats("gemini-2.5-pro", prompt_tokens, completion_tokens, cache_read_tokens, 0))
                } else {
                    None
                };

                summaries.push(SessionSummary {
                    id: sid,
                    platform: "antigravity".to_string(),
                    flavor: format!("agy-{}", flavor),
                    dirname: dir_name.to_string(),
                    main_path,
                    all_paths,
                    cwd,
                    title,
                    created_at,
                    updated_at,
                    size_bytes: total_size,
                    turn_count,
                    is_subagent,
                    parent_id,
                    is_running,
                    has_transcript,
                    token_stats,
                });
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
                    let mtype = val.get("type").and_then(Value::as_str).unwrap_or("");
                    let time_str = val.get("created_at").and_then(Value::as_str).unwrap_or("").to_string();

                    if mtype == "USER_INPUT" {
                        let raw = val.get("content").and_then(Value::as_str).unwrap_or("");
                        let text = if let Some(caps) = AGY_REQ_RE.captures(raw) {
                            caps.get(1).map_or(raw, |m| m.as_str()).trim().to_string()
                        } else {
                            raw.trim().to_string()
                        };

                        if !text.is_empty() {
                            msgs.push(ChatMessage {
                                role: "user".to_string(),
                                text,
                                time: time_str,
                                msg_type: "USER_INPUT".to_string(),
                                thinking: None,
                                tool_calls: Vec::new(),
                            });
                        }
                    } else if mtype == "PLANNER_RESPONSE" {
                        let text = val.get("content").and_then(Value::as_str).unwrap_or("").trim().to_string();
                        let thinking = val.get("thinking").and_then(Value::as_str).map(|s| s.trim().to_string());
                        let mut tool_calls = Vec::new();

                        if let Some(calls) = val.get("tool_calls").and_then(Value::as_array) {
                            for call in calls {
                                let name = call.get("name").and_then(Value::as_str).unwrap_or("tool").to_string();
                                let args = call.get("args").map(|a| serde_json::to_string_pretty(a).unwrap_or_default());
                                tool_calls.push(ToolCallItem { name, args, output: None });
                            }
                        }

                        if !text.is_empty() || !tool_calls.is_empty() || thinking.is_some() {
                            msgs.push(ChatMessage {
                                role: "assistant".to_string(),
                                text,
                                time: time_str,
                                msg_type: "PLANNER_RESPONSE".to_string(),
                                thinking,
                                tool_calls,
                            });
                        }
                    } else if mtype == "SUBAGENT_INVOCATION" {
                        let name = val.get("agent_name").or_else(|| val.get("role")).and_then(Value::as_str).unwrap_or("subagent");
                        msgs.push(ChatMessage {
                            role: "subagent".to_string(),
                            text: format!("调用子代理: {}", name),
                            time: time_str,
                            msg_type: "SUBAGENT".to_string(),
                            thinking: None,
                            tool_calls: Vec::new(),
                        });
                    }

                    if msgs.len() >= max_msgs {
                        break;
                    }
                }
            }
        }

        msgs
    }

    fn prune_indexes(&self, session_ids: &[String]) -> usize {
        let base = Self::gemini_home();
        let mut pruned = 0;
        let flavors = ["antigravity-cli", "antigravity", "antigravity-ide"];

        for dir_name in flavors {
            let db_path = base.join(dir_name).join("conversation_summaries.db");
            if db_path.exists() {
                if let Ok(conn) = Connection::open(&db_path) {
                    for chunk in session_ids.chunks(400) {
                        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                        let query = format!("DELETE FROM conversation_summaries WHERE conversation_id IN ({})", placeholders);
                        let params: Vec<&dyn rusqlite::ToSql> = chunk.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
                        if let Ok(count) = conn.execute(&query, rusqlite::params_from_iter(params)) {
                            pruned += count;
                        }
                    }
                }
            }
        }

        pruned
    }
}
