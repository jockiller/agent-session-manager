use std::collections::HashSet;
use std::path::Path;

use crate::config::apply::{AgentPaths, ApplyContext, ApplyRequest};
use crate::config::endpoint;
use crate::config::fileio::{read_text, ApplyError};
use crate::config::models::{APIKind, EffortLevel, MCPServer, MCPTransport, ModelEntry, Provider};

use super::pi;

pub fn discovered(home: &Path) -> Vec<String> {
    let directory = home.join(".dsh/profiles");
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().join("cordis.patch.yml").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

pub fn env_name(provider_id: &str) -> String {
    let mapped: String = provider_id
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect();
    let safe = mapped.trim_matches('_').to_uppercase();
    format!("{}_API_KEY", if safe.is_empty() { "PROVIDER" } else { &safe })
}

pub fn apply(request: &ApplyRequest, paths: &AgentPaths, ctx: &mut ApplyContext<'_>) -> Result<(), ApplyError> {
    let Some(dsh_root) = paths.instructions.parent() else {
        return Err(ApplyError::NotInstalled("DSH".into()));
    };
    if !dsh_root.exists() {
        return Err(ApplyError::NotInstalled("DSH".into()));
    }
    ctx.write_instructions(
        request.kind,
        &paths.instructions,
        &request.instructions,
        request.settings.instruction_on,
    )?;
    write_patch(request, &paths.config, ctx)?;
    if let Some(env) = &paths.extra {
        write_env(request, env, ctx)?;
    }
    Ok(())
}

fn write_patch(request: &ApplyRequest, path: &Path, ctx: &mut ApplyContext<'_>) -> Result<(), ApplyError> {
    let existing = read_text(path);
    let mut items = split_top_level(&existing);
    let leftover = leftover_providers(&items, request);
    items.retain(|item| {
        !is_plugin(item, "llm-pi-ai") && !is_default_model(item) && !is_our_mcp_insert(item)
    });
    items.insert(0, llm_pi_ai_item(request, &leftover));
    if !request.default_bare_model_id().is_empty() {
        items.insert(1, default_model_item(request));
    }
    items.extend(mcp_items(&request.mcp));
    let mut next = items.join("\n");
    if !next.ends_with('\n') {
        next.push('\n');
    }
    ctx.session
        .write(request.kind.raw(), path, next.as_bytes(), 0o644)?;
    Ok(())
}

fn write_env(request: &ApplyRequest, path: &Path, ctx: &mut ApplyContext<'_>) -> Result<(), ApplyError> {
    let existing = read_text(path);
    let mut lines: Vec<String> = if existing.is_empty() {
        Vec::new()
    } else {
        existing.split('\n').map(String::from).collect()
    };
    let mut changed = false;
    for provider in &request.providers {
        let key = request
            .keys
            .get(&provider.id)
            .cloned()
            .unwrap_or_else(|| provider.api_key.clone());
        if key.is_empty() {
            continue;
        }
        let name = env_name(&provider.id);
        let rendered = env_value(&key);
        let mut found = false;
        for line in &mut lines {
            let trimmed = line.trim();
            if trimmed.starts_with(&format!("{name}=")) || trimmed.starts_with(&format!("export {name}="))
            {
                let prefix = if trimmed.starts_with("export ") {
                    "export "
                } else {
                    ""
                };
                let next = format!("{prefix}{name}={rendered}");
                if *line != next {
                    *line = next;
                    changed = true;
                }
                found = true;
                break;
            }
        }
        if !found {
            lines.push(format!("{name}={rendered}"));
            changed = true;
        }
    }
    if !changed {
        return Ok(());
    }
    let mut body = lines.join("\n");
    if !body.ends_with('\n') {
        body.push('\n');
    }
    ctx.session
        .write(request.kind.raw(), path, body.as_bytes(), 0o600)?;
    Ok(())
}

fn leftover_providers(items: &[String], request: &ApplyRequest) -> String {
    let Some(llm) = items.iter().find(|item| is_plugin(item, "llm-pi-ai")) else {
        return String::new();
    };
    let urls = request.prune_urls();
    let ours: HashSet<&str> = request.providers.iter().map(|provider| provider.id.as_str()).collect();
    let retired = request.retired_ids();
    let mut kept: Vec<String> = Vec::new();
    for block in provider_blocks(llm) {
        if ours.contains(block.id.as_str()) || retired.contains(&block.id) {
            continue;
        }
        if !block.url.is_empty() && urls.contains(&block.url) {
            continue;
        }
        kept.push(block.raw);
    }
    kept.join("\n")
}

fn llm_pi_ai_item(request: &ApplyRequest, leftover: &str) -> String {
    let mut lines = vec![
        "- id: llm-pi-ai".to_string(),
        "  config:".to_string(),
        "    providers:".to_string(),
    ];
    if !leftover.is_empty() {
        lines.push(leftover.to_string());
    }
    for provider in &request.providers {
        let key = request
            .keys
            .get(&provider.id)
            .cloned()
            .unwrap_or_else(|| provider.api_key.clone());
        lines.extend(provider_block(provider, &key));
    }
    lines.join("\n")
}

fn provider_block(provider: &Provider, key: &str) -> Vec<String> {
    let mut lines = vec![format!("      {}:", yaml_key(&provider.id))];
    let display = if provider.name.is_empty() {
        provider.id.as_str()
    } else {
        provider.name.as_str()
    };
    lines.push(format!("        displayName: {}", yaml_string(display)));
    if !key.is_empty() {
        lines.push(format!(
            "        apiKeyEnv: {}",
            yaml_string(&env_name(&provider.id))
        ));
    }
    lines.push(format!("        api: {}", provider.api.dsh_api()));
    if !provider.base_url.is_empty() {
        lines.push(format!(
            "        baseURL: {}",
            yaml_string(&endpoint::for_provider(&provider.base_url, provider.api))
        ));
    }
    lines.push("        reasoning: high".to_string());
    if !provider.models.is_empty() {
        lines.push("        models:".to_string());
        for model in &provider.models {
            lines.extend(model_lines(model, provider.api));
        }
    }
    lines
}

fn model_lines(model: &ModelEntry, api: APIKind) -> Vec<String> {
    let mut lines = vec![format!("          - id: {}", yaml_string(model.wire_id()))];
    lines.push(format!("            name: {}", yaml_string(&model.name)));
    if let Some(context) = model.context {
        lines.push(format!("            contextWindow: {context}"));
    }
    if let Some(max) = max_tokens(model.context, model.output) {
        lines.push(format!("            maxTokens: {max}"));
    }
    let inputs: Vec<&str> = if model.input.is_empty() {
        vec!["text"]
    } else {
        model
            .input
            .iter()
            .map(String::as_str)
            .filter(|item| *item == "text" || *item == "image")
            .collect()
    };
    if !inputs.is_empty() {
        lines.push("            input:".to_string());
        for item in inputs {
            lines.push(format!("              - {item}"));
        }
    }
    if let Some(efforts) = &model.efforts {
        if efforts.is_empty() {
            lines.push("            reasoningEfforts: false".to_string());
        } else {
            lines.push("            reasoningEfforts:".to_string());
            for level in EffortLevel::ALL {
                if efforts.iter().any(|item| item == level.raw()) {
                    lines.push(format!("              {}: {}", level.raw(), level.raw()));
                }
            }
        }
    }
    if force_adaptive_thinking(api, model) {
        lines.push("            compat:".to_string());
        lines.push("              forceAdaptiveThinking: true".to_string());
    }
    lines
}

fn force_adaptive_thinking(api: APIKind, model: &ModelEntry) -> bool {
    let reasons = match &model.efforts {
        None => true,
        Some(efforts) => !efforts.is_empty(),
    };
    reasons && api == APIKind::Anthropic && pi::is_adaptive_thinking(model.wire_id())
}

fn default_model_item(request: &ApplyRequest) -> String {
    let mut lines = vec![
        "- id: agent-default-model".to_string(),
        "  name: \"@deepseek-ai/dsh-agent-default-model\"".to_string(),
        "  config:".to_string(),
        format!("    provider: {}", yaml_string(&request.default_provider_id())),
        format!("    model: {}", yaml_string(&request.default_bare_model_id())),
    ];
    let effort = request.settings.effort.trim();
    if !effort.is_empty() && effort != "off" && effort != "none" {
        lines.push(format!("    reasoningEffort: {effort}"));
    }
    lines.join("\n")
}

fn mcp_items(servers: &[MCPServer]) -> Vec<String> {
    servers
        .iter()
        .filter(|server| server.transport == MCPTransport::Stdio && !server.command.is_empty())
        .map(mcp_item)
        .collect()
}

fn mcp_item(server: &MCPServer) -> String {
    let id = sanitize_mcp(&server.id);
    let mut lines = vec![
        "- insert:".to_string(),
        format!("  - id: mcp-{id}"),
        "    name: dsh-mcp-client".to_string(),
        "    config:".to_string(),
        format!("      serverName: {id}"),
        "      transport: stdio".to_string(),
        format!("      command: {}", yaml_string(&server.command)),
    ];
    if server.args.is_empty() {
        lines.push("      args: []".to_string());
    } else {
        lines.push("      args:".to_string());
        for arg in &server.args {
            lines.push(format!("        - {}", yaml_string(arg)));
        }
    }
    lines.join("\n")
}

fn split_top_level(text: &str) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for line in text.split('\n') {
        if line.starts_with("- ") {
            if !current.is_empty() {
                items.push(current.join("\n"));
            }
            current = vec![line.to_string()];
        } else if !current.is_empty() {
            current.push(line.to_string());
        }
    }
    if !current.is_empty() {
        items.push(current.join("\n"));
    }
    items
        .into_iter()
        .filter(|item| !item.trim().is_empty())
        .collect()
}

