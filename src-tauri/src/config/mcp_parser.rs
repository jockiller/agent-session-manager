use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use super::models::{MCPServer, MCPTransport};

static SINGLE_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z0-9_-]+)\s*[:=]\s*(.+)$").unwrap());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    Unrecognized,
}

impl ParseError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Empty => "输入内容为空",
            Self::Unrecognized => "未能从输入中识别出有效的 MCP 配置（请粘贴 JSON 或 TOML 格式）",
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for ParseError {}

pub fn parse(raw: &str) -> Result<Vec<MCPServer>, ParseError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Some(servers) = parse_json(text) {
        if !servers.is_empty() {
            return Ok(servers);
        }
    }
    if let Some(servers) = parse_toml(text) {
        if !servers.is_empty() {
            return Ok(servers);
        }
    }
    if let Some(servers) = parse_single_line(text) {
        if !servers.is_empty() {
            return Ok(servers);
        }
    }
    Err(ParseError::Unrecognized)
}

fn parse_json(text: &str) -> Option<Vec<MCPServer>> {
    if !(text.starts_with('{')
        || text.contains("\"command\"")
        || text.contains("\"mcp_servers\"")
        || text.contains("\"mcpServers\"")
        || text.contains("\"mcp\""))
    {
        return None;
    }
    let object = decode_json_object(text)?;
    let root = nested(&object, &["mcp_servers", "mcpServers", "mcp"]).unwrap_or(object);
    Some(servers_from(&root))
}

fn decode_json_object(text: &str) -> Option<Map<String, Value>> {
    let attempts = [
        text.to_string(),
        format!("{{{text}"),
        format!("{{{text}}}"),
    ];
    for attempt in attempts {
        if let Ok(Value::Object(object)) = serde_json::from_str(&attempt) {
            return Some(object);
        }
    }
    None
}

fn nested(object: &Map<String, Value>, keys: &[&str]) -> Option<Map<String, Value>> {
    for key in keys {
        if let Some(Value::Object(value)) = object.get(*key) {
            if !value.is_empty() {
                return Some(value.clone());
            }
        }
    }
    None
}

fn servers_from(root: &Map<String, Value>) -> Vec<MCPServer> {
    let mut result = Vec::new();
    for (name, value) in root {
        let Value::Object(conf) = value else {
            continue;
        };
        let id = unquote(name).trim().to_string();
        if id.is_empty() {
            continue;
        }
        if let Some(server) = server_from(&id, conf) {
            result.push(server);
        }
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    result
}

fn server_from(id: &str, conf: &Map<String, Value>) -> Option<MCPServer> {
    let url = json_string(conf, "url");
    let type_name = json_string(conf, "type").to_lowercase();
    let mut command = string_list(conf.get("command"));
    command.extend(string_list(conf.get("args")));
    let env = string_map(conf.get("env"))
        .or_else(|| string_map(conf.get("environment")))
        .unwrap_or_default();

    let transport = if type_name == "sse" {
        MCPTransport::Sse
    } else if !url.is_empty() || type_name == "remote" || type_name == "http" {
        MCPTransport::Http
    } else {
        MCPTransport::Stdio
    };
    if transport == MCPTransport::Stdio && command.is_empty() {
        return None;
    }
    if transport != MCPTransport::Stdio && url.is_empty() {
        return None;
    }
    Some(MCPServer {
        id: id.to_string(),
        transport,
        command: command.first().cloned().unwrap_or_default(),
        args: command.into_iter().skip(1).collect(),
        url,
        env,
    })
}

fn parse_toml(text: &str) -> Option<Vec<MCPServer>> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut tables: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut env_tables: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut in_env = false;
    let mut saw_mcp_prefix = false;

    for raw_line in lines {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let header = &line[1..line.len() - 1];
            let (name, env) = section(header);
            if name.is_none() {
                current = None;
                in_env = false;
                continue;
            }
            if header.to_lowercase().starts_with("mcp") {
                saw_mcp_prefix = true;
            }
            current = name.clone();
            in_env = env;
            if let Some(name) = name {
                if env {
                    env_tables.entry(name).or_default();
                } else {
                    tables.entry(name).or_default();
                }
            }
            continue;
        }
        let Some(name) = current.as_ref() else {
            continue;
        };
        let Some(eq) = line.find('=') else {
            continue;
        };
        let key = line[..eq].trim().to_string();
        let value = line[eq + 1..].trim().to_string();
        if in_env {
            env_tables
                .entry(name.clone())
                .or_default()
                .insert(key, unquote(&value));
        } else {
            tables.entry(name.clone()).or_default().insert(key, value);
        }
    }

    if tables.is_empty() {
        return None;
    }
    if !saw_mcp_prefix
        && tables
            .keys()
            .any(|key| key.contains('.') || key.contains('/'))
    {
        return None;
    }

    let mut root = Map::new();
    for (name, fields) in tables {
        let mut conf = Map::new();
        if let Some(command) = fields.get("command") {
            let trimmed = command.trim();
            if trimmed.starts_with('[') {
                conf.insert("command".into(), json_string_array(parse_array(trimmed)));
            } else {
                conf.insert("command".into(), Value::String(unquote(trimmed)));
            }
        }
        if let Some(args) = fields.get("args") {
            conf.insert("args".into(), json_string_array(parse_array(args)));
        }
        if let Some(url) = fields.get("url") {
            conf.insert("url".into(), Value::String(unquote(url)));
        }
        if let Some(type_name) = fields.get("type") {
            conf.insert("type".into(), Value::String(unquote(type_name)));
        }
        let mut env = env_tables.remove(&name).unwrap_or_default();
        if let Some(inline) = fields.get("env") {
            if let Some(parsed) = decode_json_object(&unquote(inline)) {
                for (key, value) in parsed {
                    if let Value::String(text) = value {
                        env.insert(key, text);
                    }
                }
            }
        }
        if !env.is_empty() {
            let mut object = Map::new();
            for (key, value) in env {
                object.insert(key, Value::String(value));
            }
            conf.insert("env".into(), Value::Object(object));
        }
        root.insert(name, Value::Object(conf));
    }
    let servers = servers_from(&root);
    if servers.is_empty() {
        None
    } else {
        Some(servers)
    }
}

