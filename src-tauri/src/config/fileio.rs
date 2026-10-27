use std::path::{Component, Path, PathBuf};

use serde_json::{Map, Value};

use super::models::{AppDocument, MCPServer, MCPTransport};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    WriteFailed(String),
    NotInstalled(String),
}

impl ApplyError {
    pub fn message(&self) -> String {
        match self {
            Self::WriteFailed(path) => format!("写入失败：{path}"),
            Self::NotInstalled(name) => format!("{name} 未安装，已跳过"),
        }
    }
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for ApplyError {}

pub fn write_failed(path: &Path) -> ApplyError {
    ApplyError::WriteFailed(path.display().to_string())
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut temp = path.to_path_buf();
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) => {
            temp.set_extension(format!("{ext}.tmp"));
        }
        None => {
            temp.set_extension("tmp");
        }
    }
    temp
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), ApplyError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(|_| write_failed(path))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<(), ApplyError> {
    Ok(())
}

pub fn atomic_write(path: &Path, data: &[u8], permissions: u32) -> Result<(), ApplyError> {
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).map_err(|_| write_failed(path))?;
    }
    let temp = tmp_path(path);
    if std::fs::write(&temp, data).is_err() {
        let _ = std::fs::remove_file(&temp);
        return Err(write_failed(path));
    }
    if path.exists() {
        #[cfg(not(unix))]
        {
            let _ = std::fs::remove_file(path);
        }
        if std::fs::rename(&temp, path).is_err() {
            let _ = std::fs::remove_file(&temp);
            return Err(write_failed(path));
        }
    } else if std::fs::rename(&temp, path).is_err() {
        let _ = std::fs::remove_file(&temp);
        return Err(write_failed(path));
    }
    set_mode(path, permissions)?;
    Ok(())
}

pub fn atomic_write_text(path: &Path, text: &str, permissions: u32) -> Result<(), ApplyError> {
    atomic_write(path, text.as_bytes(), permissions)
}

pub fn read_json_object(path: &Path) -> Map<String, Value> {
    let Ok(data) = std::fs::read(path) else {
        return Map::new();
    };
    match serde_json::from_slice::<Value>(&data) {
        Ok(Value::Object(object)) => object,
        _ => Map::new(),
    }
}

pub fn json_object_bytes(object: &Map<String, Value>) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec_pretty(&Value::Object(object.clone()))
}

pub fn write_json_object(object: &Map<String, Value>, path: &Path, permissions: u32) -> Result<(), ApplyError> {
    let data = json_object_bytes(object).map_err(|_| write_failed(path))?;
    atomic_write(path, &data, permissions)
}

pub fn write_json_map(
    session: &mut WriteSession,
    agent: &str,
    object: &Map<String, Value>,
    path: &Path,
    permissions: u32,
) -> Result<bool, ApplyError> {
    let data = json_object_bytes(object).map_err(|_| write_failed(path))?;
    session.write(agent, path, &data, permissions)
}

pub fn load_document(path: &Path) -> AppDocument {
    let Ok(data) = std::fs::read(path) else {
        return AppDocument::default();
    };
    match serde_json::from_slice::<AppDocument>(&data) {
        Ok(mut document) => {
            document.ensure_agent_defaults();
            document
        }
        Err(_) => AppDocument::default(),
    }
}

pub fn write_document(document: &AppDocument, path: &Path) -> Result<(), ApplyError> {
    let value = serde_json::to_value(document).map_err(|_| write_failed(path))?;
    let data = serde_json::to_vec_pretty(&value).map_err(|_| write_failed(path))?;
    atomic_write(path, &data, 0o600)
}

/// When `on` and `instructions` is non-empty after trim, replace the whole file
/// with that text plus exactly one trailing newline (mode 0644). Otherwise leave
/// the file untouched.
pub fn write_instruction_file(
    session: &mut WriteSession,
    agent: &str,
    path: &Path,
    instructions: &str,
    on: bool,
) -> Result<bool, ApplyError> {
    if !(on && !instructions.trim().is_empty()) {
        return Ok(false);
    }
    let trimmed = instructions.trim_end_matches(['\n', '\r']);
    session.write(agent, path, format!("{trimmed}\n").as_bytes(), 0o644)
}