fn is_plugin(item: &str, id: &str) -> bool {
    item.contains(&format!("id: {id}")) && !item.contains("insert:")
}

fn is_default_model(item: &str) -> bool {
    is_plugin(item, "agent-default-model")
}

/// A generated MCP entry is a top-level `- insert:` whose only child has
/// `name: dsh-mcp-client` and an id starting with `mcp-`. Any other shape stays.
fn is_our_mcp_insert(item: &str) -> bool {
    let lines: Vec<&str> = item.lines().collect();
    let Some(first) = lines.first() else {
        return false;
    };
    if first.trim_end() != "- insert:" {
        return false;
    }
    let mut children: Vec<Vec<&str>> = Vec::new();
    for line in &lines[1..] {
        if line.starts_with("  - ") {
            children.push(vec![*line]);
        } else if let Some(child) = children.last_mut() {
            child.push(*line);
        }
    }
    if children.len() != 1 {
        return false;
    }
    let mut id = String::new();
    let mut name = String::new();
    for (index, line) in children[0].iter().enumerate() {
        let indent = line.chars().take_while(|character| *character == ' ').count();
        let body = if index == 0 {
            line.trim().trim_start_matches("- ").trim()
        } else if indent == 4 {
            line.trim()
        } else {
            continue;
        };
        if let Some(rest) = body.strip_prefix("id:") {
            id = unquote_simple(rest.trim());
        } else if let Some(rest) = body.strip_prefix("name:") {
            name = unquote_simple(rest.trim());
        }
    }
    name == "dsh-mcp-client" && id.starts_with("mcp-")
}

