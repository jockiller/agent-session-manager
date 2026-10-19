use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::adapters::{to_millis, AgentAdapter};
use crate::models::{ChatMessage, SessionSummary, ToolCallItem};

pub struct OpenCodeAdapter {
    platform: &'static str,
    name: &'static str,
    dirs: Vec<PathBuf>,
}

impl OpenCodeAdapter {
    pub fn new_opencode() -> Self {
        let mut dirs = Vec::new();
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            dirs.push(PathBuf::from(xdg).join("opencode"));
        }
        if let Some(home) = crate::adapters::user_home() {
            dirs.push(home.join(".local").join("share").join("opencode"));
            dirs.push(home.join(".opencode"));
            dirs.push(home.join("Library").join("Application Support").join("opencode"));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("opencode"));
        }
        Self {
            platform: "opencode",
            name: "OpenCode",
            dirs,
        }
    }

    pub fn new_codewiz() -> Self {
        let mut dirs = Vec::new();
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            dirs.push(PathBuf::from(xdg).join("codewiz"));
        }
        if let Some(home) = crate::adapters::user_home() {
            dirs.push(home.join(".local").join("share").join("codewiz"));
            dirs.push(home.join(".codewiz"));
            dirs.push(home.join("Library").join("Application Support").join("codewiz"));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("codewiz"));
        }
        Self {
            platform: "codewiz",
            name: "CodeWiz",
            dirs,
        }
    }

    #[cfg(test)]
    pub fn with_dirs(platform: &'static str, name: &'static str, dirs: Vec<PathBuf>) -> Self {
        Self {
            platform,
            name,
            dirs,
        }
    }

    fn find_db(&self) -> Option<PathBuf> {
        let plat_db = format!("{}.db", self.platform);
        for dir in &self.dirs {
            let candidate_plat = dir.join(&plat_db);
            if candidate_plat.exists() {
                return Some(candidate_plat);
            }
            let candidate_opencode = dir.join("opencode.db");
            if candidate_opencode.exists() {
                return Some(candidate_opencode);
            }
            let candidate1 = dir.join("db.sqlite");
            if candidate1.exists() {
                return Some(candidate1);
            }
            let candidate2 = dir.join("state.db");
            if candidate2.exists() {
                return Some(candidate2);
            }
        }
        None
    }
}

fn is_v2_schema(conn: &Connection) -> bool {
    conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'session_v2'")
        .and_then(|mut s| s.exists([]))
        .unwrap_or(false)
}

struct V2SessionStats {
    user_turns: u32,
    data_bytes: u64,
}

fn load_v2_stats(conn: &Connection) -> HashMap<String, V2SessionStats> {
    let mut map = HashMap::new();
    let mut stmt = match conn.prepare(
        r#"
        SELECT 
            session_id, 
            count(CASE WHEN type = 'user' THEN 1 END), 
            coalesce(sum(length(data)), 0) 
        FROM session_message 
        GROUP BY session_id
        "#,
    ) {
        Ok(s) => s,
        Err(_) => return map,
    };
    let rows = match stmt.query_map([], |row| {
        let sid: String = row.get(0)?;
        let turns: u32 = row.get(1)?;
        let bytes: i64 = row.get(2)?;
        Ok((sid, V2SessionStats {
            user_turns: turns,
            data_bytes: bytes.max(0) as u64,
        }))
    }) {
        Ok(r) => r,
        Err(_) => return map,
    };
    for (sid, stats) in rows.flatten() {
        map.insert(sid, stats);
    }
    map
}

/// Sessions the running OpenCode process currently owns a drain for. Mirrors the
/// `GET /api/session/active` endpoint: a session is active exactly while its drain is
/// suspended, i.e. while `time_suspended` is set.
fn load_running_v2(conn: &Connection) -> HashSet<String> {
    let mut set = HashSet::new();
    let mut stmt = match conn.prepare("SELECT id FROM session_v2 WHERE time_suspended IS NOT NULL")
    {
        Ok(s) => s,
        Err(_) => return set,
    };
    let rows = match stmt.query_map([], |row| row.get::<_, String>(0)) {
        Ok(r) => r,
        Err(_) => return set,
    };
    for id in rows.flatten() {
        set.insert(id);
    }
    set
}