pub struct WriteSession {
    pub root: PathBuf,
    pub home: PathBuf,
    pub stamp: String,
    pub written: Vec<PathBuf>,
    pub backups: Vec<PathBuf>,
}

fn strip_root(path: &Path) -> PathBuf {
    let mut components = path.components();
    loop {
        match components.next() {
            Some(Component::Prefix(_)) | Some(Component::RootDir) => continue,
            Some(first) => {
                let mut out = PathBuf::new();
                out.push(first.as_os_str());
                out.push(components.as_path());
                return out;
            }
            None => return PathBuf::new(),
        }
    }
}

fn backup_relative(home: &Path, path: &Path) -> PathBuf {
    if let Ok(rel) = path.strip_prefix(home) {
        if rel.as_os_str().is_empty() {
            return path
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("file"));
        }
        return rel.to_path_buf();
    }
    strip_root(path)
}

impl WriteSession {
    pub fn new(root: impl Into<PathBuf>, home: impl Into<PathBuf>) -> Self {
        Self::with_stamp(
            root,
            home,
            chrono::Local::now().format("%Y%m%d-%H%M%S").to_string(),
        )
    }

    pub fn with_stamp(root: impl Into<PathBuf>, home: impl Into<PathBuf>, stamp: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            home: home.into(),
            stamp: stamp.into(),
            written: Vec::new(),
            backups: Vec::new(),
        }
    }

    pub fn write(&mut self, agent: &str, path: &Path, contents: &[u8], mode: u32) -> Result<bool, ApplyError> {
        if let Ok(existing) = std::fs::read(path) {
            if existing == contents {
                return Ok(false);
            }
            let backup = self
                .root
                .join(&self.stamp)
                .join(agent)
                .join(backup_relative(&self.home, path));
            if !backup.exists() {
                if let Some(directory) = backup.parent() {
                    std::fs::create_dir_all(directory).map_err(|_| write_failed(path))?;
                }
                std::fs::copy(path, &backup).map_err(|_| write_failed(path))?;
                self.backups.push(backup);
            }
        }
        atomic_write(path, contents, mode)?;
        self.written.push(path.to_path_buf());
        Ok(true)
    }
}

pub fn prune_backups(root: &Path, keep: usize) -> Result<(), ApplyError> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Ok(());
    };
    let mut stamps: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    stamps.sort();
    let extra = stamps.len().saturating_sub(keep);
    for path in stamps.into_iter().take(extra) {
        std::fs::remove_dir_all(&path).map_err(|_| ApplyError::WriteFailed(path.display().to_string()))?;
    }
    Ok(())
}

pub fn expand_user_path(raw: &str, home: &Path) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return home.join(rest);
    }
    PathBuf::from(trimmed)
}

pub fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

