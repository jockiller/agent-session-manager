use std::collections::{HashMap, HashSet};

use crate::config::apply::{AgentPaths, ApplyContext, ApplyRequest};
use crate::config::endpoint;
use crate::config::fileio::{read_text, ApplyError};
use crate::config::models::{model_ref, MCPServer, MCPTransport, ModelEntry, Provider};

pub fn apply(
    request: &ApplyRequest,
    paths: &AgentPaths,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    let existing = read_text(&paths.config);
    let mut drop = model_headers_to_drop(&managed_model_keys(request), &existing);
    drop.extend(model_headers_matching_urls(&request.prune_urls(), &existing));
    let mut text = drop_tables(&existing, &drop);
    text = drop_tables(&text, &mcp_headers(&request.settings.mcp_ids, &text));

    let collisions = colliding_bare_ids(request);
    let mut blocks = Vec::new();
    for provider in &request.providers {
        let api_key = request.keys.get(&provider.id).cloned().unwrap_or_default();
        for model in &provider.models {
            let table_key = if collisions.contains(&model.id) {
                model_ref(&provider.id, &model.id)
            } else {
                model.id.clone()
            };
            blocks.push(model_table(&table_key, provider, model, &api_key));
        }
    }
    for server in &request.mcp {
        blocks.push(mcp_table(server));
    }

    // Upsert [models] on kept text before appending managed tables so a second
    // apply cannot move that section below the rewritten tables.
    let mut body = upsert_default(text.trim(), request, &collisions);
    if !blocks.is_empty() {
        body = body.trim_end().to_string();
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str(&blocks.join("\n\n"));
    }
    if !body.ends_with('\n') {
        body.push('\n');
    }

    ctx.session
        .write(request.kind.raw(), &paths.config, body.as_bytes(), 0o600)?;
    ctx.write_instructions(
        request.kind,
        &paths.instructions,
        &request.instructions,
        request.settings.instruction_on,
    )?;
    Ok(())
}

fn colliding_bare_ids(request: &ApplyRequest) -> HashSet<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for provider in &request.providers {
        for model in &provider.models {
            *seen.entry(model.id.clone()).or_insert(0) += 1;
        }
    }
    seen.into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(id, _)| id)
        .collect()
}

fn managed_model_keys(request: &ApplyRequest) -> HashSet<String> {
    let mut keys = HashSet::new();
    for provider in &request.providers {
        for model in &provider.models {
            keys.insert(model.id.clone());
            keys.insert(model_ref(&provider.id, &model.id));
        }
    }
    for provider in &request.retired {
        for model in &provider.models {
            keys.insert(model_ref(&provider.id, &model.id));
        }
    }
    keys
}

fn model_headers_to_drop(keys: &HashSet<String>, text: &str) -> HashSet<String> {
    let mut headers = HashSet::new();
    for line in text.split('\n') {
        let trimmed = line.trim();
        if !trimmed.starts_with("[model.") {
            continue;
        }
        if let Some(key) = table_key(trimmed, "[model.") {
            if keys.contains(&key) {
                headers.insert(trimmed.to_string());
            }
        }
    }
    headers
}

fn model_headers_matching_urls(urls: &HashSet<String>, text: &str) -> HashSet<String> {
    if urls.is_empty() {
        return HashSet::new();
    }
    let mut headers = HashSet::new();
    let mut current: Option<String> = None;
    for line in text.split('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current = if trimmed.starts_with("[model.") {
                Some(trimmed.to_string())
            } else {
                None
            };
            continue;
        }
        let Some(header) = current.as_ref() else {
            continue;
        };
        if !trimmed.starts_with("base_url") {
            continue;
        }
        if urls.contains(&endpoint::normalize(&toml_scalar(trimmed))) {
            headers.insert(header.clone());
        }
    }
    headers
}

fn toml_scalar(line: &str) -> String {
    let Some((_, raw)) = line.split_once('=') else {
        return String::new();
    };
    let mut value = raw.trim().to_string();
    if let Some(rest) = value.strip_prefix('"') {
        value = rest.to_string();
        if let Some(end) = value.rfind('"') {
            value.truncate(end);
        }
        value = value.replace("\\\"", "\"");
    }
    value
}

