use std::collections::HashSet;
use std::path::Path;

use serde_json::{Map, Value};

use super::json_object;
use crate::config::apply::{AgentPaths, ApplyContext, ApplyRequest};
use crate::config::fileio::{read_json_object, read_text, write_json_map, ApplyError};
use crate::config::models::{AgentKind, MCPServer, MCPTransport};

pub fn apply(
    request: &ApplyRequest,
    paths: &AgentPaths,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    if request.write_mcp {
        match request.kind {
            AgentKind::Codex => apply_codex_toml(request, &paths.config, ctx)?,
            AgentKind::Antigravity
            | AgentKind::Claude
            | AgentKind::Gemini
            | AgentKind::Cursor
            | AgentKind::Qwen => apply_json_mcp(request, &paths.config, ctx)?,
            _ => {}
        }
    }

    if request.write_instructions
        && request.kind.supports_instructions()
        && !paths.instructions.as_os_str().is_empty()
    {
        ctx.write_instructions(
            request.kind,
            &paths.instructions,
            &request.instructions,
            request.settings.instruction_on,
        )?;
    }

    Ok(())
}

fn json_file_mode(kind: AgentKind) -> u32 {
    match kind {
        AgentKind::Claude => 0o600,
        _ => 0o644,
    }
}

fn json_mcp_entry(kind: AgentKind, server: &MCPServer) -> Map<String, Value> {
    let mut entry = Map::new();
    match server.transport {
        MCPTransport::Stdio => {
            if kind == AgentKind::Claude {
                entry.insert("type".into(), Value::String("stdio".into()));
            }
            entry.insert("command".into(), Value::String(server.command.clone()));
            entry.insert(
                "args".into(),
                Value::Array(server.args.iter().cloned().map(Value::String).collect()),
            );
            if !server.env.is_empty() {
                let mut env = Map::new();
                for (k, v) in &server.env {
                    env.insert(k.clone(), Value::String(v.clone()));
                }
                entry.insert("env".into(), Value::Object(env));
            }
        }
        MCPTransport::Http => match kind {
            AgentKind::Gemini | AgentKind::Qwen => {
                entry.insert("httpUrl".into(), Value::String(server.url.clone()));
            }
            AgentKind::Claude => {
                entry.insert("type".into(), Value::String("http".into()));
                entry.insert("url".into(), Value::String(server.url.clone()));
            }
            _ => {
                entry.insert("url".into(), Value::String(server.url.clone()));
            }
        },
        MCPTransport::Sse => {
            if kind == AgentKind::Claude {
                entry.insert("type".into(), Value::String("sse".into()));
            }
            entry.insert("url".into(), Value::String(server.url.clone()));
        }
    }
    entry
}

fn apply_json_mcp(
    request: &ApplyRequest,
    path: &Path,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    let mut root = read_json_object(path);
    let mut servers = json_object(root.get("mcpServers"));

    let assigned: HashSet<&str> = request.mcp.iter().map(|s| s.id.as_str()).collect();
    let managed: HashSet<&str> = request
        .managed_mcp_ids
        .iter()
        .chain(request.settings.mcp_ids.iter())
        .map(String::as_str)
        .collect();

    let mut removed_any = false;
    let existing_keys: Vec<String> = servers.keys().cloned().collect();
    for key in existing_keys {
        if managed.contains(key.as_str()) && !assigned.contains(key.as_str()) {
            servers.remove(&key);
            removed_any = true;
        }
    }

    for server in &request.mcp {
        servers.insert(
            server.id.clone(),
            Value::Object(json_mcp_entry(request.kind, server)),
        );
    }

    if servers.is_empty() {
        root.remove("mcpServers");
    } else {
        root.insert("mcpServers".into(), Value::Object(servers));
    }

    if request.mcp.is_empty() && !removed_any {
        return Ok(());
    }

    if root.is_empty() && !path.exists() {
        return Ok(());
    }

    write_json_map(
        ctx.session,
        request.kind.raw(),
        &root,
        path,
        json_file_mode(request.kind),
    )?;
    Ok(())
}

fn toml_key(raw: &str) -> String {
    format!("\"{}\"", raw.replace('\\', "\\\\").replace('"', "\\\""))
}

fn toml_bare_or_quoted(raw: &str) -> String {
    if !raw.is_empty()
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        raw.to_string()
    } else {
        toml_key(raw)
    }
}

fn toml_string(raw: &str) -> String {
    let escaped = raw.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn mcp_root_name(header: &str) -> Option<String> {
    let inner = header
        .strip_prefix("[mcp_servers.")?
        .strip_suffix(']')?
        .trim();
    if let Some(rest) = inner.strip_prefix('"') {
        let mut out = String::new();
        let mut escaped = false;
        for ch in rest.chars() {
            if escaped {
                out.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                return Some(out);
            } else {
                out.push(ch);
            }
        }
        return Some(out);
    }
    Some(
        inner
            .split('.')
            .next()
            .unwrap_or(inner)
            .trim()
            .to_string(),
    )
}

fn drop_managed_mcp_tables(text: &str, drop_ids: &HashSet<&str>) -> String {
    if drop_ids.is_empty() || text.is_empty() {
        return text.to_string();
    }
    let mut kept = Vec::new();
    let mut skipping = false;
    for line in text.split('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            skipping = mcp_root_name(trimmed)
                .map(|root| drop_ids.contains(root.as_str()))
                .unwrap_or(false);
        }
        if skipping {
            continue;
        }
        kept.push(line);
    }
    kept.join("\n")
}