fn section(header: &str) -> (Option<String>, bool) {
    let prefixes = ["mcp_servers.", "mcpServers.", "mcp."];
    let lowered = header.to_lowercase();
    let mut rest = header.to_string();
    let mut matched = false;
    for prefix in prefixes {
        if lowered.starts_with(&prefix.to_lowercase()) {
            rest = header[prefix.len()..].to_string();
            matched = true;
            break;
        }
    }
    rest = unquote(rest.trim());
    if let Some(name) = rest.strip_suffix(".env") {
        return (if name.is_empty() { None } else { Some(name.to_string()) }, true);
    }
    if matched {
        return (if rest.is_empty() { None } else { Some(rest) }, false);
    }
    if rest.contains('.') || rest.contains('/') {
        return (None, false);
    }
    (if rest.is_empty() { None } else { Some(rest) }, false)
}

fn parse_single_line(text: &str) -> Option<Vec<MCPServer>> {
    let lines: Vec<&str> = text
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    if lines.len() != 1 {
        return None;
    }
    let line = lines[0];
    if line.starts_with('{') || line.starts_with('[') || line.contains('{') {
        return None;
    }
    if let Some(caps) = SINGLE_LINE.captures(line) {
        let name = caps.get(1).map(|m| m.as_str().to_string())?;
        let rest = caps.get(2).map(|m| m.as_str().trim().to_string())?;
        if rest.starts_with("http://") || rest.starts_with("https://") {
            return Some(vec![MCPServer {
                id: name,
                transport: MCPTransport::Http,
                url: rest,
                ..MCPServer::new("")
            }]);
        }
        let parts: Vec<&str> = rest.split_whitespace().collect();
        let command = parts.first()?;
        return Some(vec![MCPServer {
            id: name,
            command: (*command).to_string(),
            args: parts.iter().skip(1).map(|part| (*part).to_string()).collect(),
            ..MCPServer::new("")
        }]);
    }
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    Some(vec![MCPServer {
        id: parts[0].to_string(),
        command: parts[1].to_string(),
        args: parts.iter().skip(2).map(|part| (*part).to_string()).collect(),
        ..MCPServer::new("")
    }])
}

fn parse_array(raw: &str) -> Vec<String> {
    let mut value = raw.trim().to_string();
    if !(value.starts_with('[') && value.ends_with(']')) {
        return Vec::new();
    }
    value = value.replace('\'', "\"");
    let Ok(Value::Array(array)) = serde_json::from_str::<Value>(&value) else {
        return Vec::new();
    };
    array
        .into_iter()
        .filter_map(|item| match item {
            Value::String(text) => Some(text),
            _ => None,
        })
        .collect()
}

