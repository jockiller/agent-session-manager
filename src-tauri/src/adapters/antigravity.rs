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

pub struct AntigravityAdapter {
    /// Test-only home directory. Production leaves this empty and uses `GEMINI_HOME` or `~/.gemini`.
    home_override: Option<PathBuf>,
}

/// Antigravity ships as three products that share one on-disk layout: the CLI, the
/// desktop app and the IDE. Each keeps its own sub directory of `~/.gemini`.
const FLAVORS: [(&str, &str); 3] = [
    ("cli", "antigravity-cli"),
    ("app", "antigravity"),
    ("ide", "antigravity-ide"),
];

/// File names of the protobuf summary store, one per product family.
const SUMMARY_STORES: [&str; 2] = ["jetbox_summaries_proto.pb", "agyhub_summaries_proto.pb"];

impl AntigravityAdapter {
    pub fn new() -> Self {
        Self { home_override: None }
    }

    #[cfg(test)]
    fn with_home(home: PathBuf) -> Self {
        Self {
            home_override: Some(home),
        }
    }

    fn gemini_home(&self) -> PathBuf {
        if let Some(home) = &self.home_override {
            return home.clone();
        }
        std::env::var_os("GEMINI_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| crate::adapters::user_home().map(|home| home.join(".gemini")))
            .unwrap_or_else(|| PathBuf::from(".gemini"))
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

    fn summary_store(fdir: &Path) -> Option<PathBuf> {
        SUMMARY_STORES
            .iter()
            .map(|name| fdir.join(name))
            .find(|p| p.metadata().map_or(false, |m| m.len() > 0))
    }

    /// Removes `remove` from the protobuf summary store, which is the authoritative copy:
    /// Antigravity rebuilds `conversation_summaries.db` from it on every start, so a row
    /// deleted only from SQLite comes back on the next launch.
    fn prune_summary_store(fdir: &Path, remove: &HashSet<&str>) {
        let Some(store) = Self::summary_store(fdir) else {
            return;
        };
        let Ok(original) = fs::read(&store) else {
            return;
        };
        let Some((updated, dropped)) = pb_drop_records(&original, remove) else {
            // Unrecognised layout: leave the file alone rather than risk corrupting it.
            return;
        };
        if dropped == 0 {
            return;
        }

        // Keep one previous copy, then swap the new contents in atomically. Skip the rewrite
        // entirely when the backup cannot be written, so a failed cleanup never loses the store.
        if fs::copy(&store, store.with_extension("pb.pre-prune")).is_err() {
            return;
        }
        let tmp = store.with_extension("pb.tmp");
        let mut written = fs::write(&tmp, &updated).is_ok();
        if written {
            // The store is mode 0600; a fresh file would widen it to the default.
            if let Ok(meta) = fs::metadata(&store) {
                written = fs::set_permissions(&tmp, meta.permissions()).is_ok();
            }
        }
        if !written || fs::rename(&tmp, &store).is_err() {
            let _ = fs::remove_file(&tmp);
        }
    }

    fn prune_summary_db(fdir: &Path, remove: &HashSet<&str>) {
        let db_path = fdir.join("conversation_summaries.db");
        if !db_path.exists() {
            return;
        }
        let Ok(conn) = Connection::open(&db_path) else {
            return;
        };
        // Antigravity may be holding the database; wait instead of silently giving up.
        let _ = conn.busy_timeout(std::time::Duration::from_secs(5));

        let ids: Vec<&str> = remove.iter().copied().collect();
        delete_conversation_ids(&conn, "conversation_summaries", &ids);
        // The desktop app keeps an FTS5 search index with no delete trigger, so removing only the
        // summary row would leave the conversation findable.
        if sqlite_name_exists(&conn, "conversations_fts") {
            delete_conversation_ids(&conn, "conversations_fts", &ids);
        }
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }

    /// Ids still listed by the protobuf store or by its SQLite mirror, plus whether every
    /// existing store could be read. An unreadable store means absence cannot be proven, so
    /// nothing may be reported as removed in that case.
    fn summary_ids(fdir: &Path) -> (HashSet<String>, bool) {
        let mut ids = HashSet::new();
        let mut verifiable = true;

        if let Some(store) = Self::summary_store(fdir) {
            match fs::read(&store) {
                Ok(buf) => match pb_fields(&buf, 0, buf.len()) {
                    Some(fields) => ids.extend(fields.iter().filter_map(|f| pb_record_id(&buf, f))),
                    None => verifiable = false,
                },
                Err(_) => verifiable = false,
            }
        }

        let db_path = fdir.join("conversation_summaries.db");
        if db_path.exists() {
            match Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
                Ok(conn) => {
                    if !collect_conversation_ids(&conn, "conversation_summaries", &mut ids) {
                        verifiable = false;
                    }
                    if sqlite_name_exists(&conn, "conversations_fts")
                        && !collect_conversation_ids(&conn, "conversations_fts", &mut ids)
                    {
                        verifiable = false;
                    }
                }
                Err(_) => verifiable = false,
            }
        }

        (ids, verifiable)
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

fn sqlite_name_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE name = ?1",
        [name],
        |_| Ok(1i32),
    )
    .is_ok()
}

fn delete_conversation_ids(conn: &Connection, table: &str, ids: &[&str]) {
    for chunk in ids.chunks(400) {
        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "DELETE FROM {} WHERE conversation_id IN ({})",
            table, placeholders
        );
        let _ = conn.execute(&sql, rusqlite::params_from_iter(chunk.iter().copied()));
    }
}