struct ProviderSlice {
    id: String,
    url: String,
    raw: String,
}

fn provider_blocks(llm_item: &str) -> Vec<ProviderSlice> {
    let lines: Vec<&str> = llm_item.split('\n').collect();
    let mut blocks: Vec<ProviderSlice> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut current_id = String::new();
    let mut in_providers = false;

    let flush = |blocks: &mut Vec<ProviderSlice>, current: &mut Vec<String>, current_id: &mut String| {
        if current.is_empty() || current_id.is_empty() {
            return;
        }
        let raw = current.join("\n");
        let mut url = String::new();
        for line in current.iter() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("baseURL:") {
                let value = rest.trim().trim_matches(|character| character == '"' || character == '\'');
                url = endpoint::normalize(value);
            }
        }
        blocks.push(ProviderSlice {
            id: current_id.clone(),
            url,
            raw,
        });
        current.clear();
        current_id.clear();
    };

    for line in lines {
        if line.starts_with("    providers:") {
            in_providers = true;
            continue;
        }
        if !in_providers {
            continue;
        }
        if line.starts_with("      ") && !line.starts_with("       ") && line.contains(':') {
            flush(&mut blocks, &mut current, &mut current_id);
            current_id = line
                .trim()
                .split_once(':')
                .map(|(head, _)| head.to_string())
                .unwrap_or_default();
            current = vec![line.to_string()];
        } else if !current.is_empty() {
            current.push(line.to_string());
        }
    }
    flush(&mut blocks, &mut current, &mut current_id);
    blocks
}