pub fn open_code_mcp(server: &MCPServer) -> Map<String, Value> {
    match server.transport {
        MCPTransport::Stdio => {
            let mut command = vec![server.command.clone()];
            command.extend(server.args.iter().cloned());
            command.retain(|item| !item.is_empty());
            let mut object = Map::new();
            object.insert("type".into(), Value::String("local".into()));
            object.insert(
                "command".into(),
                Value::Array(command.into_iter().map(Value::String).collect()),
            );
            if !server.env.is_empty() {
                let mut env = Map::new();
                for (key, value) in &server.env {
                    env.insert(key.clone(), Value::String(value.clone()));
                }
                object.insert("environment".into(), Value::Object(env));
            }
            object
        }
        MCPTransport::Http | MCPTransport::Sse => {
            let mut object = Map::new();
            object.insert("type".into(), Value::String("remote".into()));
            object.insert("url".into(), Value::String(server.url.clone()));
            object
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::{
        APIKind, AgentKind, AgentSettings, AppDocument, MCPServer, MCPTransport, ModelEntry, Provider,
    };
    use std::collections::BTreeMap;

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

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn sample_document() -> AppDocument {
        let mut document = AppDocument::default();
        document.providers = vec![Provider {
            id: "g".into(),
            name: "GJ".into(),
            base_url: "https://example.invalid/v1".into(),
            api: APIKind::Responses,
            catalog_url: String::new(),
            api_key: "sk-test".into(),
            compat_base_url: String::new(),
            models: vec![ModelEntry::new("flash", "Flash")],
        }];
        document.set_settings(
            AgentSettings {
                model: "g/flash".into(),
                effort: "high".into(),
                instruction_on: true,
                mcp_ids: vec!["dbx".into()],
                ..AgentSettings::default()
            },
            AgentKind::Opencode,
        );
        document
    }

    #[test]
    fn round_trip_document() {
        let dir = TestDir::new("state");
        let url = dir.path().join("state.json");
        let document = sample_document();
        write_document(&document, &url).unwrap();
        let loaded = load_document(&url);
        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.providers[0].name, "GJ");
        assert_eq!(loaded.providers[0].api_key, "sk-test");

        let provider: Provider = serde_json::from_str(
            r#"{ "id": "g", "name": "GJ", "baseURL": "https://example.invalid/v1" }"#,
        )
        .unwrap();
        assert_eq!(provider.api_key, "");
        assert_eq!(loaded.settings(AgentKind::Opencode).model, "g/flash");
        assert_eq!(loaded.settings(AgentKind::Opencode).mcp_ids, ["dbx"]);
        assert_eq!(loaded.settings(AgentKind::Dsh).effort, "high");
    }

    #[test]
    fn decode_ignores_unknown_agent_keys() {
        let raw = r#"{
            "version": 1,
            "providers": [],
            "library": { "instructions": "", "mcp": [] },
            "agents": {
                "codex": { "model": "x" },
                "opencode": { "model": "g/flash" }
            },
            "appliedAt": { "codex": "t", "opencode": "2026-01-01T00:00:00Z" }
        }"#;
        let document: AppDocument = serde_json::from_str(raw).unwrap();
        assert!(!document.agents.contains_key("codex"));
        assert_eq!(
            document.agents.get("opencode").map(|s| s.model.as_str()),
            Some("g/flash")
        );
        assert!(!document.applied_at.contains_key("codex"));
        assert_eq!(
            document.applied_at.get("opencode").map(String::as_str),
            Some("2026-01-01T00:00:00Z")
        );
    }

    #[test]
    fn apply_error_messages() {
        assert_eq!(
            ApplyError::WriteFailed("/tmp/x".into()).to_string(),
            "写入失败：/tmp/x"
        );
        assert_eq!(
            ApplyError::NotInstalled("OpenCode".into()).to_string(),
            "OpenCode 未安装，已跳过"
        );
    }

    #[test]
    fn instruction_file_replaces_whole_file_when_on() {
        let dir = TestDir::new("instr");
        let path = dir.path().join("AGENTS.md");
        std::fs::write(&path, "old\ncontent\n").unwrap();
        let mut session = WriteSession::with_stamp(dir.path().join("backups"), dir.path(), "stamp");
        write_instruction_file(&mut session, "opencode", &path, "keep diffs small\n\n", true).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep diffs small\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o644);
        }
    }

    #[test]
    fn instruction_file_leaves_untouched_when_off_or_empty() {
        let dir = TestDir::new("instr_off");
        let path = dir.path().join("AGENTS.md");
        std::fs::write(&path, "keep me\n").unwrap();
        let mut session = WriteSession::with_stamp(dir.path().join("backups"), dir.path(), "stamp");
        write_instruction_file(&mut session, "opencode", &path, "new body", false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep me\n");
        write_instruction_file(&mut session, "opencode", &path, "   \n", true).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep me\n");
        assert!(session.written.is_empty());
    }

    #[test]
    fn write_session_skips_identical_and_backs_up_changes() {
        let dir = TestDir::new("backup");
        let home = dir.path();
        let backups = home.join("backups");
        let target = home.join("opencode.json");
        std::fs::write(&target, b"one").unwrap();
        let mut session = WriteSession::with_stamp(&backups, home, "20260102-030405");
        assert!(!session.write("opencode", &target, b"one", 0o600).unwrap());
        assert!(session.written.is_empty());
        assert!(session.backups.is_empty());

        assert!(session.write("opencode", &target, b"two", 0o600).unwrap());
        assert_eq!(std::fs::read(&target).unwrap(), b"two");
        let backup = backups.join("20260102-030405/opencode/opencode.json");
        assert_eq!(std::fs::read(&backup).unwrap(), b"one");
        assert_eq!(session.backups, vec![backup]);
    }

    #[test]
    fn write_session_writes_new_file_without_backup() {
        let dir = TestDir::new("backup_new");
        let home = dir.path();
        let backups = home.join("backups");
        let target = home.join("nested/config.json");
        let mut session = WriteSession::with_stamp(&backups, home, "20260102-030405");
        assert!(session.write("dsh", &target, b"fresh", 0o600).unwrap());
        assert_eq!(std::fs::read(&target).unwrap(), b"fresh");
        assert!(session.backups.is_empty());
        assert!(!backups.join("20260102-030405").exists());
    }

    #[test]
    fn write_session_mirrors_path_and_keeps_existing_backup() {
        let dir = TestDir::new("backup_mirror");
        let home = dir.path();
        let backups = home.join("backups");
        let v2 = home.join(".zcode/v2/config.json");
        let cli = home.join(".zcode/cli/config.json");
        std::fs::create_dir_all(v2.parent().unwrap()).unwrap();
        std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
        std::fs::write(&v2, b"v2-old").unwrap();
        std::fs::write(&cli, b"cli-old").unwrap();
        let mut session = WriteSession::with_stamp(&backups, home, "20260102-030405");
        assert!(session.write("zcode", &v2, b"v2-new", 0o600).unwrap());
        assert!(session.write("zcode", &cli, b"cli-new", 0o600).unwrap());
        let v2_backup = backups.join("20260102-030405/zcode/.zcode/v2/config.json");
        let cli_backup = backups.join("20260102-030405/zcode/.zcode/cli/config.json");
        assert_eq!(std::fs::read(&v2_backup).unwrap(), b"v2-old");
        assert_eq!(std::fs::read(&cli_backup).unwrap(), b"cli-old");

        assert!(session.write("zcode", &v2, b"v2-newer", 0o600).unwrap());
        assert_eq!(std::fs::read(&v2_backup).unwrap(), b"v2-old");
        assert_eq!(std::fs::read(&v2).unwrap(), b"v2-newer");
    }

    #[test]
    fn prune_backups_keeps_newest() {
        let dir = TestDir::new("prune");
        for stamp in ["20260101-000000", "20260102-000000", "20260103-000000"] {
            std::fs::create_dir_all(dir.path().join(stamp)).unwrap();
        }
        prune_backups(dir.path(), 2).unwrap();
        let mut left: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["20260102-000000", "20260103-000000"]);
    }

    #[test]
    fn open_code_mcp_encoding() {
        let local = MCPServer {
            id: "dbx".into(),
            transport: MCPTransport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "dbx".into()],
            url: String::new(),
            env: BTreeMap::from([("TOKEN".into(), "abc".into())]),
        };
        let encoded = open_code_mcp(&local);
        assert_eq!(encoded.get("type").and_then(Value::as_str), Some("local"));
        assert_eq!(
            encoded.get("command"),
            Some(&Value::Array(vec![
                Value::String("npx".into()),
                Value::String("-y".into()),
                Value::String("dbx".into()),
            ]))
        );
        assert_eq!(
            encoded
                .get("environment")
                .and_then(Value::as_object)
                .and_then(|env| env.get("TOKEN"))
                .and_then(Value::as_str),
            Some("abc")
        );

        let remote = MCPServer {
            id: "search".into(),
            transport: MCPTransport::Http,
            url: "https://example.invalid/mcp".into(),
            ..MCPServer::new("search")
        };
        let encoded = open_code_mcp(&remote);
        assert_eq!(encoded.get("type").and_then(Value::as_str), Some("remote"));
        assert_eq!(
            encoded.get("url").and_then(Value::as_str),
            Some("https://example.invalid/mcp")
        );
    }

    #[test]
    fn json_object_empty_on_failure_and_pretty_write() {
        let dir = TestDir::new("json");
        let missing = dir.path().join("nope.json");
        assert!(read_json_object(&missing).is_empty());
        std::fs::write(&missing, b"[]").unwrap();
        assert!(read_json_object(&missing).is_empty());

        let mut object = Map::new();
        object.insert("z".into(), Value::String("https://a/b".into()));
        object.insert("a".into(), Value::from(1));
        let path = dir.path().join("out.json");
        write_json_object(&object, &path, 0o600).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("https://a/b"));
        assert!(!text.contains(r"https:\/\/a\/b"));
        let parsed = read_json_object(&path);
        assert_eq!(parsed.get("a").and_then(Value::as_i64), Some(1));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }
}
