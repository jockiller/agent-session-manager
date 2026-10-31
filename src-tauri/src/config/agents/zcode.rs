use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::config::apply::{AgentPaths, ApplyContext, ApplyRequest};
use crate::config::endpoint;
use crate::config::fileio::{open_code_mcp, read_json_object, write_json_map, ApplyError};
use crate::config::models::{MCPServer, MCPTransport, ModelEntry, Provider};

pub fn apply(
    request: &ApplyRequest,
    paths: &AgentPaths,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    let mut root = read_json_object(&paths.config);
    let mut providers: BTreeMap<String, Value> =
        super::json_object(root.get("provider")).into_iter().collect();
    endpoint::prune_providers(&mut providers, &request.providers, &request.retired);
    for provider in &request.providers {
        let key = request.keys.get(&provider.id).cloned().unwrap_or_default();
        providers.insert(provider.id.clone(), Value::Object(zcode_provider(provider, &key)));
    }
    let alive: HashSet<String> = providers.keys().cloned().collect();
    root.insert("provider".into(), Value::Object(providers.into_iter().collect()));

    let model = request.default_model_base();
    if !model.is_empty() {
        root.insert("model".into(), Value::String(model));
    } else if request.clears_default(root.get("model").and_then(Value::as_str).unwrap_or("")) {
        root.remove("model");
    }
    let effort = request.settings.effort.trim();
    if !effort.is_empty() {
        root.insert("thoughtLevel".into(), Value::String(effort.to_string()));
    } else {
        root.remove("thoughtLevel");
    }

    let mut mcp = super::json_object(root.get("mcp"));
    sync_mcp(&mut mcp, request);
    if mcp.is_empty() {
        root.remove("mcp");
    } else {
        root.insert("mcp".into(), Value::Object(mcp));
    }

    write_json_map(ctx.session, request.kind.raw(), &root, &paths.config, 0o600)?;
    write_cli_mcp(request, &paths.config, ctx)?;
    ctx.write_instructions(
        request.kind,
        &paths.instructions,
        &request.instructions,
        request.settings.instruction_on,
    )?;
    if let Some(extra) = &paths.extra {
        upsert_rules(extra, request, &alive, ctx)?;
    }
    Ok(())
}

fn base_url(provider: &Provider) -> String {
    endpoint::for_provider(&provider.base_url, provider.api)
}

fn sync_mcp(section: &mut Map<String, Value>, request: &ApplyRequest) {
    let assigned: HashSet<&str> = request.mcp.iter().map(|server| server.id.as_str()).collect();
    let names: Vec<String> = section.keys().cloned().collect();
    for name in names {
        if request.settings.mcp_ids.contains(&name) && !assigned.contains(name.as_str()) {
            section.remove(&name);
        }
    }
    for server in &request.mcp {
        section.insert(server.id.clone(), Value::Object(open_code_mcp(server)));
    }
}

fn cli_config_path(config: &Path) -> PathBuf {
    config
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("cli/config.json"))
        .unwrap_or_else(|| PathBuf::from("cli/config.json"))
}

fn write_cli_mcp(
    request: &ApplyRequest,
    config: &Path,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    let cli = cli_config_path(config);
    if !cli.exists() {
        return Ok(());
    }
    let mut root = read_json_object(&cli);
    let mut mcp = super::json_object(root.get("mcp"));
    let mut servers = super::json_object(mcp.get("servers"));
    let assigned: HashSet<&str> = request.mcp.iter().map(|server| server.id.as_str()).collect();
    let names: Vec<String> = servers.keys().cloned().collect();
    for name in names {
        if request.settings.mcp_ids.contains(&name) && !assigned.contains(name.as_str()) {
            servers.remove(&name);
        }
    }
    for server in &request.mcp {
        servers.insert(server.id.clone(), Value::Object(zcode_cli_entry(server)));
    }
    if servers.is_empty() {
        mcp.remove("servers");
    } else {
        mcp.insert("servers".into(), Value::Object(servers));
    }
    if mcp.is_empty() {
        root.remove("mcp");
    } else {
        root.insert("mcp".into(), Value::Object(mcp));
    }
    write_json_map(ctx.session, request.kind.raw(), &root, &cli, 0o644)?;
    Ok(())
}

fn zcode_cli_entry(server: &MCPServer) -> Map<String, Value> {
    if server.transport == MCPTransport::Stdio {
        let mut entry = Map::new();
        entry.insert("type".into(), Value::String("stdio".into()));
        entry.insert("command".into(), Value::String(server.command.clone()));
        entry.insert(
            "args".into(),
            Value::Array(server.args.iter().cloned().map(Value::String).collect()),
        );
        entry.insert("enabled".into(), Value::Bool(true));
        if !server.env.is_empty() {
            let mut env = Map::new();
            for (key, value) in &server.env {
                env.insert(key.clone(), Value::String(value.clone()));
            }
            entry.insert("env".into(), Value::Object(env));
        }
        entry
    } else {
        let mut entry = Map::new();
        entry.insert("type".into(), Value::String("http".into()));
        entry.insert("url".into(), Value::String(server.url.clone()));
        entry.insert("enabled".into(), Value::Bool(true));
        entry
    }
}

