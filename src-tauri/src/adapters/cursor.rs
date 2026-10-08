use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::adapters::{to_millis, AgentAdapter};
use crate::models::{ChatMessage, SessionSummary};

pub struct CursorAdapter;

impl CursorAdapter {
    fn candidate_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(home) = crate::adapters::user_home() {
            // macOS / Linux general .cursor dotfolder
            let p_cursor = home.join(".cursor");
            if p_cursor.exists() {
                dirs.push(p_cursor);
            }
            // macOS Application Support
            let p_mac = home.join("Library/Application Support/Cursor");
            if p_mac.exists() {
                dirs.push(p_mac);
            }
            let p_mac_nightly = home.join("Library/Application Support/Cursor - Nightly");
            if p_mac_nightly.exists() {
                dirs.push(p_mac_nightly);
            }
            // Linux .config
            let p_linux = home.join(".config").join("Cursor");
            if p_linux.exists() {
                dirs.push(p_linux);
            }
        }
        // Windows APPDATA
        if let Ok(appdata) = std::env::var("APPDATA") {
            let p_win = PathBuf::from(appdata).join("Cursor");
            if p_win.exists() {
                dirs.push(p_win);
            }
        }
        dirs
    }

    /// Extract local workspace folder from workspace.json
    fn resolve_workspace_path(folder_path: &Path) -> (String, String) {
        let ws_json = folder_path.join("workspace.json");
        if ws_json.exists() {
            if let Ok(content) = fs::read_to_string(&ws_json) {
                if let Ok(val) = serde_json::from_str::<Value>(&content) {
                    if let Some(raw_uri) = val.get("folder").or_else(|| val.get("workspace")).and_then(Value::as_str) {
                        let clean = raw_uri.strip_prefix("file://").unwrap_or(raw_uri);
                        let decoded = urlencoding::decode(clean).unwrap_or(std::borrow::Cow::Borrowed(clean)).to_string();
                        #[cfg(target_os = "windows")]
                        let decoded = if decoded.starts_with('/') && decoded.chars().nth(2) == Some(':') {
                            decoded[1..].to_string()
                        } else {
                            decoded
                        };
                        let dir_name = Path::new(&decoded)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string();
                        return (decoded, dir_name);
                    }
                }
            }
        }
        (String::new(), String::new())
    }

    /// Helper to query value from SQLite checking ItemTable or cursorDiskKV
    fn query_sqlite_key(conn: &Connection, key: &str) -> Option<String> {
        let sql_item = "SELECT value FROM ItemTable WHERE key = ?1";
        if let Ok(mut stmt) = conn.prepare(sql_item) {
            if let Ok(val) = stmt.query_row([key], |row| row.get(0)) {
                return Some(val);
            }
        }
        let sql_disk = "SELECT value FROM cursorDiskKV WHERE key = ?1";
        if let Ok(mut stmt) = conn.prepare(sql_disk) {
            if let Ok(val) = stmt.query_row([key], |row| row.get(0)) {
                return Some(val);
            }
        }
        None
    }

    /// Helper to update value in SQLite checking ItemTable or cursorDiskKV
    fn update_sqlite_key(conn: &Connection, key: &str, val: &str) -> bool {
        let mut updated = false;
        let sql_item = "UPDATE ItemTable SET value = ?1 WHERE key = ?2";
        if let Ok(mut stmt) = conn.prepare(sql_item) {
            if let Ok(rows) = stmt.execute([val, key]) {
                if rows > 0 {
                    updated = true;
                }
            }
        }
        let sql_disk = "UPDATE cursorDiskKV SET value = ?1 WHERE key = ?2";
        if let Ok(mut stmt) = conn.prepare(sql_disk) {
            if let Ok(rows) = stmt.execute([val, key]) {
                if rows > 0 {
                    updated = true;
                }
            }
        }
        updated
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
            // 1. Scan projects/**/agent-transcripts/*.jsonl and transcripts/*.jsonl
            let scan_dirs = [root.join("projects"), root.join("transcripts"), root.clone()];
            for base_dir in scan_dirs {
                if !base_dir.exists() {
                    continue;
                }
                for entry in walkdir::WalkDir::new(&base_dir).into_iter().flatten() {
                    let path = entry.path();
                    if !path.is_file() || path.extension().map_or(true, |ext| ext != "jsonl") {
                        continue;
                    }

                    let path_str = path.to_string_lossy();
                    if !path_str.contains("agent-transcripts") && !path_str.contains("transcripts") {
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
                    let created_at = to_millis(mtime_sec);
                    let updated_at = to_millis(mtime_sec);
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

                    let dirname = if !cwd.is_empty() {
                        Path::new(&cwd)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string()
                    } else {
                        String::new()
                    };

                    if title.is_empty() {
                        title = format!("Cursor Agent {}", &raw_id[..raw_id.len().min(8)]);
                    }

                    let main_path_str = path.to_string_lossy().to_string();
                    summaries.push(SessionSummary {
                        id: raw_id,
                        platform: "cursor".to_string(),
                        flavor: "agent".to_string(),
                        dirname,
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

            // 2. Scan workspaceStorage/*/state.vscdb and globalStorage/state.vscdb
            let mut db_targets = Vec::new();
            let storage_dir = root.join("User").join("workspaceStorage");
            if storage_dir.exists() {
                if let Ok(entries) = fs::read_dir(&storage_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let db_path = path.join("state.vscdb");
                        if db_path.exists() {
                            let (cwd, dirname) = Self::resolve_workspace_path(&path);
                            db_targets.push((db_path, cwd, dirname));
                        }
                    }
                }
            }

            let global_db = root.join("User").join("globalStorage").join("state.vscdb");
            if global_db.exists() {
                db_targets.push((global_db, String::new(), String::new()));
            }

            for (db_path, cwd, dirname) in db_targets {
                let conn = match Connection::open_with_flags(
                    &db_path,
                    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
                ) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let db_size = fs::metadata(&db_path).map(|m| m.len()).unwrap_or(4096);
                let db_path_str = db_path.to_string_lossy().to_string();

                // (A) Scan Composer Data
                if let Some(json_str) = Self::query_sqlite_key(&conn, "composer.composerData") {
                    if let Ok(val) = serde_json::from_str::<Value>(&json_str) {
                        if let Some(composers) = val.get("allComposers").and_then(Value::as_array) {
                            for comp in composers {
                                let cid = comp.get("composerId").and_then(Value::as_str).unwrap_or("");
                                if cid.is_empty() || seen_ids.contains(cid) {
                                    continue;
                                }
                                seen_ids.insert(cid.to_string());

                                let name = comp.get("name").and_then(Value::as_str).unwrap_or("");
                                let created_raw = comp.get("createdAt").and_then(Value::as_i64).unwrap_or(0);
                                let created_at = if created_raw > 0 { to_millis(created_raw) } else { chrono::Utc::now().timestamp_millis() };

                                let turn_count = comp.get("conversation")
                                    .and_then(Value::as_array)
                                    .map(|arr| arr.len() as u32)
                                    .unwrap_or(1);

                                let title = if !name.is_empty() {
                                    name.to_string()
                                } else if !dirname.is_empty() {
                                    format!("{}: Composer {}", dirname, &cid[..cid.len().min(6)])
                                } else {
                                    format!("Composer 会话 {}", &cid[..cid.len().min(8)])
                                };

                                summaries.push(SessionSummary {
                                    id: cid.to_string(),
                                    platform: "cursor".to_string(),
                                    flavor: "composer".to_string(),
                                    dirname: dirname.clone(),
                                    main_path: db_path_str.clone(),
                                    all_paths: vec![],
                                    cwd: cwd.clone(),
                                    title,
                                    created_at,
                                    updated_at: created_at,
                                    size_bytes: db_size,
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
                }

                // (B) Scan Chat Data (workbench.panel.aichat.chatdata)
                if let Some(json_str) = Self::query_sqlite_key(&conn, "workbench.panel.aichat.chatdata") {
                    if let Ok(val) = serde_json::from_str::<Value>(&json_str) {
                        if let Some(tabs) = val.get("tabs").and_then(Value::as_array) {
                            for tab in tabs {
                                let tid = tab.get("tabId").and_then(Value::as_str).unwrap_or("");
                                if tid.is_empty() || seen_ids.contains(tid) {
                                    continue;
                                }
                                seen_ids.insert(tid.to_string());

                                let chat_title = tab.get("chatTitle").and_then(Value::as_str).unwrap_or("");
                                let bubbles = tab.get("bubbles").and_then(Value::as_array);
                                let turn_count = bubbles.map(|b| b.len() as u32).unwrap_or(1);

                                let title = if !chat_title.is_empty() {
                                    chat_title.to_string()
                                } else if !dirname.is_empty() {
                                    format!("{}: Chat {}", dirname, &tid[..tid.len().min(6)])
                                } else {
                                    format!("Cursor 对话 {}", &tid[..tid.len().min(8)])
                                };

                                let now_ms = chrono::Utc::now().timestamp_millis();
                                summaries.push(SessionSummary {
                                    id: tid.to_string(),
                                    platform: "cursor".to_string(),
                                    flavor: "chat".to_string(),
                                    dirname: dirname.clone(),
                                    main_path: db_path_str.clone(),
                                    all_paths: vec![],
                                    cwd: cwd.clone(),
                                    title,
                                    created_at: now_ms,
                                    updated_at: now_ms,
                                    size_bytes: db_size,
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
                // Check Composer Data
                if session.flavor == "composer" || session.flavor.is_empty() {
                    if let Some(json_str) = Self::query_sqlite_key(&conn, "composer.composerData") {
                        if let Ok(val) = serde_json::from_str::<Value>(&json_str) {
                            if let Some(composers) = val.get("allComposers").and_then(Value::as_array) {
                                for comp in composers {
                                    if comp.get("composerId").and_then(Value::as_str) == Some(&session.id) {
                                        if let Some(conv) = comp.get("conversation").and_then(Value::as_array) {
                                            for turn in conv {
                                                let role_code = turn.get("type").and_then(Value::as_str).unwrap_or("1");
                                                let text = turn.get("text")
                                                    .or_else(|| turn.get("richText"))
                                                    .and_then(Value::as_str)
                                                    .unwrap_or("");
                                                if !text.is_empty() {
                                                    msgs.push(ChatMessage {
                                                        role: if role_code == "2" { "assistant".to_string() } else { "user".to_string() },
                                                        text: text.to_string(),
                                                        time: String::new(),
                                                        msg_type: "text".to_string(),
                                                        thinking: None,
                                                        tool_calls: Vec::new(),
                                                    });
                                                }
                                                if msgs.len() >= max_msgs {
                                                    break;
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

                // Check AI Chat Data
                if msgs.is_empty() && (session.flavor == "chat" || session.flavor.is_empty()) {
                    if let Some(json_str) = Self::query_sqlite_key(&conn, "workbench.panel.aichat.chatdata") {
                        if let Ok(val) = serde_json::from_str::<Value>(&json_str) {
                            if let Some(tabs) = val.get("tabs").and_then(Value::as_array) {
                                for tab in tabs {
                                    if tab.get("tabId").and_then(Value::as_str) == Some(&session.id) {
                                        if let Some(bubbles) = tab.get("bubbles").and_then(Value::as_array) {
                                            for b in bubbles {
                                                let b_type = b.get("type").and_then(Value::as_str).unwrap_or("user");
                                                let text = b.get("rawText")
                                                    .or_else(|| b.get("text"))
                                                    .and_then(Value::as_str)
                                                    .unwrap_or("");
                                                if !text.is_empty() {
                                                    msgs.push(ChatMessage {
                                                        role: if b_type == "ai" { "assistant".to_string() } else { "user".to_string() },
                                                        text: text.to_string(),
                                                        time: String::new(),
                                                        msg_type: "text".to_string(),
                                                        thinking: None,
                                                        tool_calls: Vec::new(),
                                                    });
                                                }
                                                if msgs.len() >= max_msgs {
                                                    break;
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

    fn prune_indexes(&self, session_ids: &[String]) -> usize {
        if session_ids.is_empty() {
            return 0;
        }
        let set: HashSet<&str> = session_ids.iter().map(|s| s.as_str()).collect();
        let roots = Self::candidate_dirs();
        let mut pruned = 0;

        for root in &roots {
            let mut db_targets = Vec::new();
            let storage_dir = root.join("User").join("workspaceStorage");
            if storage_dir.exists() {
                if let Ok(entries) = fs::read_dir(&storage_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let db_path = path.join("state.vscdb");
                        if db_path.exists() {
                            db_targets.push(db_path);
                        }
                    }
                }
            }
            let global_db = root.join("User").join("globalStorage").join("state.vscdb");
            if global_db.exists() {
                db_targets.push(global_db);
            }

            for db_path in db_targets {
                let conn = match Connection::open(&db_path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let mut db_modified = false;

                // 1. Prune composer.composerData
                if let Some(json_str) = Self::query_sqlite_key(&conn, "composer.composerData") {
                    if let Ok(mut val) = serde_json::from_str::<Value>(&json_str) {
                        if let Some(composers) = val.get_mut("allComposers").and_then(Value::as_array_mut) {
                            let orig_len = composers.len();
                            composers.retain(|c| {
                                let cid = c.get("composerId").and_then(Value::as_str).unwrap_or("");
                                !set.contains(cid)
                            });
                            if composers.len() < orig_len {
                                pruned += orig_len - composers.len();
                                if let Ok(new_json) = serde_json::to_string(&val) {
                                    Self::update_sqlite_key(&conn, "composer.composerData", &new_json);
                                    db_modified = true;
                                }
                            }
                        }
                    }
                }

                // 2. Prune workbench.panel.aichat.chatdata
                if let Some(json_str) = Self::query_sqlite_key(&conn, "workbench.panel.aichat.chatdata") {
                    if let Ok(mut val) = serde_json::from_str::<Value>(&json_str) {
                        if let Some(tabs) = val.get_mut("tabs").and_then(Value::as_array_mut) {
                            let orig_len = tabs.len();
                            tabs.retain(|t| {
                                let tid = t.get("tabId").and_then(Value::as_str).unwrap_or("");
                                !set.contains(tid)
                            });
                            if tabs.len() < orig_len {
                                pruned += orig_len - tabs.len();
                                if let Ok(new_json) = serde_json::to_string(&val) {
                                    Self::update_sqlite_key(&conn, "workbench.panel.aichat.chatdata", &new_json);
                                    db_modified = true;
                                }
                            }
                        }
                    }
                }

                if db_modified {
                    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
                }
            }

            // 3. Remove agent transcripts if present
            let scan_dirs = [root.join("projects"), root.join("transcripts"), root.clone()];
            for base_dir in scan_dirs {
                if !base_dir.exists() {
                    continue;
                }
                for entry in walkdir::WalkDir::new(&base_dir).into_iter().flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().map_or(false, |e| e == "jsonl") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            if set.contains(stem) {
                                let _ = fs::remove_file(path);
                                pruned += 1;
                            }
                        }
                    }
                }
            }
        }

        pruned
    }
}
