use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary};

pub struct CursorAdapter;

impl CursorAdapter {
    fn candidate_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(home) = crate::adapters::user_home() {
            let p_cursor = home.join(".cursor");
            if p_cursor.exists() {
                dirs.push(p_cursor);
            }
            let p_mac = home.join("Library/Application Support/Cursor");
            if p_mac.exists() {
                dirs.push(p_mac);
            }
            let p_linux = home.join(".config").join("Cursor");
            if p_linux.exists() {
                dirs.push(p_linux);
            }
        }
        if let Ok(appdata) = std::env::var("APPDATA") {
            let p_win = PathBuf::from(appdata).join("Cursor");
            if p_win.exists() {
                dirs.push(p_win);
            }
        }
        dirs
    }
}

impl AgentAdapter for CursorAdapter {
    fn platform_id(&self) -> &'static str {
        "cursor"
    }

    fn display_name(&self) -> &'static str {
        "Cursor"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let roots = Self::candidate_dirs();
        if roots.is_empty() {
            return Vec::new();
        }

        let mut summaries = Vec::new();
        let mut seen_ids = HashSet::new();

        for root in roots {
            // 1. Scan projects/**/agent-transcripts/*.jsonl
            let projects_dir = root.join("projects");
            if projects_dir.exists() {
                for entry in walkdir::WalkDir::new(&projects_dir).into_iter().flatten() {
                    let path = entry.path();
                    if !path.is_file() || path.extension().map_or(true, |ext| ext != "jsonl") {
                        continue;
                    }

                    let path_str = path.to_string_lossy();
                    if !path_str.contains("agent-transcripts") {
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
                                if let Some(c) = val.get("cwd").or_else(|| val.get("workspacePath")).and_then(Value::as_str) {
                                    if cwd.is_empty() {
                                        cwd = c.to_string();
                                    }
                                }

                                if let Some(role) = val.get("role").and_then(Value::as_str) {
                                    if role == "user" && title.is_empty() {
                                        if let Some(txt) = val.get("text").or_else(|| val.get("content")).and_then(Value::as_str) {
                                            if !txt.trim().is_empty() {
                                                title = txt.trim().to_string();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if title.is_empty() {
                        title = format!("Cursor 会话 {}", &raw_id[..raw_id.len().min(8)]);
                    }

                    let main_path_str = path.to_string_lossy().to_string();
                    summaries.push(SessionSummary {
                        id: raw_id,
                        platform: "cursor".to_string(),
                        flavor: "cursor".to_string(),
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

            // 2. Scan workspaceStorage/*/state.vscdb
            let storage_dir = root.join("User").join("workspaceStorage");
            if storage_dir.exists() {
                if let Ok(entries) = fs::read_dir(&storage_dir) {
                    for entry in entries.flatten() {
                        let db_path = entry.path().join("state.vscdb");
                        if !db_path.exists() {
                            continue;
                        }

                        if let Ok(conn) = Connection::open_with_flags(
                            &db_path,
                            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
                        ) {
                            let mut stmt = match conn.prepare("SELECT value FROM ItemTable WHERE key = 'composer.composerData'") {
                                Ok(s) => s,
                                Err(_) => continue,
                            };

                            let composer_data_res: rusqlite::Result<String> = stmt.query_row([], |row| row.get(0));
                            if let Ok(json_str) = composer_data_res {
                                if let Ok(val) = serde_json::from_str::<Value>(&json_str) {
                                    if let Some(composers) = val.get("allComposers").and_then(Value::as_array) {
                                        for comp in composers {
                                            let cid = comp.get("composerId").and_then(Value::as_str).unwrap_or("");
                                            if cid.is_empty() || seen_ids.contains(cid) {
                                                continue;
                                            }
                                            seen_ids.insert(cid.to_string());

                                            let name = comp.get("name").and_then(Value::as_str).unwrap_or("");
                                            let created_ms = comp.get("createdAt").and_then(Value::as_i64).unwrap_or(0);
                                            let created_sec = if created_ms > 0 { created_ms / 1000 } else { chrono::Utc::now().timestamp() };

                                            let title = if !name.is_empty() {
                                                name.to_string()
                                            } else {
                                                format!("Cursor 会话 {}", &cid[..cid.len().min(8)])
                                            };

                                            let main_path_str = db_path.to_string_lossy().to_string();
                                            summaries.push(SessionSummary {
                                                id: cid.to_string(),
                                                platform: "cursor".to_string(),
                                                flavor: "composer".to_string(),
                                                dirname: String::new(),
                                                main_path: main_path_str.clone(),
                                                all_paths: vec![main_path_str],
                                                cwd: String::new(),
                                                title,
                                                created_at: created_sec,
                                                updated_at: created_sec,
                                                size_bytes: 4096,
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
                        }
                    }
                }
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

        if path.extension().map_or(false, |ext| ext == "jsonl") {
            if let Ok(file) = File::open(path) {
                let reader = BufReader::new(file);
                for line in reader.lines().flatten() {
                    let line_str = line.trim();
                    if line_str.is_empty() {
                        continue;
                    }

                    if let Ok(val) = serde_json::from_str::<Value>(line_str) {
                        let role = val.get("role").and_then(Value::as_str).unwrap_or("user");
                        let text = val.get("text").or_else(|| val.get("content")).and_then(Value::as_str).unwrap_or("");
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
        } else if path.extension().map_or(false, |ext| ext == "vscdb") {
            if let Ok(conn) = Connection::open_with_flags(
                path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
            ) {
                if let Ok(mut stmt) = conn.prepare("SELECT value FROM ItemTable WHERE key = 'composer.composerData'") {
                    let json_res: rusqlite::Result<String> = stmt.query_row([], |row| row.get(0));
                    if let Ok(json_str) = json_res {
                        if let Ok(val) = serde_json::from_str::<Value>(&json_str) {
                            if let Some(composers) = val.get("allComposers").and_then(Value::as_array) {
                                for comp in composers {
                                    if comp.get("composerId").and_then(Value::as_str) == Some(&session.id) {
                                        if let Some(conv) = comp.get("conversation").and_then(Value::as_array) {
                                            for turn in conv {
                                                let role = turn.get("type").and_then(Value::as_str).unwrap_or("user");
                                                let text = turn.get("text").and_then(Value::as_str).unwrap_or("");
                                                if !text.is_empty() {
                                                    msgs.push(ChatMessage {
                                                        role: if role == "2" { "assistant".to_string() } else { "user".to_string() },
                                                        text: text.to_string(),
                                                        time: String::new(),
                                                        msg_type: "text".to_string(),
                                                        thinking: None,
                                                        tool_calls: Vec::new(),
                                                    });
                                                }
                                            }
                                        }
                                        break;
                                    }
                                }
                            }
                        }
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