/// Collect a session id together with all of its descendant (child) sessions.
/// OpenCode deletes a session and its child sessions as one unit.
fn collect_session_family(conn: &Connection, root: &str, is_v2: bool) -> Vec<String> {
    let table = if is_v2 { "session_v2" } else { "session" };
    let sql = format!(
        "WITH RECURSIVE family(id) AS (
             SELECT id FROM {table} WHERE id = ?1
             UNION
             SELECT s.id FROM {table} s JOIN family f ON s.parent_id = f.id
         )
         SELECT id FROM family",
        table = table
    );

    let mut ids = Vec::new();
    if let Ok(mut stmt) = conn.prepare(&sql) {
        if let Ok(rows) = stmt.query_map([root], |row| row.get::<_, String>(0)) {
            for id in rows.flatten() {
                ids.push(id);
            }
        }
    }
    ids
}

fn load_first_prompts_v2(conn: &Connection) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut stmt = match conn.prepare(
        r#"
        SELECT session_id, data
        FROM session_message
        WHERE type = 'user'
        ORDER BY session_id, seq ASC
        "#,
    ) {
        Ok(s) => s,
        Err(_) => return map,
    };
    let rows = match stmt.query_map([], |row| {
        let sid: String = row.get(0)?;
        let data: String = row.get(1)?;
        Ok((sid, data))
    }) {
        Ok(r) => r,
        Err(_) => return map,
    };
    for (sid, data) in rows.flatten() {
        if map.contains_key(&sid) {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<Value>(&data) {
            if let Some(text) = val.get("text").and_then(Value::as_str) {
                if let Some(fp) = crate::adapters::make_first_prompt(text) {
                    map.insert(sid, fp);
                }
            }
        }
    }
    map
}

/// Legacy v1: first user message text for every session
fn load_first_prompts_v1(conn: &Connection) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut stmt = match conn.prepare(
        r#"
        SELECT session_id, data
        FROM message
        ORDER BY session_id, time_created, id
        "#,
    ) {
        Ok(s) => s,
        Err(_) => return map,
    };
    let rows = match stmt.query_map([], |row| {
        let sid: String = row.get(0)?;
        let data: String = row.get(1)?;
        Ok((sid, data))
    }) {
        Ok(r) => r,
        Err(_) => return map,
    };
    for (sid, data) in rows.flatten() {
        if map.contains_key(&sid) {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<Value>(&data) {
            let role = val.get("role").or_else(|| val.get("type")).and_then(Value::as_str).unwrap_or("");
            if role == "user" {
                if let Some(content) = val.get("content") {
                    if let Some(text) = crate::adapters::user_text_from_content(content) {
                        if let Some(fp) = crate::adapters::make_first_prompt(&text) {
                            map.insert(sid, fp);
                        }
                    }
                }
            }
        }
    }
    map
}

fn format_message_time(column_ts: i64, val: &Value) -> String {
    let ts = val.get("time")
        .and_then(|t| t.get("created"))
        .and_then(Value::as_i64)
        .unwrap_or(column_ts);

    if ts <= 0 {
        return String::new();
    }
    let sec = if ts > 100_000_000_000 { ts / 1000 } else { ts };
    chrono::DateTime::from_timestamp(sec, 0)
        .map(|d| d.to_rfc3339())
        .unwrap_or_default()
}