fn zcode_provider(provider: &Provider, key: &str) -> Map<String, Value> {
    let mut models = Map::new();
    for model in &provider.models {
        models.insert(model.id.clone(), Value::Object(zcode_model_entry(model)));
    }
    let mut options = Map::new();
    if !provider.base_url.is_empty() {
        options.insert("baseURL".into(), Value::String(base_url(provider)));
    }
    if !key.is_empty() {
        options.insert("apiKey".into(), Value::String(key.to_string()));
    }
    let name = if provider.name.is_empty() {
        provider.id.clone()
    } else {
        provider.name.clone()
    };
    let mut object = Map::new();
    object.insert("enabled".into(), Value::Bool(true));
    object.insert("kind".into(), Value::String(provider.api.zcode_kind().into()));
    object.insert("name".into(), Value::String(name));
    object.insert("source".into(), Value::String("custom".into()));
    object.insert("options".into(), Value::Object(options));
    object.insert("models".into(), Value::Object(models));
    object
}

fn zcode_model_entry(model: &ModelEntry) -> Map<String, Value> {
    let mut entry = Map::new();
    entry.insert("name".into(), Value::String(model.name.clone()));
    if model.wire_id() != model.id {
        entry.insert("id".into(), Value::String(model.wire_id().to_string()));
    }
    if let Some(context) = model.context {
        let mut limit = Map::new();
        limit.insert("context".into(), Value::Number(context.into()));
        if let Some(output) = model.output {
            limit.insert("output".into(), Value::Number(output.into()));
        }
        entry.insert("limit".into(), Value::Object(limit));
    } else if let Some(output) = model.output {
        let mut limit = Map::new();
        limit.insert("output".into(), Value::Number(output.into()));
        entry.insert("limit".into(), Value::Object(limit));
    }
    if !model.input.is_empty() {
        let mut modalities = Map::new();
        modalities.insert(
            "input".into(),
            Value::Array(model.input.iter().cloned().map(Value::String).collect()),
        );
        modalities.insert("output".into(), Value::Array(vec![Value::String("text".into())]));
        entry.insert("modalities".into(), Value::Object(modalities));
    }
    if let Some(efforts) = &model.efforts {
        if efforts.is_empty() {
            entry.insert("reasoning".into(), Value::Bool(false));
        } else {
            let mut reasoning = Map::new();
            reasoning.insert("enabled".into(), Value::Bool(true));
            reasoning.insert(
                "variants".into(),
                Value::Array(efforts.iter().cloned().map(Value::String).collect()),
            );
            entry.insert("reasoning".into(), Value::Object(reasoning));
        }
    }
    entry
}