fn codex_mcp_block(server: &MCPServer) -> String {
    let mut lines = vec![format!("[mcp_servers.{}]", toml_key(&server.id))];
    if server.transport == MCPTransport::Stdio {
        lines.push(format!("command = {}", toml_string(&server.command)));
        if !server.args.is_empty() {
            let args: Vec<String> = server.args.iter().map(|a| toml_string(a)).collect();
            lines.push(format!("args = [{}]", args.join(", ")));
        }
    } else {
        lines.push(format!("url = {}", toml_string(&server.url)));
    }
    if !server.env.is_empty() {
        lines.push(String::new());
        lines.push(format!("[mcp_servers.{}.env]", toml_key(&server.id)));
        for (k, v) in &server.env {
            lines.push(format!("{} = {}", toml_bare_or_quoted(k), toml_string(v)));
        }
    }
    lines.join("\n")
}

fn apply_codex_toml(
    request: &ApplyRequest,
    path: &Path,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    let existing = read_text(path);
    let drop_ids: HashSet<&str> = request
        .managed_mcp_ids
        .iter()
        .chain(request.settings.mcp_ids.iter())
        .map(String::as_str)
        .chain(request.mcp.iter().map(|s| s.id.as_str()))
        .collect();

    let remaining = drop_managed_mcp_tables(&existing, &drop_ids);
    if request.mcp.is_empty() && remaining == existing {
        return Ok(());
    }
    let blocks: Vec<String> = request.mcp.iter().map(codex_mcp_block).collect();

    let mut body = remaining.trim().to_string();
    if !blocks.is_empty() {
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str(&blocks.join("\n\n"));
    }
    if body.is_empty() && !path.exists() {
        return Ok(());
    }
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }

    ctx.session
        .write(request.kind.raw(), path, body.as_bytes(), 0o600)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::apply::{apply, AgentPaths};
    use crate::config::fileio::read_json_object;
    use crate::config::models::{AgentSettings, AppDocument};
    use serde_json::json;
    use std::collections::{BTreeMap, HashMap};
    use std::path::PathBuf;

    struct TestDir(PathBuf);
    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_mcp_only_{}_{}",
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

    fn base_doc(kind: AgentKind, mcp_ids: Vec<&str>, instruction_on: bool) -> AppDocument {
        let mut doc = AppDocument::default();
        doc.library.instructions = "Follow clean coding rules".into();
        doc.library.mcp = vec![
            MCPServer {
                id: "dbx".into(),
                transport: MCPTransport::Stdio,
                command: "npx".into(),
                args: vec!["-y".into(), "dbx-mcp".into()],
                url: String::new(),
                env: BTreeMap::from([("TOKEN".into(), "secret".into())]),
            },
            MCPServer {
                id: "remote".into(),
                transport: MCPTransport::Http,
                command: String::new(),
                args: Vec::new(),
                url: "https://mcp.example/mcp".into(),
                env: BTreeMap::new(),
            },
            MCPServer {
                id: "legacy-sse".into(),
                transport: MCPTransport::Sse,
                command: String::new(),
                args: Vec::new(),
                url: "https://mcp.example/sse".into(),
                env: BTreeMap::new(),
            },
        ];
        doc.set_settings(
            AgentSettings {
                instruction_on,
                mcp_ids: mcp_ids.into_iter().map(String::from).collect(),
                ..AgentSettings::default()
            },
            kind,
        );
        doc
    }

    #[test]
    fn antigravity_writes_mcp_and_agents_md_preserving_foreign_and_is_idempotent() {
        let dir = TestDir::new("agy");
        let home = dir.path();
        let cfg = home.join(".gemini/config/mcp_config.json");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(
            &cfg,
            serde_json::to_vec_pretty(&json!({
                "customTop": true,
                "mcpServers": {
                    "keep-foreign": { "command": "echo" },
                    "legacy-sse": { "url": "https://old/sse" }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let doc = base_doc(AgentKind::Antigravity, vec!["dbx", "remote"], true);
        let backups = home.join("backups");
        let first = apply(&doc, AgentKind::Antigravity, home, &HashMap::new(), &backups).unwrap();
        assert!(!first.written.is_empty());

        let root = read_json_object(&cfg);
        assert_eq!(root.get("customTop"), Some(&json!(true)));
        let servers = root["mcpServers"].as_object().unwrap();
        assert!(servers.contains_key("keep-foreign"));
        assert!(!servers.contains_key("legacy-sse"));
        assert_eq!(servers["dbx"]["command"], "npx");
        assert_eq!(servers["dbx"]["env"]["TOKEN"], "secret");
        assert_eq!(servers["remote"]["url"], "https://mcp.example/mcp");
        assert!(servers["dbx"].get("type").is_none());

        assert_eq!(
            std::fs::read_to_string(home.join(".gemini/config/AGENTS.md")).unwrap(),
            "Follow clean coding rules\n"
        );

        let second = apply(&doc, AgentKind::Antigravity, home, &HashMap::new(), &backups).unwrap();
        assert!(second.written.is_empty());
    }

    #[test]
    fn claude_codex_gemini_qwen_and_cursor_write_expected_formats() {
        let dir = TestDir::new("others");
        let home = dir.path();
        let backups = home.join("backups");
        let empty = HashMap::new();

        // Claude Code: preserves foreign keys in ~/.claude.json, adds type, writes ~/.claude/CLAUDE.md
        std::fs::write(
            home.join(".claude.json"),
            r#"{"numStartups": 5, "mcpServers": {"foreign": {"type": "stdio", "command": "ls"}}}"#,
        )
        .unwrap();
        let claude_doc = base_doc(AgentKind::Claude, vec!["dbx", "remote"], true);
        apply(&claude_doc, AgentKind::Claude, home, &empty, &backups).unwrap();
        let claude_root = read_json_object(&home.join(".claude.json"));
        assert_eq!(claude_root["numStartups"], json!(5));
        assert_eq!(claude_root["mcpServers"]["foreign"]["command"], "ls");
        assert_eq!(claude_root["mcpServers"]["dbx"]["type"], "stdio");
        assert_eq!(claude_root["mcpServers"]["remote"]["type"], "http");
        assert_eq!(
            std::fs::read_to_string(home.join(".claude/CLAUDE.md")).unwrap(),
            "Follow clean coding rules\n"
        );

        // Codex: TOML preserve & prune + AGENTS.md
        let codex_cfg = home.join(".codex/config.toml");
        std::fs::create_dir_all(codex_cfg.parent().unwrap()).unwrap();
        std::fs::write(
            &codex_cfg,
            "# header\nmodel = \"o3\"\n\n[mcp_servers.\"foreign\"]\ncommand = \"true\"\n\n[mcp_servers.\"legacy-sse\"]\nurl = \"old\"\n",
        )
        .unwrap();
        let codex_doc = base_doc(AgentKind::Codex, vec!["dbx", "remote"], true);
        apply(&codex_doc, AgentKind::Codex, home, &empty, &backups).unwrap();
        let codex_text = std::fs::read_to_string(&codex_cfg).unwrap();
        assert!(codex_text.contains("# header"));
        assert!(codex_text.contains("model = \"o3\""));
        assert!(codex_text.contains("[mcp_servers.\"foreign\"]"));
        assert!(!codex_text.contains("legacy-sse"));
        assert!(codex_text.contains("[mcp_servers.\"dbx\"]"));
        assert!(codex_text.contains("[mcp_servers.\"dbx\".env]"));
        assert!(codex_text.contains("TOKEN = \"secret\""));
        assert!(codex_text.contains("[mcp_servers.\"remote\"]"));
        assert_eq!(
            std::fs::read_to_string(home.join(".codex/AGENTS.md")).unwrap(),
            "Follow clean coding rules\n"
        );
        let second_codex = apply(&codex_doc, AgentKind::Codex, home, &empty, &backups).unwrap();
        assert!(second_codex.written.is_empty());

        // Gemini & Qwen: http uses httpUrl, sse uses url
        for (kind, cfg_rel, md_rel) in [
            (AgentKind::Gemini, ".gemini/settings.json", ".gemini/GEMINI.md"),
            (AgentKind::Qwen, ".qwen/settings.json", ".qwen/QWEN.md"),
        ] {
            let doc = base_doc(kind, vec!["remote", "legacy-sse"], true);
            apply(&doc, kind, home, &empty, &backups).unwrap();
            let root = read_json_object(&home.join(cfg_rel));
            assert_eq!(root["mcpServers"]["remote"]["httpUrl"], "https://mcp.example/mcp");
            assert_eq!(root["mcpServers"]["legacy-sse"]["url"], "https://mcp.example/sse");
            assert_eq!(
                std::fs::read_to_string(home.join(md_rel)).unwrap(),
                "Follow clean coding rules\n"
            );
        }

        // Cursor: mcp only, no instructions file even if instruction_on=true
        let cursor_doc = base_doc(AgentKind::Cursor, vec!["dbx"], true);
        apply(&cursor_doc, AgentKind::Cursor, home, &empty, &backups).unwrap();
        let cursor_root = read_json_object(&home.join(".cursor/mcp.json"));
        assert_eq!(cursor_root["mcpServers"]["dbx"]["command"], "npx");
        assert!(AgentPaths::live(AgentKind::Cursor, home, "", &empty)
            .instructions
            .as_os_str()
            .is_empty());
    }
}