fn mcp_headers(names: &[String], text: &str) -> HashSet<String> {
    let wanted: HashSet<&str> = names.iter().map(String::as_str).collect();
    let mut headers = HashSet::new();
    for line in text.split('\n') {
        let trimmed = line.trim();
        if !trimmed.starts_with("[mcp_servers.") {
            continue;
        }
        if let Some(key) = table_key(trimmed, "[mcp_servers.") {
            let root = mcp_root_name(&key);
            if wanted.contains(key.as_str()) || wanted.contains(root.as_str()) {
                headers.insert(trimmed.to_string());
            }
        }
    }
    headers
}

fn mcp_root_name(key: &str) -> String {
    if let Some(rest) = key.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return rest[..end].to_string();
        }
    }
    key.split_once('.')
        .map(|(head, _)| head.to_string())
        .unwrap_or_else(|| key.to_string())
}

fn table_key(header: &str, prefix: &str) -> Option<String> {
    if !header.starts_with(prefix) || !header.ends_with(']') {
        return None;
    }
    let mut raw = header[prefix.len()..header.len() - 1].to_string();
    if raw.starts_with('"') && raw.ends_with('"') {
        raw.remove(0);
        raw.pop();
    }
    Some(raw)
}

fn drop_tables(text: &str, headers: &HashSet<String>) -> String {
    if headers.is_empty() {
        return text.to_string();
    }
    let mut lines = Vec::new();
    let mut skipping = false;
    for line in text.split('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            skipping = headers.contains(trimmed);
        }
        if skipping {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n")
}

fn model_table(key: &str, provider: &Provider, model: &ModelEntry, api_key: &str) -> String {
    let mut lines = vec![format!("[model.{}]", toml_key(key))];
    lines.push(format!("model = {}", toml_string(model.wire_id())));
    lines.push(format!("name = {}", toml_string(&model.name)));
    if !provider.base_url.is_empty() {
        lines.push(format!(
            "base_url = {}",
            toml_string(&endpoint::for_provider(&provider.base_url, provider.api))
        ));
    }
    if !api_key.is_empty() {
        lines.push(format!("api_key = {}", toml_string(api_key)));
    }
    lines.push(format!(
        "api_backend = {}",
        toml_string(provider.api.grok_backend())
    ));
    if let Some(context) = model.context {
        lines.push(format!("context_window = {context}"));
    }
    if let Some(efforts) = &model.efforts {
        if !efforts.is_empty() {
            let items: Vec<String> = efforts.iter().map(|item| toml_string(item)).collect();
            lines.push(format!("reasoning_efforts = [{}]", items.join(", ")));
        }
    }
    lines.join("\n")
}

fn mcp_table(server: &MCPServer) -> String {
    let mut lines = vec![format!("[mcp_servers.{}]", toml_key(&server.id))];
    if server.transport == MCPTransport::Stdio {
        lines.push(format!("command = {}", toml_string(&server.command)));
        if !server.args.is_empty() {
            let items: Vec<String> = server.args.iter().map(|item| toml_string(item)).collect();
            lines.push(format!("args = [{}]", items.join(", ")));
        }
    } else {
        lines.push(format!("url = {}", toml_string(&server.url)));
    }
    if !server.env.is_empty() {
        lines.push(String::new());
        lines.push(format!("[mcp_servers.{}.env]", toml_key(&server.id)));
        for (key, value) in &server.env {
            lines.push(format!("{key} = {}", toml_string(value)));
        }
    }
    lines.join("\n")
}

fn upsert_default(text: &str, request: &ApplyRequest, collisions: &HashSet<String>) -> String {
    let mut default_key = request.default_model_base();
    if default_key.contains('/') {
        if let Some((provider_id, model_id)) = default_key.split_once('/') {
            if !collisions.contains(model_id) {
                let _ = provider_id;
                default_key = model_id.to_string();
            }
        }
    } else if collisions.contains(&default_key) {
        if let Some(provider) = request.providers.iter().find(|provider| {
            provider.models.iter().any(|model| model.id == default_key)
        }) {
            default_key = model_ref(&provider.id, &default_key);
        }
    }
    let effort = request.settings.effort.as_str();
    let mut lines: Vec<String> = if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').map(str::to_string).collect()
    };
    let mut in_models = false;
    let mut saw_models = false;
    let mut saw_default = false;
    let mut saw_effort = false;
    for line in lines.iter_mut() {
        let trimmed = line.trim().to_string();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_models = trimmed == "[models]";
            if in_models {
                saw_models = true;
            }
        } else if in_models && trimmed.starts_with("default =") {
            if !default_key.is_empty() {
                *line = format!("default = {}", toml_string(&default_key));
                saw_default = true;
            } else if request.references_retired_model(&toml_scalar(&trimmed)) {
                *line = String::new();
            } else {
                saw_default = true;
            }
        } else if in_models && trimmed.starts_with("default_reasoning_effort =") && !effort.is_empty()
        {
            *line = format!("default_reasoning_effort = {}", toml_string(effort));
            saw_effort = true;
        }
    }
    if !saw_models && (!default_key.is_empty() || !effort.is_empty()) {
        let mut extra = vec!["[models]".to_string()];
        if !default_key.is_empty() {
            extra.push(format!("default = {}", toml_string(&default_key)));
        }
        if !effort.is_empty() {
            extra.push(format!(
                "default_reasoning_effort = {}",
                toml_string(effort)
            ));
        }
        if !lines.is_empty() && !lines.last().unwrap().is_empty() {
            lines.push(String::new());
        }
        lines.extend(extra);
    } else if saw_models {
        if let Some(insert_at) = lines.iter().rposition(|line| line.trim() == "[models]") {
            let mut offset = 1;
            if !saw_default && !default_key.is_empty() {
                lines.insert(
                    insert_at + offset,
                    format!("default = {}", toml_string(&default_key)),
                );
                offset += 1;
            }
            if !saw_effort && !effort.is_empty() {
                lines.insert(
                    insert_at + offset,
                    format!("default_reasoning_effort = {}", toml_string(effort)),
                );
            }
        }
    }
    lines.join("\n")
}

