use std::path::PathBuf;

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::adapters::{to_millis, AgentAdapter};
use crate::models::{ChatMessage, SessionSummary};

pub struct OpenCodeAdapter {
    platform: &'static str,
    name: &'static str,
    dirs: Vec<PathBuf>,
}

impl OpenCodeAdapter {
    pub fn new_opencode() -> Self {
        let mut dirs = Vec::new();
        if let Some(home) = crate::adapters::user_home() {
            dirs.push(home.join(".local").join("share").join("opencode"));
            dirs.push(home.join(".opencode"));
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
        if let Some(home) = crate::adapters::user_home() {
            dirs.push(home.join(".local").join("share").join("codewiz"));
            dirs.push(home.join(".codewiz"));
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

    fn find_db(&self) -> Option<PathBuf> {
        for dir in &self.dirs {
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
            let main_path_str = db_path.to_string_lossy().to_string();

            summaries.push(SessionSummary {
                id,
                platform: self.platform.to_string(),
                flavor: self.platform.to_string(),
                dirname: directory.clone(),
                main_path: main_path_str,
                all_paths: vec![],
                cwd: directory,
                title,
                created_at,
                updated_at,
                size_bytes: 4096,
                turn_count: 1,
                is_subagent,
                parent_id,
                is_running: false,
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
        // Try fetching from message table
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
            if let Ok(conn) = Connection::open(&db_path) {
                for id in session_ids {
                    let _ = conn.execute("DELETE FROM part WHERE message_id IN (SELECT id FROM message WHERE session_id = ?)", [id]);
                    let _ = conn.execute("DELETE FROM message WHERE session_id = ?", [id]);
                    if let Ok(c) = conn.execute("DELETE FROM session WHERE id = ?", [id]) {
                        pruned += c;
                    }
                }
                let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
            }
        }

        pruned
    }
}
