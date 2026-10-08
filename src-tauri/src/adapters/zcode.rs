use crate::adapters::{to_millis, AgentAdapter};
use crate::models::{ChatMessage, SessionSummary, ToolCallItem};
use crate::services::token_calc::make_stats_extended;
use chrono::DateTime;
use rusqlite::{Connection, OpenFlags};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct ZCodeAdapter {
    db_path: PathBuf,
    tasks_db_path: PathBuf,
    exec_dir: PathBuf,
    artifacts_dir: PathBuf,
    image_cache_dir: PathBuf,
    rollout_dir: PathBuf,
}

impl ZCodeAdapter {
    pub fn new() -> Self {
        let home = crate::adapters::user_home().unwrap_or_else(|| PathBuf::from("."));
        let db_path = home.join(".zcode").join("cli").join("db").join("db.sqlite");
        let tasks_db_path = home.join(".zcode").join("v2").join("tasks-index.sqlite");
        let exec_dir = home.join(".zcode").join("cli").join("exec");
        let artifacts_dir = home.join(".zcode").join("cli").join("artifacts");
        let image_cache_dir = home.join(".zcode").join("cli").join("image-cache");
        let rollout_dir = home.join(".zcode").join("cli").join("rollout");
        Self {
            db_path,
            tasks_db_path,
            exec_dir,
            artifacts_dir,
            image_cache_dir,
            rollout_dir,
        }
    }