fn json_string_array(items: Vec<String>) -> Value {
    Value::Array(items.into_iter().map(Value::String).collect())
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(list)) => list
            .iter()
            .filter_map(|item| item.as_str())
            .filter(|text| !text.is_empty())
            .map(str::to_string)
            .collect(),
        Some(Value::String(text)) if !text.is_empty() => vec![text.clone()],
        _ => Vec::new(),
    }
}

fn string_map(value: Option<&Value>) -> Option<BTreeMap<String, String>> {
    let Value::Object(object) = value? else {
        return None;
    };
    let mut result = BTreeMap::new();
    for (key, raw) in object {
        if let Some(text) = value_to_string(raw) {
            result.insert(key.clone(), text);
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

fn value_to_string(raw: &Value) -> Option<String> {
    match raw {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn json_string(object: &Map<String, Value>, key: &str) -> String {
    match object.get(key) {
        Some(Value::String(text)) => text.clone(),
        _ => String::new(),
    }
}

fn unquote(raw: &str) -> String {
    let mut value = raw.trim().to_string();
    if value.len() >= 2 {
        if (value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\''))
        {
            value.remove(0);
            value.pop();
            value = value.replace("\\\"", "\"");
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::MCPTransport;

    #[test]
    fn parse_claude_mcp_servers_json() {
        let servers = parse(
            r#"
    {
      "mcpServers": {
        "nyaterm": {
          "command": "/Applications/NyaTerm.app/Contents/MacOS/nyaterm-mcp"
        },
        "dbx": {
          "command": "npx",
          "args": ["-y", "dbx"]
        }
      }
    }
    "#,
        )
        .unwrap();
        assert_eq!(
            servers.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["dbx", "nyaterm"]
        );
        let dbx = servers.iter().find(|s| s.id == "dbx").unwrap();
        assert_eq!(dbx.command, "npx");
        assert_eq!(dbx.args, ["-y", "dbx"]);
        assert_eq!(dbx.transport, MCPTransport::Stdio);
    }

    #[test]
    fn parse_open_code_local_command_array_and_env() {
        let servers = parse(
            r#"
    {
      "mcp": {
        "rea": {
          "type": "local",
          "command": ["/opt/rea", "mcp"],
          "environment": { "JAVA_HOME": "/tmp/java" }
        }
      }
    }
    "#,
        )
        .unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].id, "rea");
        assert_eq!(servers[0].command, "/opt/rea");
        assert_eq!(servers[0].args, ["mcp"]);
        assert_eq!(servers[0].env.get("JAVA_HOME").map(String::as_str), Some("/tmp/java"));
    }

    #[test]
    fn parse_missing_outer_braces() {
        let servers = parse(r#""dbx": { "command": "npx", "args": ["-y", "dbx"] }"#).unwrap();
        assert_eq!(
            servers.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["dbx"]
        );
    }

    #[test]
    fn parse_remote_url() {
        let servers =
            parse(r#"{ "mcpServers": { "search": { "url": "https://example.invalid/mcp" } } }"#)
                .unwrap();
        assert_eq!(servers[0].transport, MCPTransport::Http);
        assert_eq!(servers[0].url, "https://example.invalid/mcp");
    }

    #[test]
    fn parse_grok_toml() {
        let servers = parse(
            r#"
    [mcp_servers.dbx]
    command = "/usr/local/bin/dbx-mcp"
    args = ["--stdio"]

    [mcp_servers.dbx.env]
    TOKEN = "abc"
    "#,
        )
        .unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].id, "dbx");
        assert_eq!(servers[0].command, "/usr/local/bin/dbx-mcp");
        assert_eq!(servers[0].args, ["--stdio"]);
        assert_eq!(servers[0].env.get("TOKEN").map(String::as_str), Some("abc"));
    }

    #[test]
    fn parse_single_line_command() {
        let servers = parse("dbx /usr/local/bin/dbx-mcp --stdio").unwrap();
        assert_eq!(servers[0].id, "dbx");
        assert_eq!(servers[0].command, "/usr/local/bin/dbx-mcp");
        assert_eq!(servers[0].args, ["--stdio"]);
    }

    #[test]
    fn parse_empty_throws() {
        assert_eq!(parse("   ").unwrap_err(), ParseError::Empty);
        assert_eq!(ParseError::Empty.to_string(), "输入内容为空");
    }

    #[test]
    fn parse_unrecognized_throws() {
        assert_eq!(parse("not-mcp").unwrap_err(), ParseError::Unrecognized);
        assert_eq!(
            ParseError::Unrecognized.to_string(),
            "未能从输入中识别出有效的 MCP 配置（请粘贴 JSON 或 TOML 格式）"
        );
    }
}