fn max_tokens(context: Option<i64>, output: Option<i64>) -> Option<i64> {
    let output = output?;
    let Some(context) = context else {
        return Some(output);
    };
    if context <= 0 {
        return Some(output);
    }
    let budget = (context as f64 * 0.9) as i64;
    Some(output.min(budget.max(1)))
}

fn sanitize_mcp(raw: &str) -> String {
    raw.chars()
        .map(|character| {
            if character.is_alphabetic() || character.is_numeric() || character == '_' || character == '-'
            {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn yaml_key(raw: &str) -> String {
    if raw
        .chars()
        .all(|character| character.is_alphanumeric() || character == '_' || character == '-')
    {
        raw.to_string()
    } else {
        yaml_string(raw)
    }
}

fn yaml_string(raw: &str) -> String {
    let plain = !raw.is_empty()
        && raw.chars().all(|character| {
            character.is_alphanumeric() || matches!(character, '-' | '_' | '.' | '/' | ':' | '@')
        });
    if plain {
        return raw.to_string();
    }
    let escaped = raw.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn env_value(raw: &str) -> String {
    let plain = raw
        .chars()
        .all(|character| !character.is_whitespace() && character != '#');
    if plain {
        return raw.to_string();
    }
    let escaped = raw.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn unquote_simple(raw: &str) -> String {
    if raw.len() >= 2
        && ((raw.starts_with('"') && raw.ends_with('"')) || (raw.starts_with('\'') && raw.ends_with('\'')))
    {
        raw[1..raw.len() - 1].to_string()
    } else {
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::apply::{apply, AgentPaths};
    use crate::config::models::{
        APIKind, AgentKind, AgentSettings, AppDocument, MCPServer, MCPTransport, ModelEntry, Provider,
    };
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_dsh_{}_{}",
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

    fn apply_doc(
        document: &AppDocument,
        home: &Path,
    ) -> Result<crate::config::apply::ApplyOutcome, ApplyError> {
        apply(
            document,
            AgentKind::Dsh,
            home,
            &HashMap::new(),
            &home.join("_backups"),
        )
    }

    fn sample_provider() -> Provider {
        let mut model = ModelEntry::new("flash", "Flash");
        model.context = Some(200_000);
        model.output = Some(64_000);
        model.input = vec!["text".into(), "image".into(), "pdf".into()];
        model.efforts = Some(vec!["low".into(), "high".into()]);
        Provider {
            id: "g".into(),
            name: "GJ".into(),
            base_url: "https://example.invalid/v1".into(),
            api: APIKind::Responses,
            catalog_url: String::new(),
            api_key: "sk-test".into(),
            compat_base_url: String::new(),
            models: vec![model],
        }
    }

    #[test]
    fn dsh_env_name_and_discovered() {
        assert_eq!(env_name("g"), "G_API_KEY");
        assert_eq!(env_name("g-ja"), "G_JA_API_KEY");
        assert_eq!(env_name("..."), "PROVIDER_API_KEY");

        let dir = TestDir::new("discovered");
        let home = dir.path();
        assert!(discovered(home).is_empty());
        std::fs::create_dir_all(home.join(".dsh/profiles/coding")).unwrap();
        std::fs::write(home.join(".dsh/profiles/coding/cordis.patch.yml"), "x\n").unwrap();
        std::fs::create_dir_all(home.join(".dsh/profiles/empty")).unwrap();
        std::fs::create_dir_all(home.join(".dsh/profiles/web")).unwrap();
        std::fs::write(home.join(".dsh/profiles/web/cordis.patch.yml"), "y\n").unwrap();
        assert_eq!(discovered(home), ["coding", "web"]);
    }

    #[test]
    fn dsh_apply_not_installed() {
        let dir = TestDir::new("missing");
        let err = apply_doc(&AppDocument::default(), dir.path()).unwrap_err();
        assert_eq!(err, ApplyError::NotInstalled("DSH".into()));
        assert_eq!(err.to_string(), "DSH 未安装，已跳过");
    }

    #[test]
    fn dsh_apply_writes_chosen_profile_and_providers() {
        let dir = TestDir::new("writes");
        let home = dir.path();
        std::fs::create_dir_all(home.join(".dsh")).unwrap();
        let mut document = AppDocument::default();
        document.providers = vec![sample_provider()];
        document.set_settings(
            AgentSettings {
                model: "g/flash".into(),
                effort: "high".into(),
                profile: "coding".into(),
                ..AgentSettings::default()
            },
            AgentKind::Dsh,
        );
        apply_doc(&document, home).unwrap();

        let paths = AgentPaths::live(AgentKind::Dsh, home, "coding", &HashMap::new());
        assert!(paths
            .config
            .to_string_lossy()
            .ends_with("/.dsh/profiles/coding/cordis.patch.yml"));
        let text = std::fs::read_to_string(&paths.config).unwrap();
        assert!(text.contains("id: llm-pi-ai"));
        assert!(text.contains("displayName: GJ"));
        assert!(text.contains("api: openai-responses"));
        assert!(text.contains("baseURL: https://example.invalid/v1"));
        assert!(text.contains("id: flash"));
        assert!(text.contains("contextWindow: 200000"));
        assert!(text.contains("id: agent-default-model"));
        assert!(text.contains("provider: g"));
        assert!(text.contains("reasoningEffort: high"));
        assert!(!text.contains("pdf"));
        let env = std::fs::read_to_string(paths.extra.as_ref().unwrap()).unwrap();
        assert!(env.contains("G_API_KEY=sk-test"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let patch_mode = std::fs::metadata(&paths.config).unwrap().permissions().mode() & 0o777;
            let env_mode = std::fs::metadata(paths.extra.as_ref().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(patch_mode, 0o644);
            assert_eq!(env_mode, 0o600);
        }
    }

    #[test]
    fn dsh_ignores_effort_by_model() {
        let dir = TestDir::new("ignore_effort_by_model");
        let home = dir.path();
        std::fs::create_dir_all(home.join(".dsh")).unwrap();
        let mut document = AppDocument::default();
        document.providers = vec![sample_provider()];
        let mut settings = AgentSettings {
            model: "g/flash".into(),
            effort: "high".into(),
            profile: "coding".into(),
            ..AgentSettings::default()
        };
        settings
            .effort_by_model
            .insert("g/flash".into(), "low".into());
        document.set_settings(settings, AgentKind::Dsh);
        apply_doc(&document, home).unwrap();
        let paths = AgentPaths::live(AgentKind::Dsh, home, "coding", &HashMap::new());
        let text = std::fs::read_to_string(&paths.config).unwrap();
        assert!(text.contains("reasoningEffort: high"), "{text}");
        assert!(!text.contains("reasoningEffort: low"), "{text}");
    }

    #[test]
    fn dsh_apply_removes_same_url_provider() {
        let dir = TestDir::new("prune_url");
        let home = dir.path();
        std::fs::create_dir_all(home.join(".dsh")).unwrap();
        let paths = AgentPaths::live(AgentKind::Dsh, home, "web", &HashMap::new());
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        std::fs::write(
            &paths.config,
            "\
- id: llm-pi-ai
  config:
    providers:
      old:
        displayName: old
        api: openai-responses
        baseURL: https://example.invalid/v1
      keep:
        displayName: keep
        api: openai-completions
        baseURL: https://other.invalid/v1
",
        )
        .unwrap();

        let mut document = AppDocument::default();
        document.providers = vec![Provider {
            id: "g".into(),
            name: "GJ".into(),
            base_url: "https://example.invalid/v1".into(),
            api: APIKind::Responses,
            catalog_url: String::new(),
            api_key: String::new(),
            compat_base_url: String::new(),
            models: vec![ModelEntry::new("flash", "Flash")],
        }];
        apply_doc(&document, home).unwrap();
        let text = std::fs::read_to_string(&paths.config).unwrap();
        assert!(!text.contains("displayName: old"));
        assert!(text.contains("displayName: keep"));
        assert!(text.contains("displayName: GJ"));
    }

    #[test]
    fn dsh_apply_normalizes_anthropic_endpoint() {
        let dir = TestDir::new("endpoint");
        let home = dir.path();
        std::fs::create_dir_all(home.join(".dsh")).unwrap();
        for pasted in ["https://relay.invalid/v1/messages", "https://relay.invalid/v1"] {
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
                    profile: "test".into(),
                    ..AgentSettings::default()
                },
                AgentKind::Dsh,
            );
            apply_doc(&document, home).unwrap();
            let paths = AgentPaths::live(AgentKind::Dsh, home, "test", &HashMap::new());
            let dsh = std::fs::read_to_string(&paths.config).unwrap();
            assert!(dsh.contains("baseURL: https://relay.invalid"), "{pasted}");
            assert!(!dsh.contains("relay.invalid/v1"), "{pasted}");
        }
    }

    #[test]
    fn dsh_apply_twice_is_idempotent() {
        let dir = TestDir::new("idempotent");
        let home = dir.path();
        std::fs::create_dir_all(home.join(".dsh")).unwrap();
        let paths = AgentPaths::live(AgentKind::Dsh, home, "web", &HashMap::new());
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        std::fs::write(
            &paths.config,
            "\
- id: llm-pi-ai
  config:
    providers:
      keep:
        displayName: keep
        api: openai-completions
        baseURL: https://other.invalid/v1
      old:
        displayName: old
        api: openai-responses
        baseURL: https://example.invalid/v1
- id: other-plugin
  foo: bar
- insert:
  - id: hand-hook
    name: foreign-insert
    config:
      x: 1
",
        )
        .unwrap();
        std::fs::write(paths.extra.as_ref().unwrap(), "PATH=/usr/bin\nexport OTHER=keep\n").unwrap();

        let mut document = AppDocument::default();
        document.providers = vec![sample_provider()];
        document.library.mcp = vec![MCPServer {
            id: "dbx".into(),
            transport: MCPTransport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "dbx".into()],
            url: String::new(),
            env: Default::default(),
        }];
        document.set_settings(
            AgentSettings {
                model: "g/flash".into(),
                effort: "high".into(),
                mcp_ids: vec!["dbx".into()],
                ..AgentSettings::default()
            },
            AgentKind::Dsh,
        );

        let first = apply_doc(&document, home).unwrap();
        assert!(!first.written.is_empty());
        let patch = std::fs::read_to_string(&paths.config).unwrap();
        let env = std::fs::read_to_string(paths.extra.as_ref().unwrap()).unwrap();
        assert!(patch.contains("displayName: keep"));
        assert!(!patch.contains("displayName: old"));
        assert!(patch.contains("id: other-plugin"));
        assert!(patch.contains("id: hand-hook"));
        assert!(patch.contains("name: foreign-insert"));
        assert!(patch.contains("id: mcp-dbx"));
        assert!(patch.contains("name: dsh-mcp-client"));
        assert!(env.contains("PATH=/usr/bin"));
        assert!(env.contains("export OTHER=keep"));
        assert!(env.contains("G_API_KEY=sk-test"));

        let second = apply_doc(&document, home).unwrap();
        assert!(second.written.is_empty());
        assert_eq!(std::fs::read_to_string(&paths.config).unwrap(), patch);
        assert_eq!(std::fs::read_to_string(paths.extra.as_ref().unwrap()).unwrap(), env);
    }

    #[test]
    fn dsh_deselecting_mcp_removes_insert_keeps_foreign() {
        let dir = TestDir::new("mcp");
        let home = dir.path();
        std::fs::create_dir_all(home.join(".dsh")).unwrap();
        let paths = AgentPaths::live(AgentKind::Dsh, home, "web", &HashMap::new());
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        std::fs::write(
            &paths.config,
            "\
- id: llm-pi-ai
  config:
    providers:
      keep:
        displayName: keep
        api: openai-completions
        baseURL: https://other.invalid/v1
- insert:
  - id: hand-hook
    name: foreign-insert
    config:
      x: 1
",
        )
        .unwrap();

        let mut document = AppDocument::default();
        document.providers = vec![sample_provider()];
        document.library.mcp = vec![MCPServer {
            id: "dbx".into(),
            transport: MCPTransport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "dbx".into()],
            url: String::new(),
            env: Default::default(),
        }];
        document.set_settings(
            AgentSettings {
                model: "g/flash".into(),
                effort: "high".into(),
                mcp_ids: vec!["dbx".into()],
                ..AgentSettings::default()
            },
            AgentKind::Dsh,
        );
        apply_doc(&document, home).unwrap();
        let with_mcp = std::fs::read_to_string(&paths.config).unwrap();
        assert!(with_mcp.contains("id: mcp-dbx"));
        assert!(with_mcp.contains("name: foreign-insert"));

        document.set_settings(
            AgentSettings {
                model: "g/flash".into(),
                effort: "high".into(),
                mcp_ids: vec![],
                ..AgentSettings::default()
            },
            AgentKind::Dsh,
        );
        apply_doc(&document, home).unwrap();
        let without = std::fs::read_to_string(&paths.config).unwrap();
        assert!(!without.contains("id: mcp-dbx"));
        assert!(!without.contains("dsh-mcp-client"));
        assert!(without.contains("id: hand-hook"));
        assert!(without.contains("name: foreign-insert"));
    }

    #[test]
    fn dsh_anthropic_adaptive_writes_force_adaptive_thinking_compat() {
        let dir = TestDir::new("compat");
        let home = dir.path();
        std::fs::create_dir_all(home.join(".dsh")).unwrap();
        let mut document = AppDocument::default();
        document.providers = vec![
            Provider {
                id: "claude".into(),
                name: "Claude".into(),
                base_url: "https://relay.invalid".into(),
                api: APIKind::Anthropic,
                catalog_url: String::new(),
                api_key: "sk".into(),
                compat_base_url: String::new(),
                models: vec![
                    ModelEntry {
                        efforts: Some(vec!["low".into(), "high".into()]),
                        ..ModelEntry::new("claude-opus-4-6", "Opus46")
                    },
                    ModelEntry {
                        efforts: None,
                        ..ModelEntry::new("claude-sonnet-4.6", "Sonnet46")
                    },
                    ModelEntry {
                        efforts: Some(vec!["low".into(), "high".into()]),
                        ..ModelEntry::new("claude-sonnet-4-5", "Sonnet45")
                    },
                    ModelEntry {
                        efforts: Some(vec![]),
                        ..ModelEntry::new("claude-haiku-5", "Haiku5")
                    },
                ],
            },
            Provider {
                id: "openai".into(),
                name: "OpenAI".into(),
                base_url: "https://example.invalid/v1".into(),
                api: APIKind::Responses,
                catalog_url: String::new(),
                api_key: "sk".into(),
                compat_base_url: String::new(),
                models: vec![ModelEntry {
                    efforts: Some(vec!["low".into(), "high".into()]),
                    ..ModelEntry::new("claude-opus-4-6", "Not Anthropic")
                }],
            },
        ];
        document.set_settings(
            AgentSettings {
                model: "claude/claude-opus-4-6".into(),
                effort: "high".into(),
                profile: "coding".into(),
                ..AgentSettings::default()
            },
            AgentKind::Dsh,
        );
        let first = apply_doc(&document, home).unwrap();
        assert!(!first.written.is_empty());
        let paths = AgentPaths::live(AgentKind::Dsh, home, "coding", &HashMap::new());
        let text = std::fs::read_to_string(&paths.config).unwrap();
        assert_eq!(text.matches("forceAdaptiveThinking: true").count(), 2, "{text}");
        assert!(text.contains(
            "\
          - id: claude-opus-4-6
            name: Opus46
            input:
              - text
            reasoningEfforts:
              low: low
              high: high
            compat:
              forceAdaptiveThinking: true"
        ));
        assert!(text.contains(
            "\
          - id: claude-sonnet-4.6
            name: Sonnet46
            input:
              - text
            compat:
              forceAdaptiveThinking: true"
        ));
        assert!(text.contains("id: claude-sonnet-4-5"));
        assert!(text.contains("id: claude-haiku-5"));
        assert!(text.contains("reasoningEfforts: false"));
        let openai = text
            .split("openai:")
            .nth(1)
            .unwrap_or("");
        assert!(!openai.contains("forceAdaptiveThinking"), "{text}");
        let sonnet45 = text
            .split("id: claude-sonnet-4-5")
            .nth(1)
            .and_then(|rest| rest.split("- id:").next())
            .unwrap_or("");
        assert!(!sonnet45.contains("compat:"), "{text}");
        let haiku = text
            .split("id: claude-haiku-5")
            .nth(1)
            .and_then(|rest| rest.split("- id:").next())
            .unwrap_or("");
        assert!(!haiku.contains("compat:"), "{text}");
        let second = apply_doc(&document, home).unwrap();
        assert!(
            second.written.is_empty(),
            "second apply wrote {:?}",
            second.written
        );
    }
}