fn upsert_rules(
    path: &Path,
    request: &ApplyRequest,
    alive: &HashSet<String>,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_json_object(path);
    let mut config = super::json_object(root.get("config"));
    let mut provider_config = super::json_object(config.get("providerConfigRules"));
    let mut rules = match provider_config.get("providerRules") {
        Some(Value::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    let ours: HashSet<&str> = request.providers.iter().map(|provider| provider.id.as_str()).collect();
    let urls = request.prune_urls();
    let retired = request.retired_ids();
    rules.retain(|rule| !should_drop_provider_rule(rule, &ours, &retired, &urls));
    for provider in &request.providers {
        rules.push(Value::Object(provider_rule(provider, request)));
    }
    provider_config.insert("providerRules".into(), Value::Array(rules));
    config.insert("providerConfigRules".into(), Value::Object(provider_config));

    let model_rules = upsert_model_rules(&config, alive, request);
    let mut model_config = super::json_object(config.get("modelConfigRules"));
    model_config.insert("providerModelRules".into(), Value::Array(model_rules));
    config.insert("modelConfigRules".into(), Value::Object(model_config));

    root.insert("config".into(), Value::Object(config));
    write_json_map(ctx.session, request.kind.raw(), &root, path, 0o644)?;
    Ok(())
}

fn should_drop_provider_rule(
    rule: &Value,
    ours: &HashSet<&str>,
    retired: &HashSet<String>,
    urls: &HashSet<String>,
) -> bool {
    let Some(object) = rule.as_object() else {
        return false;
    };
    if let Some(id) = object.get("providerId").and_then(Value::as_str) {
        if ours.contains(id) || retired.contains(id) {
            return true;
        }
    }
    let url = rule_api_url(object);
    !url.is_empty() && urls.contains(&url)
}

fn rule_api_url(rule: &Map<String, Value>) -> String {
    let Some(api) = rule
        .get("config")
        .and_then(Value::as_object)
        .and_then(|config| config.get("api"))
        .and_then(Value::as_object)
    else {
        return String::new();
    };
    let raw = match api.get("baseUrl") {
        Some(Value::String(url)) => url.as_str(),
        _ => match api.get("baseURL") {
            Some(Value::String(url)) => url.as_str(),
            _ => "",
        },
    };
    endpoint::normalize(raw)
}

fn provider_rule(provider: &Provider, request: &ApplyRequest) -> Map<String, Value> {
    let ids: Vec<Value> = provider
        .models
        .iter()
        .map(|model| Value::String(model.id.clone()))
        .collect();
    let mut api = Map::new();
    api.insert("type".into(), Value::String(provider.api.zcode_api_type().into()));
    if !provider.base_url.is_empty() {
        api.insert("baseUrl".into(), Value::String(base_url(provider)));
    }
    let mut access = Map::new();
    access.insert("type".into(), Value::String("api-key".into()));
    if let Some(key) = request.keys.get(&provider.id) {
        if !key.is_empty() {
            access.insert("apiKey".into(), Value::String(key.clone()));
        }
    }
    let mut config = Map::new();
    config.insert("access".into(), Value::Object(access));
    config.insert("api".into(), Value::Object(api));
    config.insert("group".into(), Value::String("standard-personal".into()));
    config.insert("modelOrder".into(), Value::Array(ids.clone()));
    config.insert("personalModelIds".into(), Value::Array(ids));
    let name = if provider.name.is_empty() {
        provider.id.clone()
    } else {
        provider.name.clone()
    };
    let mut rule = Map::new();
    rule.insert("enabled".into(), Value::Bool(true));
    rule.insert("providerId".into(), Value::String(provider.id.clone()));
    rule.insert("providerName".into(), Value::String(name));
    rule.insert("config".into(), Value::Object(config));
    rule
}

fn upsert_model_rules(
    config: &Map<String, Value>,
    alive: &HashSet<String>,
    request: &ApplyRequest,
) -> Vec<Value> {
    let existing = match config
        .get("modelConfigRules")
        .and_then(Value::as_object)
        .and_then(|object| object.get("providerModelRules"))
    {
        Some(Value::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    let ours: HashSet<&str> = request.providers.iter().map(|provider| provider.id.as_str()).collect();
    let urls = request.prune_urls();
    let retired = request.retired_ids();
    let mut rules_by_provider: HashMap<String, Map<String, Value>> = HashMap::new();
    if let Some(Value::Array(items)) = config
        .get("providerConfigRules")
        .and_then(Value::as_object)
        .and_then(|object| object.get("providerRules"))
    {
        for rule in items {
            if let Some(object) = rule.as_object() {
                if let Some(id) = object.get("providerId").and_then(Value::as_str) {
                    rules_by_provider.insert(id.to_string(), object.clone());
                }
            }
        }
    }
    let mut legacy_by_model: HashMap<String, Map<String, Value>> = HashMap::new();
    for rule in &existing {
        let Some(object) = rule.as_object() else {
            continue;
        };
        let Some(model_id) = object.get("modelId").and_then(Value::as_str) else {
            continue;
        };
        let Some(body) = object.get("config").and_then(Value::as_object) else {
            continue;
        };
        legacy_by_model.entry(model_id.to_string()).or_insert_with(|| body.clone());
    }
    let mut by_key: HashMap<String, Value> = HashMap::new();
    let mut rules = Vec::new();
    for rule in &existing {
        let Some(object) = rule.as_object() else {
            continue;
        };
        let Some(provider_id) = object.get("providerId").and_then(Value::as_str) else {
            continue;
        };
        let Some(model_id) = object.get("modelId").and_then(Value::as_str) else {
            continue;
        };
        by_key.insert(format!("{provider_id}/{model_id}"), rule.clone());
        if ours.contains(provider_id) {
            continue;
        }
        if retired.contains(provider_id) {
            continue;
        }
        if !alive.contains(provider_id) {
            continue;
        }
        if let Some(target) = rules_by_provider.get(provider_id) {
            let url = rule_api_url(target);
            if !url.is_empty() && urls.contains(&url) {
                continue;
            }
        }
        rules.push(rule.clone());
    }
    for provider in &request.providers {
        for model in &provider.models {
            let id = format!("{}/{}", provider.id, model.id);
            let mut rule = by_key.get(&id).cloned().unwrap_or_else(|| {
                let mut object = Map::new();
                object.insert("modelId".into(), Value::String(model.id.clone()));
                object.insert("providerId".into(), Value::String(provider.id.clone()));
                object.insert(
                    "config".into(),
                    Value::Object(legacy_by_model.get(&model.id).cloned().unwrap_or_default()),
                );
                Value::Object(object)
            });
            rebuild_model_rule(&mut rule, model);
            rules.push(rule);
        }
    }
    rules
}

fn rebuild_model_rule(rule: &mut Value, model: &ModelEntry) {
    let Some(object) = rule.as_object_mut() else {
        return;
    };
    let mut body = match object.get("config") {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    };
    let mut props = match body.get("properties") {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    };
    if let Some(context) = model.context {
        props.insert("contextWindow".into(), Value::Number(context.into()));
    }
    let mut formats = match props.get("inputFormat") {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    };
    formats.insert(
        "supportsImage".into(),
        Value::Bool(model.input.iter().any(|item| item == "image")),
    );
    if model.input.iter().any(|item| item == "pdf") {
        formats.insert("supportsPdf".into(), Value::Bool(true));
    }
    if model.input.iter().any(|item| item == "video") {
        formats.insert("supportsVideo".into(), Value::Bool(true));
    }
    if model.input.iter().any(|item| item == "audio") {
        formats.insert("supportsAudio".into(), Value::Bool(true));
    }
    props.insert("inputFormat".into(), Value::Object(formats));
    body.insert("properties".into(), Value::Object(props));
    let mut specs = match body.get("optionSpecs") {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    };
    if let Some(efforts) = &model.efforts {
        if !efforts.is_empty() {
            let mut level = match specs.get("reasoningLevel") {
                Some(Value::Object(map)) => map.clone(),
                _ => Map::new(),
            };
            level.insert(
                "values".into(),
                Value::Array(efforts.iter().cloned().map(Value::String).collect()),
            );
            specs.insert("reasoningLevel".into(), Value::Object(level));
        } else {
            specs.remove("reasoningLevel");
        }
    } else {
        specs.remove("reasoningLevel");
    }
    if let Some(output) = model.output {
        let mut max_output = match specs.get("maxOutputTokens") {
            Some(Value::Object(map)) => map.clone(),
            _ => Map::new(),
        };
        max_output.insert("max".into(), Value::Number(output.into()));
        specs.insert("maxOutputTokens".into(), Value::Object(max_output));
    }
    if specs.is_empty() {
        body.remove("optionSpecs");
    } else {
        body.insert("optionSpecs".into(), Value::Object(specs));
    }
    object.insert("config".into(), Value::Object(body));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::apply::{apply, AgentPaths};
    use crate::config::fileio::read_json_object;
    use crate::config::models::{
        APIKind, AgentKind, AgentSettings, AppDocument, MCPServer, MCPTransport, ModelEntry, Provider,
    };
    use serde_json::json;
    use std::collections::{BTreeMap, HashMap};
    use std::path::Path;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_zcode_{}_{}",
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

    fn empty_env() -> HashMap<String, String> {
        HashMap::new()
    }

    fn zcode_paths(home: &Path) -> AgentPaths {
        AgentPaths::live(AgentKind::Zcode, home, "", &empty_env())
    }

    fn apply_zcode(document: &AppDocument, home: &Path, backups: &Path) -> crate::config::apply::ApplyOutcome {
        apply(document, AgentKind::Zcode, home, &empty_env(), backups).unwrap()
    }

    fn write_json(path: &Path, value: &Value) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
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
        for kind in AgentKind::ALL {
            document.set_settings(
                AgentSettings {
                    model: "a/m1".into(),
                    picked_model_ids: vec!["a/m1".into()],
                    ..AgentSettings::default()
                },
                kind,
            );
        }
        document
    }

    fn make_document(base_url: &str, api: APIKind) -> AppDocument {
        let mut document = AppDocument::default();
        document.providers = vec![Provider {
            id: "claude".into(),
            name: "Claude".into(),
            base_url: base_url.into(),
            api,
            catalog_url: String::new(),
            api_key: "sk-relay".into(),
            compat_base_url: String::new(),
            models: vec![
                ModelEntry {
                    id: "opus".into(),
                    request_id: None,
                    name: "Claude Opus 5.5".into(),
                    context: Some(200_000),
                    output: Some(64_000),
                    input: vec!["text".into(), "image".into()],
                    efforts: Some(vec![
                        "low".into(),
                        "medium".into(),
                        "high".into(),
                        "xhigh".into(),
                    ]),
                    cost: None,
                    tool_call: true,
                },
                ModelEntry {
                    id: "sonnet".into(),
                    request_id: None,
                    name: "Claude Sonnet 5.5".into(),
                    context: None,
                    output: None,
                    input: Vec::new(),
                    efforts: Some(vec![]),
                    cost: None,
                    tool_call: true,
                },
            ],
        }];
        document.library.instructions = "keep diffs small".into();
        document.library.mcp = vec![
            MCPServer {
                id: "dbx".into(),
                transport: MCPTransport::Stdio,
                command: "dbx-mcp".into(),
                args: vec!["--stdio".into()],
                url: String::new(),
                env: BTreeMap::from([("DBX_TOKEN".into(), "t".into())]),
            },
            MCPServer {
                id: "docs server".into(),
                transport: MCPTransport::Sse,
                command: String::new(),
                args: Vec::new(),
                url: "https://mcp.example/sse".into(),
                env: BTreeMap::new(),
            },
        ];
        let settings = AgentSettings {
            model: "claude/opus".into(),
            effort: "high".into(),
            instruction_on: true,
            mcp_ids: vec!["dbx".into(), "docs server".into()],
            ..AgentSettings::default()
        };
        document.set_settings(settings, AgentKind::Zcode);
        document
    }

    fn provider_rules(root: &Map<String, Value>) -> Vec<&Map<String, Value>> {
        root.get("config")
            .and_then(Value::as_object)
            .and_then(|config| config.get("providerConfigRules"))
            .and_then(Value::as_object)
            .and_then(|object| object.get("providerRules"))
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_object).collect())
            .unwrap_or_default()
    }

    fn model_rules(root: &Map<String, Value>) -> Vec<&Map<String, Value>> {
        root.get("config")
            .and_then(Value::as_object)
            .and_then(|config| config.get("modelConfigRules"))
            .and_then(Value::as_object)
            .and_then(|object| object.get("providerModelRules"))
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_object).collect())
            .unwrap_or_default()
    }

    #[test]
    fn zcode_prune_drops_unpicked_provider_entries() {
        let dir = TestDir::new("prune");
        let home = dir.path();
        let paths = zcode_paths(home);
        let provider_config = paths.extra.as_ref().unwrap();
        write_json(
            &paths.config,
            &json!({
                "model": "b/m2",
                "provider": {
                    "a": {"kind": "openai-compatible", "options": {"baseURL": "https://a.example/v1"}},
                    "b": {"kind": "openai-compatible", "options": {"baseURL": "https://b.example/v1"}},
                },
            }),
        );
        let cli = home.join(".zcode/cli/config.json");
        write_json(
            &cli,
            &json!({"mcp": {"servers": {"orphan": {"type": "stdio", "command": "/old"}}}}),
        );
        write_json(
            provider_config,
            &json!({
                "schemaVersion": 1,
                "config": {
                    "providerConfigRules": {
                        "providerRules": [
                            {"providerId": "a", "config": {"api": {"type": "openai-chat-completions", "baseUrl": "https://a.example/v1"}}},
                            {"providerId": "b", "config": {"api": {"type": "openai-chat-completions", "baseUrl": "https://b.example/v1"}}},
                        ],
                    },
                    "modelConfigRules": {
                        "providerModelRules": [
                            {"providerId": "a", "modelId": "m1", "config": {}},
                            {"providerId": "b", "modelId": "m2", "config": {}},
                        ],
                    },
                },
            }),
        );

        apply_zcode(&two_provider_document(), home, &home.join("backups"));

        let root = read_json_object(&paths.config);
        let providers = root.get("provider").and_then(Value::as_object).unwrap();
        assert!(providers.get("a").is_some());
        assert!(providers.get("b").is_none());

        let rules_root = read_json_object(provider_config);
        let provider_rules = provider_rules(&rules_root);
        assert!(provider_rules.iter().any(|rule| rule.get("providerId").and_then(Value::as_str) == Some("a")));
        assert!(!provider_rules.iter().any(|rule| rule.get("providerId").and_then(Value::as_str) == Some("b")));

        let model_rules = model_rules(&rules_root);
        assert!(model_rules.iter().all(|rule| rule.get("providerId").and_then(Value::as_str) == Some("a")));
        assert!(!model_rules.iter().any(|rule| rule.get("providerId").and_then(Value::as_str) == Some("b")));
    }

    #[test]
    fn zcode_keeps_builtin_model_rules() {
        let dir = TestDir::new("builtin");
        let home = dir.path();
        let paths = zcode_paths(home);
        let provider_config = paths.extra.as_ref().unwrap();
        write_json(
            &paths.config,
            &json!({
                "provider": {
                    "builtin:zai": {"kind": "anthropic", "source": "builtin"},
                    "a": {"kind": "openai-compatible", "options": {"baseURL": "https://a.example/v1"}},
                },
            }),
        );
        write_json(
            provider_config,
            &json!({
                "schemaVersion": 1,
                "config": {
                    "modelConfigRules": {
                        "providerModelRules": [
                            {"providerId": "builtin:zai", "modelId": "glm-5", "config": {"properties": {"contextWindow": 200000}}},
                        ],
                    },
                },
            }),
        );

        apply_zcode(&two_provider_document(), home, &home.join("backups"));

        let rules_root = read_json_object(provider_config);
        let model_rules = model_rules(&rules_root);
        let builtin = model_rules
            .iter()
            .find(|rule| rule.get("providerId").and_then(Value::as_str) == Some("builtin:zai"));
        assert!(builtin.is_some());
        let properties = builtin
            .unwrap()
            .get("config")
            .and_then(Value::as_object)
            .and_then(|config| config.get("properties"))
            .and_then(Value::as_object);
        assert_eq!(
            properties.and_then(|object| object.get("contextWindow")).and_then(Value::as_i64),
            Some(200_000)
        );
    }

    #[test]
    fn zcode_apply_writes_thought_level_whitelist_and_protocol() {
        let dir = TestDir::new("thought");
        let home = dir.path();
        let paths = zcode_paths(home);
        let provider_config = paths.extra.as_ref().unwrap();
        write_json(
            provider_config,
            &json!({
                "schemaVersion": 1,
                "config": {
                    "modelConfigRules": {
                        "providerModelRules": [
                            {"providerId": "gpt-oooh", "modelId": "opus", "config": {"properties": {"supportsNativeWebSearch": true}}},
                        ],
                    },
                },
            }),
        );
        let cli = home.join(".zcode/cli/config.json");
        write_json(
            &cli,
            &json!({
                "mcp": {
                    "servers": {
                        "dbx": {"type": "stdio", "command": "/old/dbx", "args": [], "enabled": true}
                    }
                }
            }),
        );

        apply_zcode(&make_document("https://relay.example/v1", APIKind::Completions), home, &home.join("backups"));

        let root = read_json_object(&paths.config);
        assert_eq!(root.get("model").and_then(Value::as_str), Some("claude/opus"));
        assert_eq!(root.get("thoughtLevel").and_then(Value::as_str), Some("high"));
        let provider = root
            .get("provider")
            .and_then(Value::as_object)
            .and_then(|object| object.get("claude"))
            .and_then(Value::as_object)
            .unwrap();
        assert_eq!(provider.get("kind").and_then(Value::as_str), Some("openai-compatible"));
        assert_eq!(
            provider
                .get("options")
                .and_then(Value::as_object)
                .and_then(|options| options.get("baseURL"))
                .and_then(Value::as_str),
            Some("https://relay.example/v1")
        );
        let opus = provider
            .get("models")
            .and_then(Value::as_object)
            .and_then(|models| models.get("opus"))
            .and_then(Value::as_object)
            .unwrap();
        assert!(opus.get("reasoning").and_then(Value::as_object).is_some());
        assert_eq!(
            opus.get("reasoning")
                .and_then(Value::as_object)
                .and_then(|reasoning| reasoning.get("variants")),
            Some(&json!(["low", "medium", "high", "xhigh"]))
        );
        let sonnet = provider
            .get("models")
            .and_then(Value::as_object)
            .and_then(|models| models.get("sonnet"))
            .and_then(Value::as_object)
            .unwrap();
        assert_eq!(sonnet.get("reasoning").and_then(Value::as_bool), Some(false));

        let rules_root = read_json_object(provider_config);
        let provider_rules = provider_rules(&rules_root);
        let rule = provider_rules
            .iter()
            .find(|item| item.get("providerId").and_then(Value::as_str) == Some("claude"))
            .unwrap();
        let rule_config = rule.get("config").and_then(Value::as_object).unwrap();
        assert_eq!(
            rule_config
                .get("api")
                .and_then(Value::as_object)
                .and_then(|api| api.get("type"))
                .and_then(Value::as_str),
            Some("openai-chat-completions")
        );
        assert_eq!(rule_config.get("personalModelIds"), Some(&json!(["opus", "sonnet"])));
        assert_eq!(rule_config.get("modelOrder"), Some(&json!(["opus", "sonnet"])));
        let model_rules = model_rules(&rules_root);
        let opus_rule = model_rules
            .iter()
            .find(|item| item.get("modelId").and_then(Value::as_str) == Some("opus"))
            .unwrap();
        let properties = opus_rule
            .get("config")
            .and_then(Value::as_object)
            .and_then(|config| config.get("properties"))
            .and_then(Value::as_object)
            .unwrap();
        assert_eq!(properties.get("contextWindow").and_then(Value::as_i64), Some(200_000));
        assert_eq!(
            properties
                .get("inputFormat")
                .and_then(Value::as_object)
                .and_then(|formats| formats.get("supportsImage"))
                .and_then(Value::as_bool),
            Some(true)
        );
        let specs = opus_rule
            .get("config")
            .and_then(Value::as_object)
            .and_then(|config| config.get("optionSpecs"))
            .and_then(Value::as_object)
            .unwrap();
        assert_eq!(
            specs
                .get("reasoningLevel")
                .and_then(Value::as_object)
                .and_then(|level| level.get("values")),
            Some(&json!(["low", "medium", "high", "xhigh"]))
        );
        assert!(!model_rules.iter().any(|item| item.get("providerId").and_then(Value::as_str) == Some("gpt-oooh")));
        assert_eq!(
            properties.get("supportsNativeWebSearch").and_then(Value::as_bool),
            Some(true)
        );

        let cli_root = read_json_object(&cli);
        let servers = cli_root
            .get("mcp")
            .and_then(Value::as_object)
            .and_then(|mcp| mcp.get("servers"))
            .and_then(Value::as_object)
            .unwrap();
        assert_eq!(
            servers
                .get("dbx")
                .and_then(Value::as_object)
                .and_then(|entry| entry.get("command"))
                .and_then(Value::as_str),
            Some("dbx-mcp")
        );
        assert_eq!(
            servers
                .get("docs server")
                .and_then(Value::as_object)
                .and_then(|entry| entry.get("url"))
                .and_then(Value::as_str),
            Some("https://mcp.example/sse")
        );

        // Deviation 2: AGENTS.md is the whole file, not a marked block.
        let text = std::fs::read_to_string(&paths.instructions).unwrap();
        assert_eq!(text, "keep diffs small\n");
        assert!(!text.contains("agentsync:begin"));
    }

    #[test]
    fn zcode_anthropic_base_url_drops_version_suffix() {
        let dir = TestDir::new("anthropic");
        let home = dir.path();
        let paths = zcode_paths(home);
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();

        apply_zcode(
            &make_document("https://relay.example/v1", APIKind::Anthropic),
            home,
            &home.join("backups"),
        );

        let root = read_json_object(&paths.config);
        let provider = root
            .get("provider")
            .and_then(Value::as_object)
            .and_then(|object| object.get("claude"))
            .and_then(Value::as_object)
            .unwrap();
        assert_eq!(provider.get("kind").and_then(Value::as_str), Some("anthropic"));
        assert_eq!(
            provider
                .get("options")
                .and_then(Value::as_object)
                .and_then(|options| options.get("baseURL"))
                .and_then(Value::as_str),
            Some("https://relay.example")
        );

        let other_dir = TestDir::new("responses");
        let other_home = other_dir.path();
        let other = zcode_paths(other_home);
        std::fs::create_dir_all(other.config.parent().unwrap()).unwrap();
        let mut responses = make_document("https://relay.example/v1", APIKind::Responses);
        responses.set_settings(
            AgentSettings {
                model: "claude/opus".into(),
                effort: "high".into(),
                ..AgentSettings::default()
            },
            AgentKind::Zcode,
        );
        apply_zcode(&responses, other_home, &other_home.join("backups"));
        let other_root = read_json_object(&other.config);
        let other_provider = other_root
            .get("provider")
            .and_then(Value::as_object)
            .and_then(|object| object.get("claude"))
            .and_then(Value::as_object)
            .unwrap();
        assert_eq!(
            other_provider
                .get("options")
                .and_then(Value::as_object)
                .and_then(|options| options.get("baseURL"))
                .and_then(Value::as_str),
            Some("https://relay.example/v1")
        );
    }

    #[test]
    fn apply_normalizes_pasted_endpoint_for_zcode() {
        for pasted in ["https://relay.invalid/v1/messages", "https://relay.invalid/v1"] {
            let dir = TestDir::new("endpoint");
            let home = dir.path();
            let mut document = AppDocument::default();
            document.providers = vec![Provider {
                id: "claude".into(),
                name: "Claude".into(),
                base_url: pasted.into(),
                api: APIKind::Anthropic,
                catalog_url: String::new(),
                api_key: "sk-relay".into(),
                compat_base_url: String::new(),
                models: vec![ModelEntry {
                    id: "opus".into(),
                    request_id: None,
                    name: "Opus".into(),
                    context: None,
                    output: None,
                    input: Vec::new(),
                    efforts: Some(vec!["low".into(), "high".into()]),
                    cost: None,
                    tool_call: true,
                }],
            }];
            document.set_settings(
                AgentSettings {
                    model: "claude/opus".into(),
                    effort: "high".into(),
                    ..AgentSettings::default()
                },
                AgentKind::Zcode,
            );
            apply_zcode(&document, home, &home.join("backups"));
            let root = read_json_object(&zcode_paths(home).config);
            let provider = root
                .get("provider")
                .and_then(Value::as_object)
                .and_then(|object| object.get("claude"))
                .and_then(Value::as_object)
                .unwrap();
            assert_eq!(
                provider
                    .get("options")
                    .and_then(Value::as_object)
                    .and_then(|options| options.get("baseURL"))
                    .and_then(Value::as_str),
                Some("https://relay.invalid"),
                "{pasted}"
            );
        }
    }

    #[test]
    fn apply_twice_is_idempotent_and_keeps_foreign_content() {
        let dir = TestDir::new("idempotent");
        let home = dir.path();
        let paths = zcode_paths(home);
        let provider_config = paths.extra.as_ref().unwrap();
        let cli = home.join(".zcode/cli/config.json");
        write_json(
            &paths.config,
            &json!({
                "theme": "dark",
                "provider": {
                    "foreign": {
                        "kind": "openai-compatible",
                        "source": "user",
                        "options": {"baseURL": "https://foreign.example/v1"}
                    }
                },
                "mcp": {
                    "user-tool": {"type": "local", "command": ["keep-me"]}
                }
            }),
        );
        write_json(
            &cli,
            &json!({
                "ui": {"sidebar": true},
                "mcp": {
                    "servers": {
                        "user-tool": {"type": "stdio", "command": "keep-me", "args": [], "enabled": true}
                    }
                }
            }),
        );
        write_json(
            provider_config,
            &json!({
                "schemaVersion": 1,
                "config": {
                    "providerConfigRules": {
                        "providerRules": [
                            {
                                "providerId": "foreign",
                                "providerName": "Foreign",
                                "config": {"api": {"type": "openai-chat-completions", "baseUrl": "https://foreign.example/v1"}}
                            }
                        ]
                    },
                    "modelConfigRules": {
                        "providerModelRules": [
                            {"providerId": "foreign", "modelId": "kept", "config": {"properties": {"contextWindow": 111}}}
                        ]
                    }
                }
            }),
        );

        let document = make_document("https://relay.example/v1", APIKind::Completions);
        let first = apply_zcode(&document, home, &home.join("backups1"));
        assert!(!first.written.is_empty());
        let second = apply_zcode(&document, home, &home.join("backups2"));
        assert!(second.written.is_empty());
        assert!(second.backups.is_empty());

        let root = read_json_object(&paths.config);
        assert_eq!(root.get("theme").and_then(Value::as_str), Some("dark"));
        let providers = root.get("provider").and_then(Value::as_object).unwrap();
        assert!(providers.get("foreign").is_some());
        assert!(providers.get("claude").is_some());
        let mcp = root.get("mcp").and_then(Value::as_object).unwrap();
        assert!(mcp.get("user-tool").is_some());
        assert!(mcp.get("dbx").is_some());

        let cli_root = read_json_object(&cli);
        assert_eq!(
            cli_root
                .get("ui")
                .and_then(Value::as_object)
                .and_then(|ui| ui.get("sidebar"))
                .and_then(Value::as_bool),
            Some(true)
        );
        let servers = cli_root
            .get("mcp")
            .and_then(Value::as_object)
            .and_then(|mcp| mcp.get("servers"))
            .and_then(Value::as_object)
            .unwrap();
        assert!(servers.get("user-tool").is_some());
        assert!(servers.get("dbx").is_some());

        let rules_root = read_json_object(provider_config);
        assert_eq!(rules_root.get("schemaVersion").and_then(Value::as_i64), Some(1));
        let provider_rules = provider_rules(&rules_root);
        assert!(provider_rules.iter().any(|rule| rule.get("providerId").and_then(Value::as_str) == Some("foreign")));
        assert!(provider_rules.iter().any(|rule| rule.get("providerId").and_then(Value::as_str) == Some("claude")));
        let model_rules = model_rules(&rules_root);
        assert!(model_rules.iter().any(|rule| {
            rule.get("providerId").and_then(Value::as_str) == Some("foreign")
                && rule.get("modelId").and_then(Value::as_str) == Some("kept")
        }));
        assert!(model_rules.iter().any(|rule| {
            rule.get("providerId").and_then(Value::as_str) == Some("claude")
                && rule.get("modelId").and_then(Value::as_str) == Some("opus")
        }));
    }

    #[test]
    fn two_config_json_files_get_distinct_backups() {
        let dir = TestDir::new("backups");
        let home = dir.path();
        let paths = zcode_paths(home);
        let cli = home.join(".zcode/cli/config.json");
        write_json(&paths.config, &json!({"model": "old/v2"}));
        write_json(
            &cli,
            &json!({"mcp": {"servers": {"old": {"type": "stdio", "command": "/old"}}}}),
        );

        let outcome = apply_zcode(
            &make_document("https://relay.example/v1", APIKind::Completions),
            home,
            &home.join("backups"),
        );
        let v2 = outcome
            .backups
            .iter()
            .filter(|path| path.ends_with("v2/config.json"))
            .count();
        let cli_backups = outcome
            .backups
            .iter()
            .filter(|path| path.ends_with("cli/config.json"))
            .count();
        assert_eq!(v2, 1);
        assert_eq!(cli_backups, 1);
        let v2_path = outcome
            .backups
            .iter()
            .find(|path| path.ends_with("v2/config.json"))
            .unwrap();
        let cli_path = outcome
            .backups
            .iter()
            .find(|path| path.ends_with("cli/config.json"))
            .unwrap();
        assert_ne!(v2_path, cli_path);
        assert_eq!(std::fs::read_to_string(v2_path).unwrap().contains("old/v2"), true);
        assert!(std::fs::read_to_string(cli_path).unwrap().contains("/old"));
    }
}