fn collect_conversation_ids(conn: &Connection, table: &str, into: &mut HashSet<String>) -> bool {
    let sql = format!("SELECT conversation_id FROM {}", table);
    let mut stmt = match conn.prepare(&sql) {
        Ok(stmt) => stmt,
        Err(_) => return false,
    };
    let rows = match stmt.query_map([], |row| row.get::<_, String>(0)) {
        Ok(rows) => rows,
        Err(_) => return false,
    };
    for row in rows {
        match row {
            Ok(id) => {
                into.insert(id);
            }
            Err(_) => return false,
        }
    }
    true
}

/// One field of a protobuf message, with the byte span it occupies.
struct PbField {
    number: u64,
    wire: u8,
    /// Offset of the field tag, so the whole field can be copied verbatim.
    start: usize,
    /// Offset of the field value, after the tag and (for wire type 2) its length prefix.
    value_start: usize,
    /// End of the field.
    value_end: usize,
}

/// Minimal protobuf wire-format reader, just enough to walk the summary store.
/// Returns `None` for anything it does not understand, so callers can leave the file alone.
fn pb_fields(buf: &[u8], mut pos: usize, end: usize) -> Option<Vec<PbField>> {
    let mut out = Vec::new();
    while pos < end {
        let start = pos;
        let (key, after_key) = pb_varint(buf, pos, end)?;
        pos = after_key;
        let number = key >> 3;
        let wire = (key & 0x7) as u8;
        if number == 0 {
            return None;
        }
        let (value_start, value_end) = match wire {
            0 => {
                let (_value, after) = pb_varint(buf, pos, end)?;
                (pos, after)
            }
            2 => {
                let (len, after) = pb_varint(buf, pos, end)?;
                let value_end = after.checked_add(len as usize)?;
                if value_end > end {
                    return None;
                }
                (after, value_end)
            }
            1 => (pos, pos.checked_add(8)?),
            5 => (pos, pos.checked_add(4)?),
            _ => return None,
        };
        if value_end > end {
            return None;
        }
        out.push(PbField {
            number,
            wire,
            start,
            value_start,
            value_end,
        });
        pos = value_end;
    }
    Some(out)
}

