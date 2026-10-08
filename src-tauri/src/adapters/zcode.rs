use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, SessionSummary, ToolCallItem};
use crate::services::token_calc::make_stats_extended;
use chrono::DateTime;
use rusqlite::{Connection, OpenFlags};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct ZCodeAdapter {
    db_path: PathBuf,
    exec_dir: PathBuf,
}

impl ZCodeAdapter {
    pub fn new() -> Self {
        let home = crate::adapters::user_home().unwrap_or_else(|| PathBuf::from("."));
        let db_path = home.join(".zcode").join("cli").join("db").join("db.sqlite");
        let exec_dir = home.join(".zcode").join("cli").join("exec");
        Self { db_path, exec_dir }
    }

    fn calculate_dir_size(&self, path: &Path) -> u64 {
        if !path.exists() {
            return 1024;
        }
        if path.is_file() {
            return fs::metadata(path).map(|m| m.len()).unwrap_or(1024);
        }
        WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum()
    }
}

impl AgentAdapter for ZCodeAdapter {
    fn platform_id(&self) -> &'static str {
        "zcode"
    }

    fn display_name(&self) -> &'static str {
        "ZCode"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        if !self.db_path.exists() {
            return Vec::new();
        }

        let conn = match Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        ) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[ZCode] Failed to open database: {}", e);
                return Vec::new();
            }
        };

        let mut stmt = match conn.prepare(
            r#"
            SELECT 
                s.id, 
                s.title, 
                s.directory, 
                s.parent_id, 
                s.time_created, 
                s.time_updated,
                COALESCE(SUM(m.input_tokens), 0) as in_tok,
                COALESCE(SUM(m.output_tokens), 0) as out_tok,
                COALESCE(SUM(m.reasoning_tokens), 0) as reason_tok,
                COALESCE(SUM(m.cache_creation_input_tokens), 0) as cache_w_tok,
                COALESCE(SUM(m.cache_read_input_tokens), 0) as cache_r_tok,
                COALESCE(SUM(m.duration_ms), 0) as dur_ms,
                COALESCE(
                    NULLIF(MAX(m.model_id), ''), 
                    (SELECT json_extract(msg.data, '$.modelID') FROM message msg WHERE msg.session_id = s.id AND json_extract(msg.data, '$.modelID') IS NOT NULL ORDER BY msg.time_created DESC LIMIT 1),
                    'zcode'
                ) as model_id,
                (SELECT COUNT(*) FROM message msg WHERE msg.session_id = s.id) as msg_count
            FROM session s
            LEFT JOIN model_usage m ON s.id = m.session_id
            GROUP BY s.id
            ORDER BY s.time_updated DESC
            "#,
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[ZCode] Failed to prepare statement: {}", e);
                return Vec::new();
            }
        };

        struct RawRow {
            id: String,
            title: String,
            cwd: String,
            parent_id: Option<String>,
            time_created: i64,
            time_updated: i64,
            in_tok: i64,
            out_tok: i64,
            reason_tok: i64,
            cache_w_tok: i64,
            cache_r_tok: i64,
            dur_ms: i64,
            model_id: String,
            msg_count: i64,
        }

        let rows = stmt.query_map([], |row| {
            Ok(RawRow {
                id: row.get(0)?,
                title: row.get(1)?,
                cwd: row.get(2)?,
                parent_id: row.get(3)?,
                time_created: row.get(4)?,
                time_updated: row.get(5)?,
                in_tok: row.get(6)?,
                out_tok: row.get(7)?,
                reason_tok: row.get(8)?,
                cache_w_tok: row.get(9)?,
                cache_r_tok: row.get(10)?,
                dur_ms: row.get(11)?,
                model_id: row.get(12)?,
                msg_count: row.get(13)?,
            })
        });

        let mut sessions = Vec::new();

        if let Ok(iter) = rows {
            for item in iter.flatten() {
                let is_subagent = item.parent_id.as_ref().map_or(false, |p| !p.trim().is_empty());
                let parent_id = if is_subagent { item.parent_id.clone() } else { None };

                let exec_path = self.exec_dir.join(&item.id);
                let (main_path, all_paths, size_bytes) = if exec_path.exists() {
                    let sz = self.calculate_dir_size(&exec_path);
                    (exec_path.to_string_lossy().to_string(), vec![exec_path.to_string_lossy().to_string()], sz)
                } else {
                    (self.db_path.to_string_lossy().to_string(), vec![], 2048)
                };

                let token_stats = make_stats_extended(
                    &item.model_id,
                    item.in_tok.max(0) as u64,
                    item.out_tok.max(0) as u64,
                    item.cache_r_tok.max(0) as u64,
                    item.cache_w_tok.max(0) as u64,
                    item.reason_tok.max(0) as u64,
                    if item.dur_ms > 0 { Some(item.dur_ms as u64) } else { None },
                );

                let title = if item.title.trim().is_empty() {
                    format!("ZCode Session {}", &item.id[..8.min(item.id.len())])
                } else {
                    item.title.trim().to_string()
                };

                let flavor = if !item.model_id.trim().is_empty() {
                    item.model_id.trim().to_string()
                } else {
                    "zcode".to_string()
                };

                sessions.push(SessionSummary {
                    id: item.id.clone(),
                    platform: "zcode".to_string(),
                    flavor,
                    dirname: item.id.clone(),
                    main_path,
                    all_paths,
                    cwd: item.cwd,
                    title,
                    created_at: item.time_created / 1000,
                    updated_at: item.time_updated / 1000,
                    size_bytes,
                    turn_count: item.msg_count.max(0) as u32,
                    is_subagent,
                    parent_id,
                    is_running: false,
                    has_transcript: true,
                    token_stats: Some(token_stats),
                });
            }
        }

        sessions
    }

    fn load_messages(&self, session: &SessionSummary, max_msgs: usize) -> Vec<ChatMessage> {
        if !self.db_path.exists() {
            return Vec::new();
        }

        let conn = match Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        ) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut stmt = match conn.prepare(
            r#"
            SELECT id, data, time_created
            FROM message
            WHERE session_id = ?
            ORDER BY sequence ASC, time_created ASC
            LIMIT ?
            "#,
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        struct MsgRow {
            id: String,
            data: String,
            time_created: i64,
        }

        let msg_rows: Vec<MsgRow> = match stmt.query_map([&session.id, &max_msgs.to_string()], |row| {
            Ok(MsgRow {
                id: row.get(0)?,
                data: row.get(1)?,
                time_created: row.get(2)?,
            })
        }) {
            Ok(iter) => iter.flatten().collect(),
            Err(_) => return Vec::new(),
        };

        let mut part_stmt = match conn.prepare(
            r#"
            SELECT data
            FROM part
            WHERE message_id = ?
            ORDER BY sequence ASC, time_created ASC
            "#,
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let mut results = Vec::new();

        for msg in msg_rows {
            let parsed_meta: serde_json::Value = serde_json::from_str(&msg.data).unwrap_or(serde_json::Value::Null);
            let role = parsed_meta.get("role").and_then(|r| r.as_str()).unwrap_or("user").to_string();

            let time_formatted = DateTime::from_timestamp_millis(msg.time_created)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "".to_string());

            let mut text_parts = Vec::new();
            let mut thinking: Option<String> = None;
            let mut tool_calls = Vec::new();

            if let Ok(part_iter) = part_stmt.query_map([&msg.id], |r| r.get::<_, String>(0)) {
                for part_str in part_iter.flatten() {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&part_str) {
                        let part_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        match part_type {
                            "text" => {
                                if let Some(txt) = val.get("text").and_then(|t| t.as_str()) {
                                    if !txt.is_empty() {
                                        text_parts.push(txt.to_string());
                                    }
                                }
                            }
                            "reasoning" => {
                                if let Some(reason) = val.get("text").and_then(|t| t.as_str()) {
                                    thinking = Some(reason.to_string());
                                }
                            }
                            "tool" => {
                                let name = val.get("tool").and_then(|t| t.as_str()).unwrap_or("tool").to_string();
                                let input_str = val.get("state").and_then(|s| s.get("input")).map(|i| i.to_string());
                                let output_str = val.get("state").and_then(|s| s.get("output")).map(|o| o.to_string());
                                tool_calls.push(ToolCallItem {
                                    name,
                                    args: input_str,
                                    output: output_str,
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }

            let full_text = text_parts.join("\n\n");
            if full_text.is_empty() && thinking.is_none() && tool_calls.is_empty() {
                continue;
            }

            results.push(ChatMessage {
                role,
                text: full_text,
                time: time_formatted,
                msg_type: "text".to_string(),
                thinking,
                tool_calls,
            });
        }

        results
    }

    fn prune_indexes(&self, session_ids: &[String]) -> usize {
        if !self.db_path.exists() || session_ids.is_empty() {
            return 0;
        }

        let conn = match Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(_) => return 0,
        };

        let mut deleted_count = 0;
        for id in session_ids {
            let res = conn.execute("DELETE FROM session WHERE id = ?", [id]);
            if res.is_ok() {
                deleted_count += 1;
            }

            let exec_dir = self.exec_dir.join(id);
            if exec_dir.exists() {
                let _ = fs::remove_dir_all(exec_dir);
            }
        }

        deleted_count
    }
}