impl AgentAdapter for OpenCodeAdapter {
    fn platform_id(&self) -> &'static str {
        self.platform
    }

    fn display_name(&self) -> &'static str {
        self.name
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let db_path = match self.find_db() {
            Some(p) => p,
            None => return Vec::new(),
        };

        let conn = match Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        ) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let main_path_str = db_path.to_string_lossy().to_string();

        if is_v2_schema(&conn) {
            let mut stmt = match conn.prepare(
                r#"
                SELECT 
                    s.id, 
                    COALESCE(s.title, ''), 
                    COALESCE(s.directory, ''), 
                    s.parent_id, 
                    COALESCE(s.time_created, 0), 
                    COALESCE(s.time_updated, s.time_created, 0),
                    COALESCE(s.model, ''),
                    COALESCE(s.tokens_input, 0),
                    COALESCE(s.tokens_output, 0),
                    COALESCE(s.tokens_reasoning, 0),
                    COALESCE(s.tokens_cache_read, 0),
                    COALESCE(s.tokens_cache_write, 0)
                FROM session_v2 s
                ORDER BY COALESCE(s.time_updated, s.time_created, 0) DESC
                "#,
            ) {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };

            let mut summaries = Vec::new();
            let first_prompts = load_first_prompts_v2(&conn);
            let msg_stats = load_v2_stats(&conn);
            let running = load_running_v2(&conn);

            let rows = match stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let directory: String = row.get(2)?;
                let parent_id: Option<String> = row.get(3)?;
                let time_created: i64 = row.get(4)?;
                let time_updated: i64 = row.get(5)?;
                let model: String = row.get(6)?;
                let tokens_input: i64 = row.get(7)?;
                let tokens_output: i64 = row.get(8)?;
                let tokens_reasoning: i64 = row.get(9)?;
                let tokens_cache_read: i64 = row.get(10)?;
                let tokens_cache_write: i64 = row.get(11)?;
                Ok((
                    id,
                    title,
                    directory,
                    parent_id,
                    time_created,
                    time_updated,
                    model,
                    tokens_input,
                    tokens_output,
                    tokens_reasoning,
                    tokens_cache_read,
                    tokens_cache_write,
                ))
            }) {
                Ok(r) => r,
                Err(_) => return summaries,
            };

            for r in rows.flatten() {
                let (
                    id,
                    mut title,
                    directory,
                    parent_id,
                    time_created,
                    time_updated,
                    model,
                    tokens_input,
                    tokens_output,
                    tokens_reasoning,
                    tokens_cache_read,
                    tokens_cache_write,
                ) = r;

                let created_at = to_millis(time_created);
                let updated_at = to_millis(time_updated);

                if title.is_empty() {
                    title = format!("{} 会话 {}", self.name, &id[..id.len().min(8)]);
                }

                let is_subagent = parent_id.is_some();
                let stat = msg_stats.get(&id);
                let turn_count = stat.map(|s| s.user_turns).unwrap_or(0).max(1);
                let size_bytes = stat.map(|s| s.data_bytes).unwrap_or(0).max(4096);

                let token_stats = if tokens_input > 0
                    || tokens_output > 0
                    || tokens_cache_read > 0
                    || tokens_cache_write > 0
                    || tokens_reasoning > 0
                {
                    Some(crate::services::token_calc::make_stats_extended(
                        &model,
                        tokens_input.max(0) as u64,
                        tokens_output.max(0) as u64,
                        tokens_cache_read.max(0) as u64,
                        tokens_cache_write.max(0) as u64,
                        tokens_reasoning.max(0) as u64,
                        None,
                    ))
                } else {
                    None
                };

                summaries.push(SessionSummary {
                    id: id.clone(),
                    platform: self.platform.to_string(),
                    flavor: self.platform.to_string(),
                    dirname: directory.clone(),
                    main_path: main_path_str.clone(),
                    all_paths: vec![],
                    cwd: directory,
                    title,
                    first_prompt: first_prompts.get(&id).cloned().unwrap_or_default(),
                    created_at,
                    updated_at,
                    size_bytes,
                    turn_count,
                    is_subagent,
                    parent_id,
                    is_running: running.contains(&id),
                    is_archived: false,
                    has_transcript: true,
                    token_stats,
                });
            }

            return summaries;
        }

        // Fallback: Legacy v1 schema
        let mut stmt = match conn.prepare(
            r#"
            SELECT 
                s.id, 
                COALESCE(s.title, ''), 
                COALESCE(s.directory, ''), 
                s.parent_id, 
                COALESCE(s.time_created, 0), 
                COALESCE(s.time_updated, s.time_created, 0)
            FROM session s
            ORDER BY COALESCE(s.time_updated, s.time_created, 0) DESC
            "#,
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let mut summaries = Vec::new();
        let first_prompts = load_first_prompts_v1(&conn);
        let rows = match stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let directory: String = row.get(2)?;
            let parent_id: Option<String> = row.get(3)?;
            let time_created: i64 = row.get(4)?;
            let time_updated: i64 = row.get(5)?;
            Ok((id, title, directory, parent_id, time_created, time_updated))
        }) {
            Ok(r) => r,
            Err(_) => return summaries,
        };

        for r in rows.flatten() {
            let (id, mut title, directory, parent_id, time_created, time_updated) = r;
            let created_at = to_millis(time_created);
            let updated_at = to_millis(time_updated);

            if title.is_empty() {
                title = format!("{} 会话 {}", self.name, &id[..id.len().min(8)]);
            }

            let is_subagent = parent_id.is_some();

            summaries.push(SessionSummary {
                id: id.clone(),
                platform: self.platform.to_string(),
                flavor: self.platform.to_string(),
                dirname: directory.clone(),
                main_path: main_path_str.clone(),
                all_paths: vec![],
                cwd: directory,
                title,
                first_prompt: first_prompts.get(&id).cloned().unwrap_or_default(),
                created_at,
                updated_at,
                size_bytes: 4096,
                turn_count: 1,
                is_subagent,
                parent_id,
                is_running: false,
                is_archived: false,
                has_transcript: true,
                token_stats: None,
            });
        }

        summaries
    }

    fn load_messages(&self, session: &SessionSummary, max_msgs: usize) -> Vec<ChatMessage> {
        let db_path = match self.find_db() {
            Some(p) => p,
            None => return Vec::new(),
        };

        let conn = match Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        ) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut msgs = Vec::new();

        if is_v2_schema(&conn) {
            let mut stmt = match conn.prepare(
                r#"
                SELECT type, data, time_created
                FROM session_message
                WHERE session_id = ?
                ORDER BY seq ASC, time_created ASC
                LIMIT ?
                "#,
            ) {
                Ok(s) => s,
                Err(_) => return msgs,
            };

            let rows = match stmt.query_map([&session.id, &max_msgs.to_string()], |row| {
                let msg_type: String = row.get(0)?;
                let data: String = row.get(1)?;
                let time_created: i64 = row.get(2)?;
                Ok((msg_type, data, time_created))
            }) {
                Ok(r) => r,
                Err(_) => return msgs,
            };

            for r in rows.flatten() {
                let (mtype, data_str, ts) = r;
                if let Ok(val) = serde_json::from_str::<Value>(&data_str) {
                    let time_s = format_message_time(ts, &val);

                    match mtype.as_str() {
                        "user" => {
                            let text = val.get("text").and_then(Value::as_str).unwrap_or("").trim();
                            if !text.is_empty() {
                                msgs.push(ChatMessage {
                                    role: "user".to_string(),
                                    text: text.to_string(),
                                    time: time_s,
                                    msg_type: "text".to_string(),
                                    thinking: None,
                                    tool_calls: Vec::new(),
                                });
                            }
                        }
                        "assistant" => {
                            let mut text = String::new();
                            let mut thinking = String::new();
                            let mut tool_calls = Vec::new();

                            if let Some(content_arr) = val.get("content").and_then(Value::as_array) {
                                for item in content_arr {
                                    let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
                                    match item_type {
                                        "text" => {
                                            if let Some(t) = item.get("text").and_then(Value::as_str) {
                                                if !text.is_empty() {
                                                    text.push('\n');
                                                }
                                                text.push_str(t);
                                            }
                                        }
                                        "reasoning" => {
                                            if let Some(t) = item.get("text").and_then(Value::as_str) {
                                                if !thinking.is_empty() {
                                                    thinking.push('\n');
                                                }
                                                thinking.push_str(t);
                                            }
                                        }
                                        "tool" => {
                                            let name = item.get("name").and_then(Value::as_str).unwrap_or("tool");
                                            let args = item.get("state").and_then(|s| s.get("input")).map(|inp| {
                                                if let Some(s) = inp.as_str() {
                                                    s.to_string()
                                                } else {
                                                    serde_json::to_string(inp).unwrap_or_default()
                                                }
                                            });
                                            let output = item.get("state").and_then(|s| s.get("content")).and_then(|c| {
                                                if let Some(s) = c.as_str() {
                                                    Some(s.to_string())
                                                } else if let Some(arr) = c.as_array() {
                                                    let mut out = String::new();
                                                    for part in arr {
                                                        if let Some(t) = part.get("text").and_then(Value::as_str) {
                                                            if !out.is_empty() {
                                                                out.push('\n');
                                                            }
                                                            out.push_str(t);
                                                        }
                                                    }
                                                    if out.is_empty() { None } else { Some(out) }
                                                } else {
                                                    None
                                                }
                                            });

                                            tool_calls.push(ToolCallItem {
                                                name: name.to_string(),
                                                args,
                                                output,
                                            });
                                        }
                                        _ => {}
                                    }
                                }
                            }

                            let text = text.trim().to_string();
                            let thinking = if thinking.trim().is_empty() {
                                None
                            } else {
                                Some(thinking.trim().to_string())
                            };

                            let is_tool_only = !tool_calls.is_empty() && text.is_empty();

                            if !text.is_empty() || !tool_calls.is_empty() || thinking.is_some() {
                                msgs.push(ChatMessage {
                                    role: "assistant".to_string(),
                                    text,
                                    time: time_s,
                                    msg_type: if is_tool_only {
                                        "tool_use".to_string()
                                    } else {
                                        "text".to_string()
                                    },
                                    thinking,
                                    tool_calls,
                                });
                            }
                        }
                        "compaction" => {
                            if let Some(summary) = val.get("summary").and_then(Value::as_str) {
                                msgs.push(ChatMessage {
                                    role: "system".to_string(),
                                    text: format!("[会话压缩摘要]\n{}", summary.trim()),
                                    time: time_s,
                                    msg_type: "compaction".to_string(),
                                    thinking: None,
                                    tool_calls: Vec::new(),
                                });
                            }
                        }
                        "system" => {
                            if let Some(text) = val.get("text").and_then(Value::as_str) {
                                msgs.push(ChatMessage {
                                    role: "system".to_string(),
                                    text: text.trim().to_string(),
                                    time: time_s,
                                    msg_type: "system".to_string(),
                                    thinking: None,
                                    tool_calls: Vec::new(),
                                });
                            }
                        }
                        "synthetic" => {
                            if let Some(text) = val.get("text").and_then(Value::as_str) {
                                msgs.push(ChatMessage {
                                    role: "user".to_string(),
                                    text: text.trim().to_string(),
                                    time: time_s,
                                    msg_type: "synthetic".to_string(),
                                    thinking: None,
                                    tool_calls: Vec::new(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }

            return msgs;
        }

        // Legacy v1 schema
        let mut stmt = match conn.prepare(
            r#"
            SELECT data, time_created
            FROM message
            WHERE session_id = ?
            ORDER BY time_created ASC
            LIMIT ?
            "#,
        ) {
            Ok(s) => s,
            Err(_) => return msgs,
        };

        let rows = match stmt.query_map([&session.id, &max_msgs.to_string()], |row| {
            let data: String = row.get(0)?;
            let time_created: i64 = row.get(1)?;
            Ok((data, time_created))
        }) {
            Ok(r) => r,
            Err(_) => return msgs,
        };

        for r in rows.flatten() {
            let (data_str, ts) = r;
            if let Ok(val) = serde_json::from_str::<Value>(&data_str) {
                let role = val.get("role").or_else(|| val.get("type")).and_then(Value::as_str).unwrap_or("user");
                let mut text = String::new();
                if let Some(content) = val.get("content").and_then(Value::as_str) {
                    text = content.to_string();
                } else if let Some(content_arr) = val.get("content").and_then(Value::as_array) {
                    for item in content_arr {
                        if let Some(t) = item.get("text").and_then(Value::as_str) {
                            text.push_str(t);
                            text.push('\n');
                        }
                    }
                }

                if !text.trim().is_empty() {
                    let time_s = if ts > 0 {
                        let sec = if ts > 10_000_000_000 { ts / 1000 } else { ts };
                        chrono::DateTime::from_timestamp(sec, 0)
                            .map(|d| d.to_rfc3339())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };

                    msgs.push(ChatMessage {
                        role: role.to_string(),
                        text: text.trim().to_string(),
                        time: time_s,
                        msg_type: "text".to_string(),
                        thinking: None,
                        tool_calls: Vec::new(),
                    });
                }
            }
        }

        msgs
    }

    fn prune_indexes(&self, session_ids: &[String]) -> usize {
        if session_ids.is_empty() {
            return 0;
        }

        let mut pruned = 0;
        if let Some(db_path) = self.find_db() {
            if let Ok(mut conn) = Connection::open(&db_path) {
                let is_v2 = is_v2_schema(&conn);
                let table = if is_v2 { "session_v2" } else { "session" };

                // The whole batch runs as one transaction, so a commit that fails leaves the store
                // untouched instead of half-deleted. Child rows are cleared explicitly because this
                // connection does not enable foreign key enforcement.
                let mut families: Vec<Vec<String>> = Vec::new();
                if let Ok(tx) = conn.transaction() {
                    for id in session_ids {
                        // OpenCode removes a session together with its child sessions.
                        let mut family = collect_session_family(&tx, id, is_v2);
                        if family.is_empty() {
                            // The lookup itself failed (or the session is already gone); fall back to
                            // the requested id so the row is still removed and verified below.
                            family.push(id.clone());
                        }
                        for sid in &family {
                            if is_v2 {
                                let _ = tx.execute("DELETE FROM session_message WHERE session_id = ?", [sid]);
                                let _ = tx.execute("DELETE FROM session_inbox WHERE session_id = ?", [sid]);
                                let _ = tx.execute("DELETE FROM session_pending WHERE session_id = ?", [sid]);
                                let _ = tx.execute("DELETE FROM instruction_entry WHERE session_id = ?", [sid]);
                                let _ = tx.execute("DELETE FROM instruction_state WHERE session_id = ?", [sid]);
                            } else {
                                let _ = tx.execute("DELETE FROM part WHERE message_id IN (SELECT id FROM message WHERE session_id = ?)", [sid]);
                                let _ = tx.execute("DELETE FROM message WHERE session_id = ?", [sid]);
                            }
                        }
                        for sid in &family {
                            let _ = tx.execute(&format!("DELETE FROM {} WHERE id = ?", table), [sid]);
                        }
                        families.push(family);
                    }
                    if tx.commit().is_err() {
                        // Nothing was written, so nothing may be reported as removed.
                        families.clear();
                    }
                }

                let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");

                // Only report a session as removed once the store confirms its whole family is gone,
                // so a partially applied batch is reported as a failure rather than a success.
                for family in &families {
                    let mut remaining = 0i64;
                    for sid in family {
                        remaining += conn
                            .query_row(
                                &format!("SELECT count(*) FROM {} WHERE id = ?", table),
                                [sid],
                                |row| row.get::<_, i64>(0),
                            )
                            .unwrap_or(1);
                    }
                    if remaining == 0 {
                        pruned += 1;
                    }
                }
            }
        }

        pruned
    }

    fn store_backed(&self) -> bool {
        true
    }

    fn deletes_descendants(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDir(PathBuf);
    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_test_{}_{}",
                name,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let _ = std::fs::create_dir_all(&path);
            Self(path)
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_opencode_v2_scan_and_messages() {
        let temp_dir = TestDir::new("opencode_v2");
        let db_path = temp_dir.path().join("opencode.db");
        let conn = Connection::open(&db_path).unwrap();

        conn.execute_batch(
            r#"
            CREATE TABLE session_v2 (
                id text PRIMARY KEY,
                project_id text NOT NULL,
                parent_id text,
                directory text NOT NULL,
                title text,
                model text,
                tokens_input integer DEFAULT 0 NOT NULL,
                tokens_output integer DEFAULT 0 NOT NULL,
                tokens_reasoning integer DEFAULT 0 NOT NULL,
                tokens_cache_read integer DEFAULT 0 NOT NULL,
                tokens_cache_write integer DEFAULT 0 NOT NULL,
                time_created integer NOT NULL,
                time_updated integer NOT NULL,
                time_suspended integer
            );

            CREATE TABLE session_message (
                id text PRIMARY KEY,
                session_id text NOT NULL,
                type text NOT NULL,
                seq integer NOT NULL,
                time_created integer NOT NULL,
                time_updated integer NOT NULL,
                data text NOT NULL
            );
            "#,
        ).unwrap();

        let session_id = "ses_test123";
        conn.execute(
            r#"
            INSERT INTO session_v2 (
                id, project_id, parent_id, directory, title, model,
                tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write,
                time_created, time_updated
            ) VALUES (
                ?1, 'proj1', NULL, '/workspace/demo', '测试 OpenCode v2 会话', 'gemini-2.5-pro',
                1000, 200, 50, 400, 100,
                1791943900000, 1791944000000
            )
            "#,
            [session_id],
        ).unwrap();

        // User message
        let user_data = serde_json::json!({
            "time": { "created": 1791943901000i64 },
            "text": "帮我优化这段代码"
        }).to_string();

        conn.execute(
            "INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES (?1, ?2, 'user', 1, 1791943901000, 1791943901000, ?3)",
            ["msg_1", session_id, &user_data],
        ).unwrap();

        // Assistant message with reasoning, text and tool call
        let assistant_data = serde_json::json!({
            "time": { "created": 1791943902000i64, "completed": 1791943903000i64 },
            "content": [
                { "type": "reasoning", "text": "分析用户需求并检查代码" },
                { "type": "text", "text": "好的，我已经完成分析。" },
                {
                    "type": "tool",
                    "name": "read_file",
                    "state": {
                        "input": { "path": "main.rs" },
                        "content": [ { "type": "text", "text": "fn main() {}" } ]
                    }
                }
            ]
        }).to_string();

        conn.execute(
            "INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES (?1, ?2, 'assistant', 2, 1791943902000, 1791943902000, ?3)",
            ["msg_2", session_id, &assistant_data],
        ).unwrap();

        drop(conn);

        let adapter = OpenCodeAdapter::with_dirs("opencode", "OpenCode", vec![temp_dir.path().to_path_buf()]);
        let sessions = adapter.scan_sessions();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.id, session_id);
        assert_eq!(s.title, "测试 OpenCode v2 会话");
        assert_eq!(s.first_prompt, "帮我优化这段代码");
        assert_eq!(s.turn_count, 1);
        assert!(s.token_stats.is_some());
        let t = s.token_stats.as_ref().unwrap();
        assert_eq!(t.prompt_tokens, 1000);
        assert_eq!(t.completion_tokens, 200);
        assert_eq!(t.cache_read_tokens, 400);
        assert_eq!(t.cache_write_tokens, 100);
        assert_eq!(t.reasoning_tokens, 50);
        assert!(!s.is_running);

        let msgs = adapter.load_messages(s, 50);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].text, "帮我优化这段代码");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].text, "好的，我已经完成分析。");
        assert_eq!(msgs[1].thinking.as_deref(), Some("分析用户需求并检查代码"));
        assert_eq!(msgs[1].tool_calls.len(), 1);
        assert_eq!(msgs[1].tool_calls[0].name, "read_file");

        // Test pruning
        let pruned = adapter.prune_indexes(&[session_id.to_string()]);
        assert_eq!(pruned, 1);
        assert_eq!(adapter.scan_sessions().len(), 0);
    }

    #[test]
    fn test_opencode_v1_fallback() {
        let temp_dir = TestDir::new("opencode_v1");
        let db_path = temp_dir.path().join("db.sqlite");
        let conn = Connection::open(&db_path).unwrap();

        conn.execute_batch(
            r#"
            CREATE TABLE session (
                id text PRIMARY KEY,
                title text,
                directory text,
                parent_id text,
                time_created integer,
                time_updated integer
            );

            CREATE TABLE message (
                id text PRIMARY KEY,
                session_id text NOT NULL,
                time_created integer,
                data text NOT NULL
            );
            "#,
        ).unwrap();

        let session_id = "ses_legacy_v1";
        conn.execute(
            "INSERT INTO session (id, title, directory, parent_id, time_created, time_updated) VALUES (?1, 'Legacy v1', '/legacy', NULL, 1000, 2000)",
            [session_id],
        ).unwrap();

        let user_data = serde_json::json!({
            "role": "user",
            "content": "旧版消息"
        }).to_string();

        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES ('m1', ?1, 1500, ?2)",
            [session_id, &user_data],
        ).unwrap();

        drop(conn);

        let adapter = OpenCodeAdapter::with_dirs("opencode", "OpenCode", vec![temp_dir.path().to_path_buf()]);
        let sessions = adapter.scan_sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, session_id);
        assert_eq!(sessions[0].first_prompt, "旧版消息");

        let msgs = adapter.load_messages(&sessions[0], 10);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].text, "旧版消息");

        let pruned = adapter.prune_indexes(&[session_id.to_string()]);
        assert_eq!(pruned, 1);
        assert_eq!(adapter.scan_sessions().len(), 0);
    }

    #[test]
    fn test_opencode_v2_delete_cascades_children_and_flags_running() {
        let temp_dir = TestDir::new("opencode_v2_cascade");
        let db_path = temp_dir.path().join("opencode.db");
        let conn = Connection::open(&db_path).unwrap();

        conn.execute_batch(
            r#"
            CREATE TABLE session_v2 (
                id text PRIMARY KEY,
                project_id text NOT NULL,
                parent_id text,
                directory text NOT NULL,
                title text,
                model text,
                tokens_input integer DEFAULT 0 NOT NULL,
                tokens_output integer DEFAULT 0 NOT NULL,
                tokens_reasoning integer DEFAULT 0 NOT NULL,
                tokens_cache_read integer DEFAULT 0 NOT NULL,
                tokens_cache_write integer DEFAULT 0 NOT NULL,
                time_created integer NOT NULL,
                time_updated integer NOT NULL,
                time_suspended integer
            );

            CREATE TABLE session_message (
                id text PRIMARY KEY,
                session_id text NOT NULL,
                type text NOT NULL,
                seq integer NOT NULL,
                time_created integer NOT NULL,
                time_updated integer NOT NULL,
                data text NOT NULL
            );

            CREATE TABLE session_inbox (
                id text PRIMARY KEY,
                session_id text NOT NULL,
                type text NOT NULL,
                payload text NOT NULL,
                delivery text NOT NULL,
                enqueued_seq integer NOT NULL,
                time_created integer NOT NULL
            );

            CREATE TABLE session_pending (
                id text PRIMARY KEY,
                session_id text NOT NULL,
                type text NOT NULL,
                data text NOT NULL,
                delivery text,
                admitted_seq integer NOT NULL,
                time_created integer NOT NULL
            );

            CREATE TABLE instruction_entry (
                session_id text NOT NULL,
                key text NOT NULL,
                value text,
                removed integer DEFAULT false NOT NULL,
                time_created integer NOT NULL,
                time_updated integer NOT NULL,
                PRIMARY KEY (session_id, key)
            );

            CREATE TABLE instruction_state (
                session_id text PRIMARY KEY,
                epoch_start integer NOT NULL,
                through_seq integer NOT NULL,
                initial_values text NOT NULL,
                current_values text NOT NULL
            );
            "#,
        ).unwrap();

        // parent -> child -> grandchild, plus an unrelated session
        let rows: [(&str, Option<&str>, Option<i64>); 4] = [
            ("ses_parent", None, None),
            ("ses_child", Some("ses_parent"), None),
            ("ses_grandchild", Some("ses_child"), Some(1000)),
            ("ses_other", None, None),
        ];
        for (id, parent, suspended) in rows {
            conn.execute(
                "INSERT INTO session_v2 (id, project_id, parent_id, directory, title, time_created, time_updated, time_suspended) VALUES (?1, 'proj', ?2, '/w', ?1, 1000, 1000, ?3)",
                rusqlite::params![id, parent, suspended],
            ).unwrap();
            conn.execute(
                "INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES (?1 || '_m', ?1, 'user', 1, 1000, 1000, '{}')",
                [id],
            ).unwrap();
            conn.execute(
                "INSERT INTO session_inbox (id, session_id, type, payload, delivery, enqueued_seq, time_created) VALUES (?1 || '_i', ?1, 'prompt', '{}', 'immediate', 1, 1000)",
                [id],
            ).unwrap();
            conn.execute(
                "INSERT INTO session_pending (id, session_id, type, data, admitted_seq, time_created) VALUES (?1 || '_p', ?1, 'compaction', '{}', 1, 1000)",
                [id],
            ).unwrap();
            conn.execute(
                "INSERT INTO instruction_entry (session_id, key, value, removed, time_created, time_updated) VALUES (?1, 'k', 'v', 0, 1000, 1000)",
                [id],
            ).unwrap();
            conn.execute(
                "INSERT INTO instruction_state (session_id, epoch_start, through_seq, initial_values, current_values) VALUES (?1, 1, 1, '{}', '{}')",
                [id],
            ).unwrap();
        }

        drop(conn);

        let adapter = OpenCodeAdapter::with_dirs("opencode", "OpenCode", vec![temp_dir.path().to_path_buf()]);
        let sessions = adapter.scan_sessions();
        assert_eq!(sessions.len(), 4);

        // time_suspended marks the sessions the running process currently owns a drain for
        let running: Vec<String> = sessions
            .iter()
            .filter(|s| s.is_running)
            .map(|s| s.id.clone())
            .collect();
        assert_eq!(running, vec!["ses_grandchild".to_string()]);

        // The adapter deletes exactly what it is asked for, including a running session: skipping
        // running sessions is the cleanup's decision, not the adapter's.
        assert!(adapter.deletes_descendants());

        // Deleting the parent also removes every descendant, and the requested session counts once
        let pruned = adapter.prune_indexes(&["ses_parent".to_string()]);
        assert_eq!(pruned, 1);

        let remaining: Vec<String> = adapter.scan_sessions().into_iter().map(|s| s.id).collect();
        assert_eq!(remaining, vec!["ses_other".to_string()]);

        // Every session-owned table must be cleared for the deleted family and untouched for the
        // unrelated session, so a wrong table name fails here instead of silently leaking rows.
        let conn = Connection::open(&db_path).unwrap();
        for table in [
            "session_message",
            "session_inbox",
            "session_pending",
            "instruction_entry",
            "instruction_state",
        ] {
            let leftover: i64 = conn
                .query_row(
                    &format!(
                        "SELECT count(*) FROM {} WHERE session_id IN ('ses_parent', 'ses_child', 'ses_grandchild')",
                        table
                    ),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(leftover, 0, "{} rows of the deleted family must be gone", table);

            let kept: i64 = conn
                .query_row(
                    &format!("SELECT count(*) FROM {} WHERE session_id = 'ses_other'", table),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(kept, 1, "{} row of the unrelated session must survive", table);
        }
    }
}