fn pb_varint(buf: &[u8], mut pos: usize, end: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        if pos >= end || shift > 63 {
            return None;
        }
        let b = buf[pos];
        value |= ((b & 0x7F) as u64) << shift;
        pos += 1;
        if b & 0x80 == 0 {
            return Some((value, pos));
        }
        shift += 7;
    }
}

/// Conversation id of a summary record: field 1 of the record message.
fn pb_record_id(buf: &[u8], record: &PbField) -> Option<String> {
    let inner = pb_fields(buf, record.value_start, record.value_end)?;
    let id = inner.iter().find(|f| f.number == 1 && f.wire == 2)?;
    Some(String::from_utf8_lossy(&buf[id.value_start..id.value_end]).to_string())
}

/// Drops the summary records of `remove` from the store, copying every other record
/// byte-for-byte. Returns the new contents and how many records were dropped.
fn pb_drop_records(buf: &[u8], remove: &HashSet<&str>) -> Option<(Vec<u8>, usize)> {
    let fields = pb_fields(buf, 0, buf.len())?;
    let mut out = Vec::with_capacity(buf.len());
    let mut dropped = 0;

    for field in &fields {
        let drop = field.number == 1
            && field.wire == 2
            && pb_record_id(buf, field).map_or(false, |id| remove.contains(id.as_str()));
        if drop {
            dropped += 1;
            continue;
        }
        out.extend_from_slice(&buf[field.start..field.value_end]);
    }

    // The parse consumed the buffer exactly, so the rewrite is a pure subtraction.
    Some((out, dropped))
}

struct SummaryDbRow {
    title: String,
    preview: String,
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
        let base = self.gemini_home();
        if !base.exists() {
            return Vec::new();
        }

