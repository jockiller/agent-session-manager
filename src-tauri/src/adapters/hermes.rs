use std::path::PathBuf;

use rusqlite::{Connection, OpenFlags};

use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary};

pub struct HermesAdapter;

impl HermesAdapter {
    fn db_path() -> PathBuf {
        if let Some(home) = crate::adapters::user_home() {
            home.join(".hermes").join("state.db")
        } else {
            PathBuf::from(".hermes").join("state.db")
        }
    }
}

impl AgentAdapter for HermesAdapter {
    fn platform_id(&self) -> &'static str {
        "hermes"
    }

    fn display_name(&self) -> &'static str {
        "Hermes"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let db_path = Self::db_path();
        if !db_path.exists() {
            return Vec::new();
        }

        let conn = match Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        ) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut stmt = match conn.prepare(
            r#"
            SELECT 
                id, 
                COALESCE(title, ''), 
                COALESCE(cwd, ''), 
                COALESCE(started_at, 0)
            FROM sessions
            ORDER BY started_at DESC
            "#,
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let mut summaries = Vec::new();
        let mut first_prompts: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        if let Some(mut fp_stmt) = conn
            .prepare(
                r#"
                SELECT session_id, content
                FROM messages
                WHERE role = 'user'
                ORDER BY session_id, timestamp
                "#,
            )
            .ok()
        {
            if let Ok(rows) = fp_stmt.query_map([], |row| {
                let sid: String = row.get(0)?;
                let content: String = row.get(1)?;
                Ok((sid, content))
            }) {
                for (sid, content) in rows.flatten() {
                    if first_prompts.contains_key(&sid) {
                        continue;
                    }
                    if let Some(fp) = crate::adapters::make_first_prompt(&content) {
                        first_prompts.insert(sid, fp);
                    }
                }
            }
        }
        let rows = match stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let cwd: String = row.get(2)?;
            let started_at: i64 = row.get(3)?;
            Ok((id, title, cwd, started_at))
        }) {
            Ok(r) => r,
            Err(_) => return summaries,
        };

        for r in rows.flatten() {
            let (id, mut title, cwd, started_at) = r;
            let created_at = crate::adapters::to_millis(started_at);

            if title.is_empty() {
                title = format!("Hermes 会话 {}", &id[..id.len().min(8)]);
            }

            let main_path_str = db_path.to_string_lossy().to_string();

            summaries.push(SessionSummary {
                first_prompt: first_prompts.get(&id).cloned().unwrap_or_default(),
                id: id.clone(),
                platform: "hermes".to_string(),
                flavor: "hermes".to_string(),
                dirname: cwd.clone(),
                main_path: main_path_str,
                all_paths: vec![],
                cwd,
                title,
                created_at,
                updated_at: created_at,
                size_bytes: 4096,
                turn_count: 1,
                is_subagent: false,
                parent_id: None,
                is_running: false,
                is_archived: false,
                has_transcript: true,
            });
        }

        summaries
    }

    fn load_messages(&self, session: &SessionSummary, max_msgs: usize) -> Vec<ChatMessage> {
        let db_path = Self::db_path();
        if !db_path.exists() {
            return Vec::new();
        }

        let conn = match Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        ) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut msgs = Vec::new();
        let mut stmt = match conn.prepare(
            r#"
            SELECT role, content, timestamp
            FROM messages
            WHERE session_id = ?
            ORDER BY timestamp ASC
            LIMIT ?
            "#,
        ) {
            Ok(s) => s,
            Err(_) => return msgs,
        };

        let rows = match stmt.query_map([&session.id, &max_msgs.to_string()], |row| {
            let role: String = row.get(0)?;
            let content: String = row.get(1)?;
            let timestamp: Option<String> = row.get(2).ok();
            Ok((role, content, timestamp))
        }) {
            Ok(r) => r,
            Err(_) => return msgs,
        };

        for r in rows.flatten() {
            let (role, content, timestamp) = r;
            if !content.trim().is_empty() {
                msgs.push(ChatMessage {
                    role,
                    text: content,
                    time: timestamp.unwrap_or_default(),
                    msg_type: "text".to_string(),
                    thinking: None,
                    tool_calls: Vec::new(),
                });
            }
        }

        msgs
    }

    fn prune_indexes(&self, session_ids: &[String]) -> usize {
        if session_ids.is_empty() {
            return 0;
        }
        let mut pruned = 0;
        let db_path = Self::db_path();
        if db_path.exists() {
            if let Ok(conn) = Connection::open(&db_path) {
                for id in session_ids {
                    if let Ok(c) = conn.execute("DELETE FROM sessions WHERE id = ?", [id]) {
                        pruned += c;
                    }
                }
                let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
            }
        }
        pruned
    }
}