    fn calculate_path_size(&self, path: &Path) -> u64 {
        if !path.exists() {
            return 0;
        }
        if path.is_file() {
            return fs::metadata(path).map(|m| m.len()).unwrap_or(0);
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
        let mut task_map: HashMap<String, TaskRow> = HashMap::new();

        // 1. Scan ~/.zcode/v2/tasks-index.sqlite (GUI Tasks Index)
        if self.tasks_db_path.exists() {
            if let Ok(conn) = Connection::open_with_flags(
                &self.tasks_db_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
            ) {
                let stmt = conn.prepare(
                    r#"
                    SELECT 
                        task_id, 
                        title, 
                        workspace_path, 
                        created_at, 
                        updated_at, 
                        provider, 
                        model, 
                        mode
                    FROM tasks
                    WHERE deleted = 0
                    ORDER BY updated_at DESC
                    "#,
                );

                if let Ok(mut s) = stmt {
                    let rows = s.query_map([], |row| {
                        Ok(TaskRow {
                            task_id: row.get(0)?,
                            title: row.get(1)?,
                            workspace_path: row.get(2)?,
                            created_at: row.get(3)?,
                            updated_at: row.get(4)?,
                            provider: row.get(5)?,
                            model: row.get(6)?,
                            mode: row.get(7)?,
                        })
                    });

                    if let Ok(iter) = rows {
                        for t in iter.flatten() {
                            if !t.task_id.trim().is_empty() {
                                task_map.insert(t.task_id.clone(), t);
                            }
                        }
                    }
                }
            }
        }

        // 2. Scan ~/.zcode/cli/db/db.sqlite (CLI Database)
        let mut cli_map: HashMap<String, CliSessionRow> = HashMap::new();
        if self.db_path.exists() {
            if let Ok(conn) = Connection::open_with_flags(
                &self.db_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
            ) {
                let stmt = conn.prepare(
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
                );

                if let Ok(mut s) = stmt {
                    let rows = s.query_map([], |row| {
                        Ok(CliSessionRow {
                            id: row.get(0)?,
                            title: row.get(1)?,
                            directory: row.get(2)?,
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

                    if let Ok(iter) = rows {
                        for c in iter.flatten() {
                            if !c.id.trim().is_empty() {
                                cli_map.insert(c.id.clone(), c);
                            }
                        }
                    }
                }
            }
        }

        // 3. Merge both collections by session/task ID
        let mut all_ids: HashSet<String> = HashSet::new();
        all_ids.extend(task_map.keys().cloned());
        all_ids.extend(cli_map.keys().cloned());

        let mut sessions = Vec::new();

        for id in all_ids {
            let task_opt = task_map.get(&id);
            let cli_opt = cli_map.get(&id);

            let (title, cwd, created_at, updated_at, flavor, is_subagent, parent_id, turn_count, token_stats) = match (task_opt, cli_opt) {
                (Some(t), Some(c)) => {
                    let title = if !t.title.trim().is_empty() {
                        t.title.trim().to_string()
                    } else if !c.title.trim().is_empty() {
                        c.title.trim().to_string()
                    } else {
                        format!("ZCode 会话 {}", &id[..8.min(id.len())])
                    };

                    let cwd = if !t.workspace_path.trim().is_empty() {
                        t.workspace_path.trim().to_string()
                    } else {
                        c.directory.trim().to_string()
                    };

                    let created_at = to_millis(if t.created_at > 0 { t.created_at } else { c.time_created });
                    let updated_at = to_millis(if t.updated_at > 0 { t.updated_at } else { c.time_updated });

                    let flavor = if !c.model_id.trim().is_empty() && c.model_id.trim() != "zcode" {
                        c.model_id.trim().to_string()
                    } else if let Some(m) = &t.model {
                        if !m.trim().is_empty() { m.trim().to_string() } else { "zcode".to_string() }
                    } else if let Some(p) = &t.provider {
                        p.trim().to_string()
                    } else {
                        "zcode".to_string()
                    };

                    let is_sub = c.parent_id.as_ref().map_or(false, |p| !p.trim().is_empty());
                    let p_id = if is_sub { c.parent_id.clone() } else { None };

                    let stats = make_stats_extended(
                        &flavor,
                        c.in_tok.max(0) as u64,
                        c.out_tok.max(0) as u64,
                        c.cache_r_tok.max(0) as u64,
                        c.cache_w_tok.max(0) as u64,
                        c.reason_tok.max(0) as u64,
                        if c.dur_ms > 0 { Some(c.dur_ms as u64) } else { None },
                    );

                    (title, cwd, created_at, updated_at, flavor, is_sub, p_id, c.msg_count.max(1) as u32, Some(stats))
                }
                (Some(t), None) => {
                    let title = if !t.title.trim().is_empty() {
                        t.title.trim().to_string()
                    } else {
                        format!("ZCode 会话 {}", &id[..8.min(id.len())])
                    };

                    let cwd = t.workspace_path.trim().to_string();
                    let created_at = to_millis(t.created_at);
                    let updated_at = to_millis(t.updated_at);

                    let flavor = t.model.as_ref()
                        .filter(|m| !m.trim().is_empty())
                        .cloned()
                        .or_else(|| t.provider.as_ref().filter(|p| !p.trim().is_empty()).cloned())
                        .unwrap_or_else(|| "zcode".to_string());

                    (title, cwd, created_at, updated_at, flavor, false, None, 1, None)
                }
                (None, Some(c)) => {
                    let title = if !c.title.trim().is_empty() {
                        c.title.trim().to_string()
                    } else {
                        format!("ZCode 会话 {}", &id[..8.min(id.len())])
                    };

                    let cwd = c.directory.trim().to_string();
                    let created_at = to_millis(c.time_created);
                    let updated_at = to_millis(c.time_updated);

                    let flavor = if !c.model_id.trim().is_empty() {
                        c.model_id.trim().to_string()
                    } else {
                        "zcode".to_string()
                    };

                    let is_sub = c.parent_id.as_ref().map_or(false, |p| !p.trim().is_empty());
                    let p_id = if is_sub { c.parent_id.clone() } else { None };

                    let stats = make_stats_extended(
                        &flavor,
                        c.in_tok.max(0) as u64,
                        c.out_tok.max(0) as u64,
                        c.cache_r_tok.max(0) as u64,
                        c.cache_w_tok.max(0) as u64,
                        c.reason_tok.max(0) as u64,
                        if c.dur_ms > 0 { Some(c.dur_ms as u64) } else { None },
                    );

                    (title, cwd, created_at, updated_at, flavor, is_sub, p_id, c.msg_count.max(1) as u32, Some(stats))
                }
                (None, None) => continue,
            };

            // 4. Gather associated storage locations on disk
            let mut all_paths = Vec::new();
            let mut size_bytes = 0u64;

            let exec_path = self.exec_dir.join(&id);
            if exec_path.exists() {
                size_bytes += self.calculate_path_size(&exec_path);
                all_paths.push(exec_path.to_string_lossy().to_string());
            }

            let bash_startup = self.exec_dir.join("bash-startup").join(&id);
            if bash_startup.exists() {
                size_bytes += self.calculate_path_size(&bash_startup);
                all_paths.push(bash_startup.to_string_lossy().to_string());
            }

            let artifacts_path = self.artifacts_dir.join(&id);
            if artifacts_path.exists() {
                size_bytes += self.calculate_path_size(&artifacts_path);
                all_paths.push(artifacts_path.to_string_lossy().to_string());
            }

            let img_path = self.image_cache_dir.join(&id);
            if img_path.exists() {
                size_bytes += self.calculate_path_size(&img_path);
                all_paths.push(img_path.to_string_lossy().to_string());
            }

            let rollout_path = self.rollout_dir.join(format!("model-io-{}.jsonl", id));
            if rollout_path.exists() {
                size_bytes += self.calculate_path_size(&rollout_path);
                all_paths.push(rollout_path.to_string_lossy().to_string());
            }

            let main_path = if !all_paths.is_empty() {
                all_paths[0].clone()
            } else if self.tasks_db_path.exists() {
                self.tasks_db_path.to_string_lossy().to_string()
            } else {
                self.db_path.to_string_lossy().to_string()
            };

            if size_bytes == 0 {
                size_bytes = 2048; // SQLite record estimate
            }

            let dirname = if !cwd.is_empty() {
                Path::new(&cwd)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                id.clone()
            };

            sessions.push(SessionSummary {
                id: id.clone(),
                platform: "zcode".to_string(),
                flavor,
                dirname,
                main_path,
                all_paths,
                cwd,
                title,
                created_at,
                updated_at,
                size_bytes,
                turn_count,
                is_subagent,
                parent_id,
                is_running: false,
                has_transcript: true,
                token_stats,
            });
        }

        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        sessions
    }

    fn load_messages(&self, session: &SessionSummary, max_msgs: usize) -> Vec<ChatMessage> {
        // Method A: Load from cli/db/db.sqlite if messages exist
        if self.db_path.exists() {
            if let Ok(conn) = Connection::open_with_flags(
                &self.db_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
            ) {
                let msg_sql = r#"
                    SELECT id, data, time_created
                    FROM message
                    WHERE session_id = ?
                    ORDER BY sequence ASC, time_created ASC
                    LIMIT ?
                "#;

                if let Ok(mut stmt) = conn.prepare(msg_sql) {
                    struct MsgRow {
                        id: String,
                        data: String,
                        time_created: i64,
                    }

                    if let Ok(msg_iter) = stmt.query_map([&session.id, &max_msgs.to_string()], |row| {
                        Ok(MsgRow {
                            id: row.get(0)?,
                            data: row.get(1)?,
                            time_created: row.get(2)?,
                        })
                    }) {
                        let msg_rows: Vec<MsgRow> = msg_iter.flatten().collect();

                        if !msg_rows.is_empty() {
                            let mut part_stmt = conn.prepare(
                                r#"
                                SELECT data
                                FROM part
                                WHERE message_id = ?
                                ORDER BY sequence ASC, time_created ASC
                                "#,
                            ).ok();

                            let mut results = Vec::new();

                            for msg in msg_rows {
                                let parsed_meta: serde_json::Value = serde_json::from_str(&msg.data).unwrap_or(serde_json::Value::Null);
                                let role = parsed_meta.get("role").and_then(|r| r.as_str()).unwrap_or("user").to_string();

                                let time_formatted = DateTime::from_timestamp_millis(to_millis(msg.time_created))
                                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                                    .unwrap_or_else(|| "".to_string());

                                let mut text_parts = Vec::new();
                                let mut thinking: Option<String> = None;
                                let mut tool_calls = Vec::new();

                                if let Some(ref mut pstmt) = part_stmt {
                                    if let Ok(part_iter) = pstmt.query_map([&msg.id], |r| r.get::<_, String>(0)) {
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

                            if !results.is_empty() {
                                return results;
                            }
                        }
                    }
                }
            }
        }

        // Method B: Fallback to rollout log model-io-<id>.jsonl
        let rollout_file = self.rollout_dir.join(format!("model-io-{}.jsonl", session.id));
        if rollout_file.is_file() {
            if let Ok(file) = File::open(&rollout_file) {
                let reader = BufReader::new(file);
                let mut results = Vec::new();

                for line in reader.lines().flatten() {
                    let line_str = line.trim();
                    if line_str.is_empty() {
                        continue;
                    }

                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(line_str) {
                        let time_str = v.get("startedAt").and_then(|t| t.as_str()).unwrap_or("");
                        let formatted_time = if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(time_str) {
                            dt.format("%Y-%m-%d %H:%M:%S").to_string()
                        } else {
                            "".to_string()
                        };

                        // Extract user message from payload
                        if let Some(msgs) = v.get("payload").and_then(|p| p.get("messages")).and_then(|m| m.as_array()) {
                            for m in msgs {
                                let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                                let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
                                if !content.trim().is_empty() && !content.starts_with("<system-reminder>") {
                                    results.push(ChatMessage {
                                        role: role.to_string(),
                                        text: content.to_string(),
                                        time: formatted_time.clone(),
                                        msg_type: "text".to_string(),
                                        thinking: None,
                                        tool_calls: Vec::new(),
                                    });
                                }
                            }
                        }

                        // Extract assistant response
                        if let Some(resp) = v.get("response") {
                            if let Some(text) = resp.get("text").and_then(|t| t.as_str()) {
                                if !text.trim().is_empty() {
                                    results.push(ChatMessage {
                                        role: "assistant".to_string(),
                                        text: text.to_string(),
                                        time: formatted_time,
                                        msg_type: "text".to_string(),
                                        thinking: None,
                                        tool_calls: Vec::new(),
                                    });
                                }
                            }
                        }
                    }

                    if results.len() >= max_msgs {
                        break;
                    }
                }

                if !results.is_empty() {
                    return results;
                }
            }
        }

        Vec::new()
    }

    fn prune_indexes(&self, session_ids: &[String]) -> usize {
        if session_ids.is_empty() {
            return 0;
        }

        let mut deleted_count = 0;

        // 1. Delete from GUI Tasks Index (~/.zcode/v2/tasks-index.sqlite)
        if self.tasks_db_path.exists() {
            if let Ok(conn) = Connection::open(&self.tasks_db_path) {
                for id in session_ids {
                    let _ = conn.execute("DELETE FROM task_group_members WHERE task_id = ?", [id]);
                    let _ = conn.execute(
                        "DELETE FROM task_group_view_node_orders WHERE node_type = 'task' AND (node_key = ? OR node_key LIKE ('%' || ? || '%'))",
                        [id, id],
                    );
                    if let Ok(c) = conn.execute("DELETE FROM tasks WHERE task_id = ?", [id]) {
                        deleted_count += c;
                    }
                }
                // Force SQLite WAL checkpoint to flush all changes to main db and prevent ghost state
                let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
            }
        }

        // 2. Delete from CLI Database (~/.zcode/cli/db/db.sqlite)
        if self.db_path.exists() {
            if let Ok(conn) = Connection::open(&self.db_path) {
                for id in session_ids {
                    let _ = conn.execute("DELETE FROM part WHERE message_id IN (SELECT id FROM message WHERE session_id = ?)", [id]);
                    let _ = conn.execute("DELETE FROM message WHERE session_id = ?", [id]);
                    let _ = conn.execute("DELETE FROM model_usage WHERE session_id = ?", [id]);
                    let _ = conn.execute("DELETE FROM session_entry WHERE session_id = ?", [id]);
                    let _ = conn.execute("DELETE FROM session_target WHERE session_id = ?", [id]);
                    let _ = conn.execute("DELETE FROM session_task_link WHERE session_id = ?", [id]);
                    let _ = conn.execute("DELETE FROM todo WHERE session_id = ?", [id]);
                    if let Ok(c) = conn.execute("DELETE FROM session WHERE id = ?", [id]) {
                        if deleted_count == 0 {
                            deleted_count += c;
                        }
                    }
                }
                let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
            }
        }

        // 3. Clean up associated files/directories from disk
        for id in session_ids {
            let exec_dir = self.exec_dir.join(id);
            if exec_dir.exists() {
                let _ = fs::remove_dir_all(exec_dir);
            }

            let bash_startup = self.exec_dir.join("bash-startup").join(id);
            if bash_startup.exists() {
                let _ = fs::remove_dir_all(bash_startup);
            }

            let artifacts_dir = self.artifacts_dir.join(id);
            if artifacts_dir.exists() {
                let _ = fs::remove_dir_all(artifacts_dir);
            }

            let img_dir = self.image_cache_dir.join(id);
            if img_dir.exists() {
                let _ = fs::remove_dir_all(img_dir);
            }

            let rollout_file = self.rollout_dir.join(format!("model-io-{}.jsonl", id));
            if rollout_file.exists() {
                let _ = fs::remove_file(rollout_file);
            }
        }

        deleted_count
    }
}

struct TaskRow {
    task_id: String,
    title: String,
    workspace_path: String,
    created_at: i64,
    updated_at: i64,
    provider: Option<String>,
    model: Option<String>,
    #[allow(dead_code)]
    mode: Option<String>,
}

struct CliSessionRow {
    id: String,
    title: String,
    directory: String,
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