        let projects = Self::load_projects(&base);
        let mut summaries = Vec::new();
        for (flavor, dir_name) in FLAVORS {
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
                                    _ => t2.clone().unwrap_or_default(),
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
                                    preview: Self::clean_title(&t2.unwrap_or_default()),
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
                let mut first_prompt = String::new();

                if let Some(row) = db_meta.get(&sid) {
                    title = row.title.clone();
                    cwd = row.cwd.clone();
                    updated_at = row.updated_at;
                    turn_count = row.steps;
                    parent_id = row.parent_id.clone();
                    is_subagent = row.is_sub;
                    status_running = row.status == "CASCADE_RUN_STATUS_RUNNING";
                    // DB-only sessions have no transcript; preview/title is the first user prompt
                    if first_prompt.is_empty() {
                        let raw_prompt = if title.is_empty() { row.preview.clone() } else { row.title.clone() };
                        if let Some(fp) = crate::adapters::make_first_prompt(&raw_prompt) {
                            first_prompt = fp;
                        }
                    }
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
                                    if first_prompt.is_empty() {
                                        if let Some(content) = v.get("content").and_then(Value::as_str) {
                                            // AGY_REQ_RE already unwraps <USER_REQUEST>; clean_title also works for the snippet
                                            if let Some(fp) = crate::adapters::make_first_prompt(content) {
                                                first_prompt = fp;
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
                    first_prompt,
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
        if session_ids.is_empty() {
            return 0;
        }

        let base = self.gemini_home();
        let remove: HashSet<&str> = session_ids.iter().map(|s| s.as_str()).collect();

        for (_, dir_name) in FLAVORS {
            let fdir = base.join(dir_name);
            if !fdir.exists() {
                continue;
            }
            // The protobuf store is the source of truth; the SQLite file is only its mirror and is
            // rebuilt from the store on every start, so it has to be cleared after the store.
            Self::prune_summary_store(&fdir, &remove);
            Self::prune_summary_db(&fdir, &remove);
        }

        // Report a session as removed only when no store still lists it. This makes a write that
        // Antigravity rejected (say, a locked database, or a store this code cannot parse) show up
        // as a failed deletion instead of a silent no-op.
        let mut still_listed = HashSet::new();
        let mut verifiable = true;
        for (_, dir_name) in FLAVORS {
            let (ids, ok) = Self::summary_ids(&base.join(dir_name));
            still_listed.extend(ids);
            verifiable &= ok;
        }
        if !verifiable {
            return 0;
        }
        session_ids
            .iter()
            .filter(|id| !still_listed.contains(id.as_str()))
            .count()
    }

    fn store_backed(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempHome(PathBuf);

    impl TempHome {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_agy_{}_{}",
                name,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(path.join("antigravity-cli")).unwrap();
            Self(path)
        }

        fn cli(&self) -> PathBuf {
            self.0.join("antigravity-cli")
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Builds a store with the same shape Antigravity writes: repeated field 1 holding a
    /// record message whose field 1 is the conversation id and field 2 is the summary blob.
    fn store_bytes(records: &[(&str, &[u8])]) -> Vec<u8> {
        fn varint(mut v: u64, out: &mut Vec<u8>) {
            loop {
                let b = (v & 0x7F) as u8;
                v >>= 7;
                if v == 0 {
                    out.push(b);
                    return;
                }
                out.push(b | 0x80);
            }
        }
        fn tags(field: u64, len: usize, out: &mut Vec<u8>) {
            varint((field << 3) | 2, out);
            varint(len as u64, out);
        }

        let mut out = Vec::new();
        for (id, payload) in records {
            let mut record = Vec::new();
            tags(1, id.len(), &mut record);
            record.extend_from_slice(id.as_bytes());
            tags(2, payload.len(), &mut record);
            record.extend_from_slice(payload);
            tags(1, record.len(), &mut out);
            out.extend_from_slice(&record);
        }
        out
    }

    fn write_db(path: &Path, ids: &[&str]) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE conversation_summaries (
                 conversation_id text PRIMARY KEY,
                 title text NOT NULL DEFAULT '',
                 preview text NOT NULL DEFAULT '',
                 step_count integer NOT NULL DEFAULT 0,
                 last_modified_time datetime NOT NULL,
                 workspace_uris text NOT NULL,
                 status text NOT NULL DEFAULT '',
                 source text NOT NULL DEFAULT '',
                 project_id text NOT NULL DEFAULT '',
                 agent_name text NOT NULL DEFAULT '',
                 parent_conversation_id text NOT NULL DEFAULT '',
                 nesting_depth integer NOT NULL DEFAULT 0,
                 battle_id text NOT NULL DEFAULT '',
                 winning_conversation_id text NOT NULL DEFAULT '',
                 not_fully_idle numeric NOT NULL DEFAULT false,
                 killed numeric NOT NULL DEFAULT false,
                 last_user_input_time datetime NOT NULL,
                 last_user_input_step_index integer NOT NULL DEFAULT -1,
                 app_data_dir text NOT NULL DEFAULT '',
                 raw_summary BLOB,
                 group_id text NOT NULL DEFAULT ''
             );",
        )
        .unwrap();
        for id in ids {
            conn.execute(
                "INSERT INTO conversation_summaries (conversation_id, last_modified_time, workspace_uris, last_user_input_time) VALUES (?1, '2026-01-01 00:00:00+00:00', '[]', '2026-01-01 00:00:00+00:00')",
                [id],
            )
            .unwrap();
        }
    }

    fn db_ids(path: &Path) -> Vec<String> {
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
        let mut stmt = conn.prepare("SELECT conversation_id FROM conversation_summaries").unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.flatten().collect()
    }

    fn store_ids(path: &Path) -> Vec<String> {
        let buf = fs::read(path).unwrap();
        pb_fields(&buf, 0, buf.len())
            .unwrap()
            .iter()
            .filter_map(|f| pb_record_id(&buf, f))
            .collect()
    }

    #[test]
    fn test_pb_drop_records_keeps_other_records_byte_for_byte() {
        let buf = store_bytes(&[("a", b"payload-a"), ("b", b"payload-b"), ("c", b"payload-c")]);
        let untouched = store_bytes(&[("a", b"payload-a"), ("c", b"payload-c")]);

        let remove: HashSet<&str> = ["b"].into_iter().collect();
        let (updated, dropped) = pb_drop_records(&buf, &remove).unwrap();

        assert_eq!(dropped, 1);
        assert_eq!(updated, untouched, "surviving records must not be rewritten");
        assert_eq!(pb_drop_records(&buf, &HashSet::new()).unwrap().0, buf);
    }

    #[test]
    fn test_pb_drop_records_rejects_unknown_layout() {
        // Not a valid record stream: the caller must leave the file alone.
        assert!(pb_drop_records(&[0xFF, 0xFF, 0xFF], &HashSet::new()).is_none());
    }

    #[test]
    fn test_prune_removes_store_record_and_mirror_row() {
        let home = TempHome::new("prune");
        let store = home.cli().join("jetbox_summaries_proto.pb");
        let db = home.cli().join("conversation_summaries.db");
        let records = [("keep-1", &b"one"[..]), ("drop-me", &b"two"[..]), ("keep-2", &b"three"[..])];
        fs::write(&store, store_bytes(&records)).unwrap();
        write_db(&db, &["keep-1", "drop-me", "keep-2"]);

        let adapter = AntigravityAdapter::with_home(home.0.clone());
        let pruned = adapter.prune_indexes(&["drop-me".to_string()]);

        assert_eq!(pruned, 1);
        assert_eq!(store_ids(&store), vec!["keep-1", "keep-2"]);
        assert_eq!(db_ids(&db), vec!["keep-1", "keep-2"]);
        assert!(store.with_extension("pb.pre-prune").exists(), "a backup is kept");
    }

    #[test]
    fn test_prune_reports_failure_when_the_store_still_lists_the_session() {
        let home = TempHome::new("stubborn");
        let store = home.cli().join("jetbox_summaries_proto.pb");
        let db = home.cli().join("conversation_summaries.db");
        // A layout this code does not understand: nothing may be rewritten, and the session must
        // be reported as still present rather than silently counted as deleted.
        fs::write(&store, [0xFF, 0xFF, 0xFF]).unwrap();
        write_db(&db, &["stubborn"]);

        let adapter = AntigravityAdapter::with_home(home.0.clone());
        let pruned = adapter.prune_indexes(&["stubborn".to_string()]);

        assert_eq!(pruned, 0);
        assert_eq!(fs::read(&store).unwrap(), vec![0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn test_prune_reports_success_when_only_the_mirror_row_exists() {
        let home = TempHome::new("mirror-only");
        let db = home.cli().join("conversation_summaries.db");
        write_db(&db, &["mirror-only"]);

        let adapter = AntigravityAdapter::with_home(home.0.clone());
        let pruned = adapter.prune_indexes(&["mirror-only".to_string()]);

        assert_eq!(pruned, 1);
        assert!(db_ids(&db).is_empty());
    }

    #[test]
    fn test_prune_clears_the_search_index() {
        let home = TempHome::new("fts");
        let db = home.cli().join("conversation_summaries.db");
        write_db(&db, &["keep", "drop"]);
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE VIRTUAL TABLE conversations_fts USING fts5(conversation_id UNINDEXED, title);
             INSERT INTO conversations_fts (conversation_id, title) VALUES ('keep', 'kept session');
             INSERT INTO conversations_fts (conversation_id, title) VALUES ('drop', 'gone session');",
        )
        .unwrap();
        drop(conn);

        let adapter = AntigravityAdapter::with_home(home.0.clone());
        assert_eq!(adapter.prune_indexes(&["drop".to_string()]), 1);

        let conn = Connection::open(&db).unwrap();
        let dropped: i64 = conn
            .query_row(
                "SELECT count(*) FROM conversations_fts WHERE conversation_id = 'drop'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let kept: i64 = conn
            .query_row(
                "SELECT count(*) FROM conversations_fts WHERE conversation_id = 'keep'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(dropped, 0);
        assert_eq!(kept, 1);
    }
}
