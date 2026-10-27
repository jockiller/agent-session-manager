use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::adapters::{make_first_prompt, user_text_from_content, AgentAdapter};
use crate::models::{ChatMessage, SessionSummary, ToolCallItem};

pub struct DshAdapter;

/// Metadata DSH already projects per session into `storages/session_projcache`.
#[derive(Default)]
struct Projection {
    title: Option<String>,
}

impl DshAdapter {
    fn home_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("DSH_HOME") {
            PathBuf::from(dir)
        } else if let Some(home) = crate::adapters::user_home() {
            home.join(".dsh")
        } else {
            PathBuf::from(".dsh")
        }
    }

    /// Sessions the DSH desktop archived, from `storages/workspace.json`.
    fn archived_ids(home: &Path) -> HashSet<String> {
        let mut archived = HashSet::new();
        let path = home.join("storages").join("workspace.json");
        let Ok(text) = fs::read_to_string(path) else {
            return archived;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            return archived;
        };
        if let Some(ids) = value.pointer("/global/archivedSessionIds").and_then(Value::as_array) {
            for id in ids.iter().filter_map(Value::as_str) {
                archived.insert(id.strip_prefix("session-").unwrap_or(id).to_string());
            }
        }
        archived
    }

    /// Format generation encoded in a log filename: `session.jsonl.zstd` is v0,
    /// `session.v4.jsonl.zstd` is v4.
    fn log_version(name: &str) -> Option<u32> {
        let stem = name.strip_suffix(".zstd").unwrap_or(name);
        let stem = stem.strip_suffix(".jsonl")?;
        let rest = stem.strip_prefix("session")?;
        if rest.is_empty() {
            return Some(0);
        }
        rest.strip_prefix(".v")?.parse().ok()
    }

    /// DSH bumps the log generation together with the session format, so read whichever
    /// generation this directory actually holds instead of assuming one.
    fn find_log(dir: &Path) -> Option<PathBuf> {
        let mut newest: Option<(u32, PathBuf)> = None;
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let Some(version) = Self::log_version(name) else {
                continue;
            };
            if newest.as_ref().map_or(true, |(seen, _)| version > *seen) {
                newest = Some((version, path));
            }
        }
        newest.map(|(_, path)| path)
    }

    fn open_log(path: &Path) -> Option<Box<dyn BufRead>> {
        let file = File::open(path).ok()?;
        if path.extension().and_then(|e| e.to_str()) == Some("zstd") {
            let decoder = zstd::stream::read::Decoder::new(file).ok()?;
            Some(Box::new(BufReader::new(decoder)))
        } else {
            Some(Box::new(BufReader::new(file)))
        }
    }

    /// DSH takes a non-blocking `flock(2)` write lease on `session.lock` for as long as it
    /// keeps a session open and never removes the file on release, so a probe of the lock
    /// itself — not the file's existence — tells whether a writer is attached.
    #[cfg(unix)]
    fn writer_holds_lease(lock: &Path) -> bool {
        crate::adapters::flock_is_held(lock)
    }

    /// Windows guards writes with a named kernel semaphore and writes no lock file.
    #[cfg(not(unix))]
    fn writer_holds_lease(_lock: &Path) -> bool {
        false
    }

    fn projection(home: &Path, dirname: &str) -> Projection {
        let mut projection = Projection::default();
        let path = home
            .join("storages")
            .join("session_projcache")
            .join("sessions")
            .join(format!("{dirname}.json"));
        let Ok(text) = fs::read_to_string(path) else {
            return projection;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            return projection;
        };

        if let Some(title) = value.pointer("/record/rows/title/val").and_then(Value::as_str) {
            let title = title.trim();
            if !title.is_empty() {
                projection.title = Some(title.to_string());
            }
        }

        projection
    }

    /// User text of one message event: v3 stores a `text` string, v4 a `content` block list.
    fn message_text(data: &Value) -> Option<String> {
        data.get("text")
            .and_then(user_text_from_content)
            .or_else(|| data.get("content").and_then(user_text_from_content))
    }

    /// DSH injects workspace instructions and skill catalogs as standalone user messages.
    fn is_wrapper_message(text: &str) -> bool {
        let text = text.trim_start();
        ["<system-reminder", "<user_info", "<user_instructions", "<environment_context"]
            .iter()
            .any(|prefix| text.starts_with(prefix))
    }

    /// DSH also records runtime context, skill catalogs, job notices and approval changes as
    /// user messages; only `source.kind == "user"` is text the person actually typed.
    fn is_injected_user_message(data: &Value) -> bool {
        match data.get("source").and_then(|s| s.get("kind")).and_then(Value::as_str) {
            None | Some("user") => false,
            Some(_) => true,
        }
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

impl AgentAdapter for DshAdapter {
    fn platform_id(&self) -> &'static str {
        "dsh"
    }

    fn display_name(&self) -> &'static str {
        "DSH"
    }

    fn scan_sessions(&self) -> Vec<SessionSummary> {
        let home = Self::home_dir();
        let sessions_dir = home.join("sessions");
        if !sessions_dir.exists() {
            return Vec::new();
        }

        let archived = Self::archived_ids(&home);
        let mut summaries = Vec::new();

        for ws_entry in fs::read_dir(&sessions_dir).into_iter().flatten().flatten() {
            let ws_path = ws_entry.path();
            if !ws_path.is_dir() {
                continue;
            }

            for s_entry in fs::read_dir(&ws_path).into_iter().flatten().flatten() {
                let spath = s_entry.path();
                if !spath.is_dir() {
                    continue;
                }

                let dirname = spath.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                let sid = dirname.strip_prefix("session-").unwrap_or(&dirname).to_string();
                let log_file = Self::find_log(&spath);

                let mut title = String::new();
                let mut first_prompt = String::new();
                let mut cwd = String::new();
                let mut created_at = 0i64;
                let mut updated_at = 0i64;
                let mut turn_count = 0u32;
                let mut is_subagent = false;
                let mut parent_id: Option<String> = None;

                if let Some(reader) = log_file.as_deref().and_then(Self::open_log) {
                    for line in reader.lines().flatten() {
                        let Ok(val) = serde_json::from_str::<Value>(&line) else {
                            continue;
                        };
                        let etype = val.get("type").and_then(Value::as_str).unwrap_or("");

                        if created_at == 0 {
                            if let Some(ca) = val.get("createdAt").and_then(Value::as_i64) {
                                created_at = ca;
                            }
                        }
                        if let Some(t) = val.get("time").and_then(Value::as_i64) {
                            updated_at = t;
                        }

                        match etype {
                            "session" => {
                                if cwd.is_empty() {
                                    if let Some(c) = val.get("cwd").and_then(Value::as_str) {
                                        cwd = c.to_string();
                                    }
                                }
                                if val.get("origin").and_then(Value::as_str) == Some("subagent") {
                                    is_subagent = true;
                                }
                                if val.get("delegationDepth").and_then(Value::as_i64).map_or(false, |d| d > 0) {
                                    is_subagent = true;
                                }
                                if let Some(parent) = val.get("parentSession").and_then(Value::as_str) {
                                    let p = parent.strip_prefix("session-").unwrap_or(parent);
                                    parent_id = Some(p.to_string());
                                    is_subagent = true;
                                }
                            }
                            "session/title" => {
                                if let Some(data) = val.get("data") {
                                    if let Some(t) =
                                        data.get("title").or_else(|| data.get("value")).and_then(Value::as_str)
                                    {
                                        let t = t.trim();
                                        if !t.is_empty() {
                                            title = t.to_string();
                                        }
                                    }
                                }
                            }
                            "turn/start" => turn_count += 1,
                            "user/message" => {
                                if first_prompt.is_empty() {
                                    if let Some(data) = val.get("data") {
                                        if !Self::is_injected_user_message(data) {
                                            if let Some(text) = Self::message_text(data) {
                                                if !Self::is_wrapper_message(&text) {
                                                    if let Some(fp) = make_first_prompt(&text) {
                                                        first_prompt = fp;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }

                let projection = Self::projection(&home, &dirname);
                if title.is_empty() {
                    if let Some(projected) = projection.title {
                        title = projected;
                    }
                }

                if updated_at == 0 {
                    if let Ok(meta) = spath.metadata() {
                        if let Ok(mtime) = meta.modified() {
                            if let Ok(d) = mtime.duration_since(std::time::UNIX_EPOCH) {
                                updated_at = d.as_millis() as i64;
                                if created_at == 0 {
                                    created_at = updated_at;
                                }
                            }
                        }
                    }
                }
                if created_at == 0 {
                    created_at = updated_at;
                }

                let total_size = Self::path_size(&spath);
                let main_path = log_file
                    .as_ref()
                    .map_or_else(|| spath.to_string_lossy().to_string(), |p| p.to_string_lossy().to_string());

                summaries.push(SessionSummary {
                    id: sid.clone(),
                    platform: "dsh".to_string(),
                    flavor: "dsh".to_string(),
                    dirname,
                    main_path,
                    all_paths: vec![spath.to_string_lossy().to_string()],
                    cwd,
                    title,
                    first_prompt,
                    created_at: crate::adapters::to_millis(created_at),
                    updated_at: crate::adapters::to_millis(updated_at),
                    size_bytes: total_size,
                    turn_count,
                    is_subagent,
                    parent_id,
                    is_running: Self::writer_holds_lease(&spath.join("session.lock")),
                    is_archived: archived.contains(&sid),
                    has_transcript: log_file.is_some(),
                });
            }
        }

        summaries
    }

    fn load_messages(&self, session: &SessionSummary, max_msgs: usize) -> Vec<ChatMessage> {
        let fpath = Path::new(&session.main_path);
        if !fpath.is_file() {
            return Vec::new();
        }
        let Some(reader) = Self::open_log(fpath) else {
            return Vec::new();
        };

        let mut msgs = Vec::new();
        for line in reader.lines().flatten() {
            let Ok(val) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let etype = val.get("type").and_then(Value::as_str).unwrap_or("");
            let time = val.get("time").and_then(Value::as_i64).map(|t| t.to_string()).unwrap_or_default();

            match etype {
                "user/message" => {
                    let Some(data) = val.get("data") else {
                        continue;
                    };
                    if Self::is_injected_user_message(data) {
                        continue;
                    }
                    let Some(text) = Self::message_text(data) else {
                        continue;
                    };
                    if Self::is_wrapper_message(&text) {
                        continue;
                    }
                    msgs.push(ChatMessage {
                        role: "user".to_string(),
                        text,
                        time,
                        msg_type: etype.to_string(),
                        thinking: None,
                        tool_calls: Vec::new(),
                    });
                }
                "bot/message" => {
                    let text = val
                        .get("data")
                        .and_then(|d| d.get("text"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    if !text.is_empty() {
                        msgs.push(ChatMessage {
                            role: "assistant".to_string(),
                            text,
                            time,
                            msg_type: etype.to_string(),
                            thinking: None,
                            tool_calls: Vec::new(),
                        });
                    }
                }
                "assistant/message" => {
                    let Some(message) = val.get("data").and_then(|d| d.get("message")) else {
                        continue;
                    };
                    let mut text = Vec::new();
                    let mut thinking = Vec::new();
                    let mut tool_calls = Vec::new();
                    if let Some(parts) = message.get("content").and_then(Value::as_array) {
                        for part in parts {
                            match part.get("type").and_then(Value::as_str).unwrap_or("") {
                                "text" => {
                                    if let Some(t) = part.get("text").and_then(Value::as_str) {
                                        if !t.trim().is_empty() {
                                            text.push(t.to_string());
                                        }
                                    }
                                }
                                "reasoning" => {
                                    if let Some(t) = part.get("text").and_then(Value::as_str) {
                                        if !t.trim().is_empty() {
                                            thinking.push(t.to_string());
                                        }
                                    }
                                }
                                "tool-call" => {
                                    let name = part
                                        .get("name")
                                        .and_then(Value::as_str)
                                        .unwrap_or("tool")
                                        .to_string();
                                    let args = part.get("arguments").map(|v| match v {
                                        Value::String(s) => s.clone(),
                                        other => other.to_string(),
                                    });
                                    tool_calls.push(ToolCallItem { name, args, output: None });
                                }
                                _ => {}
                            }
                        }
                    }
                    if text.is_empty() && thinking.is_empty() && tool_calls.is_empty() {
                        continue;
                    }
                    msgs.push(ChatMessage {
                        role: "assistant".to_string(),
                        text: text.join("\n"),
                        time,
                        msg_type: etype.to_string(),
                        thinking: if thinking.is_empty() { None } else { Some(thinking.join("\n\n")) },
                        tool_calls,
                    });
                }
                _ => {}
            }

            if msgs.len() >= max_msgs {
                break;
            }
        }

        msgs
    }

    fn prune_indexes(&self, session_ids: &[String]) -> usize {
        let home = Self::home_dir();
        let set: HashSet<&str> = session_ids.iter().map(String::as_str).collect();
        let dir = home
            .join("storages")
            .join("session_projcache")
            .join("sessions");
        let mut pruned = 0;

        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let id = stem.strip_prefix("session-").unwrap_or(stem);
            if set.contains(id) && fs::remove_file(&path).is_ok() {
                pruned += 1;
            }
        }

        pruned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::io::AsRawFd;

    fn temp_home(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dsh-adapter-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_session(home: &Path, workspace: &str, dirname: &str, events: &[&str]) {
        let dir = home.join("sessions").join(workspace).join(dirname);
        fs::create_dir_all(&dir).unwrap();
        let mut body = events.join("\n");
        body.push('\n');
        let compressed = zstd::stream::encode_all(body.as_bytes(), 0).unwrap();
        fs::write(dir.join("session.v4.jsonl.zstd"), compressed).unwrap();
    }

    #[test]
    fn test_log_version_parsing() {
        assert_eq!(DshAdapter::log_version("session.jsonl.zstd"), Some(0));
        assert_eq!(DshAdapter::log_version("session.jsonl"), Some(0));
        assert_eq!(DshAdapter::log_version("session.v4.jsonl.zstd"), Some(4));
        assert_eq!(DshAdapter::log_version("session.v12.jsonl"), Some(12));
        assert_eq!(DshAdapter::log_version("session.lock"), None);
        assert_eq!(DshAdapter::log_version("notes.txt"), None);
    }

    #[test]
    fn test_scans_v4_metadata_messages_and_archive() {
        let home = temp_home("scan");
        let workspace = "--tmp-ws--";

        write_session(
            &home,
            workspace,
            "session-aaaa",
            &[
                r#"{"type":"session","version":4,"id":"session-aaaa","createdAt":1700000000000,"cwd":"/tmp/ws","isSeeded":false,"delegationDepth":0}"#,
                r#"{"type":"session/title","seq":1,"time":1700000000100,"data":{"title":"修复 DSH 适配"}}"#,
                r#"{"type":"turn/start","seq":2,"time":1700000000200,"data":{"turn":1}}"#,
                r#"{"type":"user/message","seq":3,"time":1700000000200,"data":{"content":[{"type":"text","text":"The approval policy changed from \"ask\" to \"never\""}],"source":{"kind":"user-approval"},"role":"user"}}"#,
                r#"{"type":"user/message","seq":4,"time":1700000000200,"data":{"content":[{"type":"text","text":"帮我看看为什么全是运行中"}],"source":{"kind":"user"},"role":"user"}}"#,
                r#"{"type":"user/message","seq":5,"time":1700000000201,"data":{"content":[{"type":"text","text":"<system-reminder>skills</system-reminder>"}],"source":{"kind":"skill-catalog"},"role":"user"}}"#,
                r#"{"type":"user/message","seq":6,"time":1700000000202,"data":{"content":[{"type":"text","text":"Current runtime context. This snapshot supersedes earlier snapshots."}],"source":{"kind":"runtime-context"},"role":"user"}}"#,
                r#"{"type":"assistant/message","seq":7,"time":1700000000300,"data":{"message":{"role":"assistant","content":[{"type":"reasoning","text":"先看日志"},{"type":"text","text":"因为锁文件常驻"},{"type":"tool-call","name":"bash","arguments":"{\"command\":\"ls\"}"}]}}}"#,
                r#"{"type":"turn/start","seq":8,"time":1700000000400,"data":{"turn":2}}"#,
            ],
        );
        write_session(
            &home,
            workspace,
            "session-bbbb",
            &[r#"{"type":"session","version":4,"id":"session-bbbb","createdAt":1700000000000,"isSeeded":false,"delegationDepth":1}"#],
        );

        fs::create_dir_all(home.join("storages").join("session_projcache").join("sessions")).unwrap();
        fs::write(
            home.join("storages").join("workspace.json"),
            r#"{"global":{"archivedSessionIds":["session-aaaa"]}}"#,
        )
        .unwrap();
        fs::write(
            home.join("storages").join("session_projcache").join("sessions").join("session-aaaa.json"),
            r#"{"record":{"rows":{"title":{"val":"忽略：日志标题优先"},"tokenUsage":{"val":{"totals":{"uncachedInputTokens":100,"outputTokens":20,"cacheReadTokens":30,"cacheWriteTokens":5}}}}}}"#,
        )
        .unwrap();

        // The second session is held open the way DSH holds it.
        #[cfg(unix)]
        let lease = {
            let lock = File::create(home.join("sessions").join(workspace).join("session-bbbb").join("session.lock")).unwrap();
            assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) }, 0);
            lock
        };

        std::env::set_var("DSH_HOME", &home);
        let sessions = DshAdapter.scan_sessions();

        let titled = sessions.iter().find(|s| s.id == "aaaa").expect("archived session scanned");
        assert_eq!(titled.title, "修复 DSH 适配");
        assert_eq!(titled.first_prompt, "帮我看看为什么全是运行中");
        assert_eq!(titled.turn_count, 2);
        assert_eq!(titled.cwd, "/tmp/ws");
        assert_eq!(titled.created_at, 1700000000000);
        assert_eq!(titled.updated_at, 1700000000400);
        assert!(titled.has_transcript);
        assert!(titled.is_archived);
        assert!(!titled.is_running);

        let running = sessions.iter().find(|s| s.id == "bbbb").expect("locked session scanned");
        #[cfg(unix)]
        assert!(running.is_running, "held flock marks the session as running");
        assert!(running.is_subagent, "delegationDepth > 0 marks a subagent");
        assert!(!running.is_archived);

        let messages = DshAdapter.load_messages(titled, 50);
        assert_eq!(messages.len(), 2, "injected context and approval notices are skipped");
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].text, "帮我看看为什么全是运行中");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].text, "因为锁文件常驻");
        assert_eq!(messages[1].thinking.as_deref(), Some("先看日志"));
        assert_eq!(messages[1].tool_calls.len(), 1);
        assert_eq!(messages[1].tool_calls[0].name, "bash");

        std::env::remove_var("DSH_HOME");
        #[cfg(unix)]
        drop(lease);
        let _ = fs::remove_dir_all(&home);
    }
}