fn toml_key(raw: &str) -> String {
    format!("\"{}\"", raw.replace('"', "\\\""))
}

fn toml_string(raw: &str) -> String {
    let escaped = raw.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use crate::config::apply::{self, AgentPaths};
    use crate::config::models::{
        APIKind, AgentKind, AgentSettings, AppDocument, MCPServer, MCPTransport, ModelEntry,
        Provider,
    };
    use std::collections::{BTreeMap, HashMap};
    use std::path::{Path, PathBuf};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_grok_{}_{}",
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

    fn apply_grok(document: &AppDocument, home: &Path) -> apply::ApplyOutcome {
        apply::apply(
            document,
            AgentKind::Grok,
            home,
            &HashMap::new(),
            &home.join("asm-backups"),
        )
        .unwrap()
    }

    fn grok_paths(home: &Path) -> AgentPaths {
        AgentPaths::live(AgentKind::Grok, home, "", &HashMap::new())
    }

    fn write_config(paths: &AgentPaths, text: &str) {
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        std::fs::write(&paths.config, text).unwrap();
    }

    fn two_provider_document() -> AppDocument {
        let mut document = AppDocument::default();
        document.providers = vec![
            Provider {
                id: "a".into(),
                name: "A".into(),
                base_url: "https://a.example/v1".into(),
                api: APIKind::Responses,
                catalog_url: String::new(),
                api_key: "sk-a".into(),
                compat_base_url: String::new(),
                models: vec![ModelEntry::new("m1", "M1")],
            },
            Provider {
                id: "b".into(),
                name: "B".into(),
                base_url: "https://b.example/v1".into(),
                api: APIKind::Responses,
                catalog_url: String::new(),
                api_key: "sk-b".into(),
                compat_base_url: String::new(),
                models: vec![ModelEntry::new("m2", "M2")],
            },
        ];
        document.set_settings(
            AgentSettings {
                model: "a/m1".into(),
                picked_model_ids: vec!["a/m1".into()],
                ..AgentSettings::default()
            },
            AgentKind::Grok,
        );
        document
    }

    #[test]
    fn grok_apply_writes_model_table() {
        let dir = TestDir::new("model_table");
        let home = dir.path();
        let mut document = AppDocument::default();
        let mut model = ModelEntry::new("flash", "Flash");
        model.context = Some(128_000);
        model.efforts = Some(vec!["low".into(), "high".into()]);
        document.providers = vec![Provider {
            id: "g".into(),
            name: "GJ".into(),
            base_url: "https://example.invalid/v1".into(),
            api: APIKind::Responses,
            catalog_url: String::new(),
            api_key: "sk-test".into(),
            compat_base_url: String::new(),
            models: vec![model],
        }];
        document.set_settings(
            AgentSettings {
                model: "g/flash".into(),
                effort: "high".into(),
                ..AgentSettings::default()
            },
            AgentKind::Grok,
        );
        apply_grok(&document, home);
        let text = std::fs::read_to_string(grok_paths(home).config).unwrap();
        assert!(text.contains("[model.\"flash\"]"));
        assert!(text.contains("api_backend = \"chat_completions\""));
        assert!(text.contains("default = \"flash\""));
        assert!(text.contains("context_window = 128000"));
        assert!(text.contains("reasoning_efforts = [\"low\", \"high\"]"));
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn grok_apply_writes_mcp_env_and_drops_nested_tables() {
        let dir = TestDir::new("mcp_env");
        let home = dir.path();
        let paths = grok_paths(home);
        write_config(
            &paths,
            r#"[mcp_servers."dbx"]
command = "old-dbx"

[mcp_servers."dbx".env]
TOKEN = "old"
"#,
        );

        let mut document = AppDocument::default();
        document.library.mcp = vec![MCPServer {
            id: "dbx".into(),
            transport: MCPTransport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "dbx".into()],
            url: String::new(),
            env: BTreeMap::from([("TOKEN".into(), "abc".into())]),
        }];
        document.set_settings(
            AgentSettings {
                mcp_ids: vec!["dbx".into()],
                ..AgentSettings::default()
            },
            AgentKind::Grok,
        );
        apply_grok(&document, home);
        let text = std::fs::read_to_string(paths.config).unwrap();
        assert!(text.contains("[mcp_servers.\"dbx\"]"));
        assert!(text.contains("command = \"npx\""));
        assert!(text.contains("[mcp_servers.\"dbx\".env]"));
        assert!(text.contains("TOKEN = \"abc\""));
        assert!(!text.contains("old-dbx"));
        assert!(!text.contains("TOKEN = \"old\""));
    }

    #[test]
    fn grok_apply_removes_same_url_models() {
        let dir = TestDir::new("same_url");
        let home = dir.path();
        let paths = grok_paths(home);
        write_config(
            &paths,
            r#"[model."old-flash"]
model = "flash"
base_url = "https://example.invalid/v1"
api_key = "sk-old"
api_backend = "chat_completions"

[model."keep"]
model = "keep"
base_url = "https://other.invalid/v1"
api_backend = "chat_completions"
"#,
        );

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
        apply_grok(&document, home);
        let text = std::fs::read_to_string(paths.config).unwrap();
        assert!(!text.contains("[model.\"old-flash\"]"));
        assert!(text.contains("[model.\"keep\"]"));
        assert!(text.contains("[model.\"flash\"]"));
    }

    #[test]
    fn grok_apply_normalizes_pasted_anthropic_endpoint() {
        let dir = TestDir::new("endpoint");
        let home = dir.path();
        for pasted in [
            "https://relay.invalid/v1/messages",
            "https://relay.invalid/v1",
        ] {
            let mut document = AppDocument::default();
            let mut model = ModelEntry::new("opus", "Opus");
            model.efforts = Some(vec!["low".into(), "high".into()]);
            document.providers = vec![Provider {
                id: "claude".into(),
                name: "Claude".into(),
                base_url: pasted.into(),
                api: APIKind::Anthropic,
                catalog_url: String::new(),
                api_key: "sk-relay".into(),
                compat_base_url: String::new(),
                models: vec![model],
            }];
            document.set_settings(
                AgentSettings {
                    model: "claude/opus".into(),
                    effort: "high".into(),
                    ..AgentSettings::default()
                },
                AgentKind::Grok,
            );
            apply_grok(&document, home);
            let text = std::fs::read_to_string(grok_paths(home).config).unwrap();
            assert!(
                text.contains("base_url = \"https://relay.invalid\""),
                "{pasted}: {text}"
            );
            assert!(!text.contains("relay.invalid/v1"), "{pasted}: {text}");
        }
    }

    #[test]
    fn grok_drops_default_model_of_retired_provider() {
        let dir = TestDir::new("retired");
        let home = dir.path();
        let paths = grok_paths(home);
        write_config(
            &paths,
            r#"[models]
default = "b/m2"
default_reasoning_effort = "high"

[model.b-m2]
base_url = "https://b.example/v1"
name = "M2"
"#,
        );
        apply_grok(&two_provider_document(), home);
        let text = std::fs::read_to_string(paths.config).unwrap();
        assert!(!text.contains("b/m2"));
        assert!(text.contains("default = \"m1\""));
    }

    #[test]
    fn grok_apply_twice_is_idempotent_and_keeps_foreign_content() {
        let dir = TestDir::new("idempotent");
        let home = dir.path();
        let paths = grok_paths(home);
        write_config(
            &paths,
            r#"# user comment at top
[other]
foo = 1
bar = "keep"

[model."keep"]
model = "keep"
name = "Keep"
base_url = "https://other.invalid/v1"
api_backend = "chat_completions"

[mcp_servers."user-tool"]
command = "echo"

[mcp_servers."user-tool".env]
HOME = "/tmp"

[custom]
enabled = true
"#,
        );

        let mut document = AppDocument::default();
        let mut model = ModelEntry::new("flash", "Flash");
        model.context = Some(128_000);
        model.efforts = Some(vec!["low".into(), "high".into()]);
        document.providers = vec![Provider {
            id: "g".into(),
            name: "GJ".into(),
            base_url: "https://example.invalid/v1".into(),
            api: APIKind::Responses,
            catalog_url: String::new(),
            api_key: "sk-test".into(),
            compat_base_url: String::new(),
            models: vec![model],
        }];
        document.library.mcp = vec![MCPServer {
            id: "dbx".into(),
            transport: MCPTransport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "dbx".into()],
            url: String::new(),
            env: BTreeMap::from([("TOKEN".into(), "abc".into())]),
        }];
        document.library.instructions = "keep diffs small".into();
        document.set_settings(
            AgentSettings {
                model: "g/flash".into(),
                effort: "high".into(),
                instruction_on: true,
                mcp_ids: vec!["dbx".into()],
                ..AgentSettings::default()
            },
            AgentKind::Grok,
        );

        let first = apply_grok(&document, home);
        assert!(!first.written.is_empty());
        let after_first = std::fs::read_to_string(&paths.config).unwrap();
        assert!(after_first.contains("# user comment at top"));
        assert!(after_first.contains("[other]"));
        assert!(after_first.contains("bar = \"keep\""));
        assert!(after_first.contains("[model.\"keep\"]"));
        assert!(after_first.contains("[mcp_servers.\"user-tool\"]"));
        assert!(after_first.contains("HOME = \"/tmp\""));
        assert!(after_first.contains("[custom]"));
        assert!(after_first.contains("enabled = true"));
        assert!(after_first.contains("[model.\"flash\"]"));
        assert!(after_first.contains("[mcp_servers.\"dbx\"]"));
        assert!(!after_first.contains("\n\n\n"), "{after_first}");

        let second = apply_grok(&document, home);
        assert!(second.written.is_empty(), "{second:?}");
        let after_second = std::fs::read_to_string(&paths.config).unwrap();
        assert_eq!(after_first, after_second);
        assert!(!after_second.contains("\n\n\n"));
    }
}
