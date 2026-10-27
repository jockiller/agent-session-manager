use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::json_object;
use crate::config::apply::{AgentPaths, ApplyContext, ApplyRequest};
use crate::config::endpoint;
use crate::config::fileio::{read_json_object, write_json_map, ApplyError};
use crate::config::models::{APIKind, MCPServer, MCPTransport, ModelEntry, Provider};

pub(crate) const THINKING_LEVELS: &[&str] =
    &["off", "minimal", "low", "medium", "high", "xhigh", "max"];

const PRESERVE_KEYS: &[&str] = &["modelOverrides", "headers", "compat"];

const ADAPTIVE_MARKERS: &[&str] = &[
    "opus-4-6",
    "opus-4.6",
    "opus-4-7",
    "opus-4.7",
    "opus-4-8",
    "opus-4.8",
    "opus-5",
    "opus.5",
    "sonnet-4-6",
    "sonnet-4.6",
    "sonnet-5",
    "sonnet.5",
    "haiku-5",
    "haiku.5",
    "fable-5",
    "mythos-5",
];

pub fn apply(
    request: &ApplyRequest,
    paths: &AgentPaths,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    write_models(request, &paths.config, ctx)?;
    write_auth(request, &sibling(&paths.config, "auth.json"), ctx)?;
    write_settings(request, &sibling(&paths.config, "settings.json"), ctx)?;
    apply_mcp(request, &sibling(&paths.config, "mcp.json"), ctx)?;
    ctx.write_instructions(
        request.kind,
        &paths.instructions,
        &request.instructions,
        request.settings.instruction_on,
    )?;
    Ok(())
}

pub(crate) fn sibling(config: &Path, name: &str) -> PathBuf {
    match config.parent() {
        Some(directory) => directory.join(name),
        None => PathBuf::from(name),
    }
}

fn write_models(
    request: &ApplyRequest,
    path: &Path,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    let mut models = read_json_object(path);
    let mut providers = json_object(models.get("providers"));
    prune_providers(&mut providers, request);
    for provider in &request.providers {
        let preserved = match providers.get(&provider.id) {
            Some(Value::Object(previous)) => PRESERVE_KEYS
                .iter()
                .filter_map(|key| {
                    previous
                        .get(*key)
                        .cloned()
                        .map(|value| ((*key).to_string(), value))
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        let mut entry = provider_json(provider);
        for (key, value) in preserved {
            entry.insert(key, value);
        }
        providers.insert(provider.id.clone(), Value::Object(entry));
    }
    models.insert("providers".into(), Value::Object(providers));
    write_json_map(ctx.session, request.kind.raw(), &models, path, 0o644)?;
    Ok(())
}

fn prune_providers(providers: &mut Map<String, Value>, request: &ApplyRequest) {
    let urls = request.prune_urls();
    let keep: HashSet<&str> = request
        .providers
        .iter()
        .map(|provider| provider.id.as_str())
        .collect();
    let retired = request.retired_ids();
    let ids: Vec<String> = providers.keys().cloned().collect();
    for id in ids {
        if keep.contains(id.as_str()) {
            continue;
        }
        if retired.contains(&id) {
            providers.remove(&id);
            continue;
        }
        let Some(url) = provider_prune_url(providers.get(&id)) else {
            continue;
        };
        if urls.contains(&endpoint::normalize(&url)) {
            providers.remove(&id);
        }
    }
}

fn provider_prune_url(value: Option<&Value>) -> Option<String> {
    let object = value.and_then(Value::as_object)?;
    if let Some(Value::String(url)) = object.get("baseUrl") {
        return Some(url.clone());
    }
    match object
        .get("options")
        .and_then(Value::as_object)
        .and_then(|options| options.get("baseURL"))
    {
        Some(Value::String(url)) => Some(url.clone()),
        _ => None,
    }
}

fn write_auth(
    request: &ApplyRequest,
    path: &Path,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    let mut auth = read_json_object(path);
    for provider in &request.providers {
        let Some(key) = request.keys.get(&provider.id) else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        if let Some(Value::Object(current)) = auth.get(&provider.id) {
            if let Some(Value::String(kind)) = current.get("type") {
                if kind != "api_key" {
                    continue;
                }
            }
        }
        let mut entry = Map::new();
        entry.insert("type".into(), Value::String("api_key".into()));
        entry.insert("key".into(), Value::String(key.clone()));
        auth.insert(provider.id.clone(), Value::Object(entry));
    }
    write_json_map(ctx.session, request.kind.raw(), &auth, path, 0o600)?;
    Ok(())
}

fn write_settings(
    request: &ApplyRequest,
    path: &Path,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    let mut settings = read_json_object(path);
    let base = request.default_model_base();
    if !base.is_empty() {
        let parts: Vec<&str> = base.splitn(2, '/').collect();
        if parts.len() == 2 {
            settings.insert(
                "defaultProvider".into(),
                Value::String(parts[0].to_string()),
            );
            settings.insert("defaultModel".into(), Value::String(parts[1].to_string()));
        }
    }
    let effort = request.settings.effort.trim();
    if !effort.is_empty() && THINKING_LEVELS.contains(&effort) && effort != "off" {
        settings.insert(
            "defaultThinkingLevel".into(),
            Value::String(effort.to_string()),
        );
    } else {
        settings.remove("defaultThinkingLevel");
    }
    let saved_provider = settings
        .get("defaultProvider")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if base.is_empty()
        && !saved_provider.is_empty()
        && request.retired_ids().contains(&saved_provider)
    {
        settings.remove("defaultProvider");
        settings.remove("defaultModel");
    }
    write_json_map(ctx.session, request.kind.raw(), &settings, path, 0o644)?;
    Ok(())
}

pub(crate) fn apply_mcp(
    request: &ApplyRequest,
    path: &Path,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    let mut root = read_json_object(path);
    let mut servers = json_object(root.get("mcpServers"));
    let assigned: HashSet<String> = request
        .mcp
        .iter()
        .map(|server| sanitize_mcp_name(&server.id))
        .collect();
    let wanted: HashSet<String> = request
        .settings
        .mcp_ids
        .iter()
        .map(|id| sanitize_mcp_name(id))
        .collect();
    let names: Vec<String> = servers.keys().cloned().collect();
    for name in names {
        if wanted.contains(&name) && !assigned.contains(&name) {
            servers.remove(&name);
        }
    }
    for server in &request.mcp {
        servers.insert(
            sanitize_mcp_name(&server.id),
            Value::Object(mcp_entry(server)),
        );
    }
    if servers.is_empty() {
        root.remove("mcpServers");
    } else {
        root.insert("mcpServers".into(), Value::Object(servers));
    }
    if root.is_empty() && !path.exists() {
        return Ok(());
    }
    write_json_map(ctx.session, request.kind.raw(), &root, path, 0o644)?;
    Ok(())
}

fn mcp_entry(server: &MCPServer) -> Map<String, Value> {
    if server.transport == MCPTransport::Stdio {
        let mut object = Map::new();
        object.insert("command".into(), Value::String(server.command.clone()));
        object.insert(
            "args".into(),
            Value::Array(server.args.iter().cloned().map(Value::String).collect()),
        );
        if !server.env.is_empty() {
            let mut env = Map::new();
            for (key, value) in &server.env {
                env.insert(key.clone(), Value::String(value.clone()));
            }
            object.insert("env".into(), Value::Object(env));
        }
        return object;
    }
    let mut object = Map::new();
    object.insert("url".into(), Value::String(server.url.clone()));
    object
}

pub(crate) fn sanitize_mcp_name(name: &str) -> String {
    let mut out = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
            out.push(character);
        } else if out.chars().last() != Some('-') {
            out.push('-');
        }
    }
    if out.is_empty() {
        "mcp".into()
    } else {
        out
    }
}

pub(crate) fn base_url(provider: &Provider) -> String {
    endpoint::for_provider(&provider.base_url, provider.api)
}

pub(crate) fn provider_json(provider: &Provider) -> Map<String, Value> {
    let mut entry = Map::new();
    entry.insert(
        "api".into(),
        Value::String(provider.api.pi_api().to_string()),
    );
    let name = if provider.name.is_empty() {
        provider.id.as_str()
    } else {
        provider.name.as_str()
    };
    if name != provider.id {
        entry.insert("name".into(), Value::String(name.to_string()));
    }
    if !provider.base_url.is_empty() {
        entry.insert("baseUrl".into(), Value::String(base_url(provider)));
    }
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    for model in &provider.models {
        if seen.insert(model.wire_id().to_string()) {
            models.push(Value::Object(model_json(model, provider.api)));
        }
    }
    entry.insert("models".into(), Value::Array(models));
    entry
}

fn model_json(model: &ModelEntry, api: APIKind) -> Map<String, Value> {
    let mut entry = Map::new();
    entry.insert("id".into(), Value::String(model.wire_id().to_string()));
    if !model.name.is_empty() && model.name != model.wire_id() {
        entry.insert("name".into(), Value::String(model.name.clone()));
    }
    if let Some(context) = model.context {
        entry.insert("contextWindow".into(), Value::from(context));
    }
    if let Some(output) = model.output {
        entry.insert("maxTokens".into(), Value::from(output));
    }
    if !model.input.is_empty() {
        entry.insert(
            "input".into(),
            Value::Array(model.input.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(efforts) = &model.efforts {
        if efforts.is_empty() {
            entry.insert("reasoning".into(), Value::Bool(false));
        } else {
            entry.insert("reasoning".into(), Value::Bool(true));
            let mut map = Map::new();
            for (level, value) in thinking_map(efforts) {
                map.insert(level, Value::String(value));
            }
            entry.insert("thinkingLevelMap".into(), Value::Object(map));
        }
    }
    if let Some(compat) = compat(api, model.wire_id()) {
        let mut object = Map::new();
        for (key, flag) in compat {
            object.insert(key, Value::Bool(flag));
        }
        entry.insert("compat".into(), Value::Object(object));
    }
    entry
}

pub(crate) fn thinking_map(efforts: &[String]) -> Vec<(String, String)> {
    let present: HashSet<&str> = efforts
        .iter()
        .map(String::as_str)
        .filter(|level| *level != "off")
        .collect();
    THINKING_LEVELS
        .iter()
        .filter(|level| present.contains(*level))
        .map(|level| ((*level).to_string(), (*level).to_string()))
        .collect()
}

/// Claude 5 / 4.6+ only accept adaptive thinking; custom providers need compat.forceAdaptiveThinking.
pub(crate) fn compat(api: APIKind, wire_id: &str) -> Option<Vec<(String, bool)>> {
    if api == APIKind::Anthropic && is_adaptive_thinking(wire_id) {
        Some(vec![("forceAdaptiveThinking".into(), true)])
    } else {
        None
    }
}

pub(crate) fn is_adaptive_thinking(wire_id: &str) -> bool {
    let id = wire_id.to_lowercase();
    ADAPTIVE_MARKERS.iter().any(|marker| id.contains(marker))
}

pub(crate) fn is_exported_thinking_level(effort: &str) -> bool {
    let effort = effort.trim();
    !effort.is_empty() && THINKING_LEVELS.contains(&effort) && effort != "off"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::apply::apply;
    use crate::config::models::{
        AgentKind, AgentSettings, AppDocument, MCPServer, MCPTransport, ModelEntry, Provider,
    };
    use serde_json::json;
    use std::collections::{BTreeMap, HashMap};
    use std::path::Path;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_pi_{}_{}",
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

    pub(crate) fn make_document(base_url: &str, api: APIKind) -> AppDocument {
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
                    context: Some(200_000),
                    output: Some(64_000),
                    input: vec!["text".into(), "image".into()],
                    efforts: Some(vec![
                        "low".into(),
                        "medium".into(),
                        "high".into(),
                        "xhigh".into(),
                    ]),
                    ..ModelEntry::new("opus", "Claude Opus 5.5")
                },
                ModelEntry {
                    efforts: Some(vec![]),
                    ..ModelEntry::new("sonnet", "Claude Sonnet 5.5")
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
        document.set_settings(settings.clone(), AgentKind::Pi);
        document.set_settings(settings, AgentKind::Omp);
        document
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

    fn apply_pi(
        document: &AppDocument,
        home: &Path,
        env: &HashMap<String, String>,
    ) -> crate::config::apply::ApplyOutcome {
        apply(
            document,
            AgentKind::Pi,
            home,
            env,
            &home.join(".asm-backups"),
        )
        .unwrap()
    }

    fn read_json(path: &Path) -> Value {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    fn pi_paths(home: &Path, env: &HashMap<String, String>) -> AgentPaths {
        AgentPaths::live(AgentKind::Pi, home, "", env)
    }

    #[test]
    fn pi_anthropic_drops_version_suffix_but_responses_keeps_it() {
        let dir = TestDir::new("pi_responses");
        let home = dir.path();
        let empty = HashMap::new();
        apply_pi(
            &make_document("https://relay.example/v1", APIKind::Responses),
            home,
            &empty,
        );
        let paths = pi_paths(home, &empty);
        let models = read_json(&paths.config);
        let provider = &models["providers"]["claude"];
        assert_eq!(provider["api"], "openai-responses");
        assert_eq!(provider["baseUrl"], "https://relay.example/v1");
    }

    #[test]
    fn agent_kind_includes_pi_and_omp() {
        assert_eq!(
            AgentKind::ALL.map(|kind| kind.raw()),
            ["opencode", "dsh", "zcode", "grok", "pi", "omp"]
        );
        assert_eq!(APIKind::Anthropic.pi_api(), "anthropic-messages");
        assert_eq!(APIKind::Responses.pi_api(), "openai-responses");
        assert_eq!(APIKind::Completions.pi_api(), "openai-completions");
        assert_eq!(APIKind::Google.pi_api(), "google-generative-ai");
        assert_eq!(APIKind::Anthropic.zcode_api_type(), "anthropic-messages");
        assert_eq!(APIKind::Responses.zcode_api_type(), "openai-responses");
        assert_eq!(
            APIKind::Completions.zcode_api_type(),
            "openai-chat-completions"
        );
        assert_eq!(APIKind::Google.zcode_api_type(), "openai-chat-completions");
    }

    #[test]
    fn pi_apply_writes_models_auth_settings_and_mcp() {
        let dir = TestDir::new("pi_write");
        let home = dir.path();
        let empty = HashMap::new();
        apply_pi(
            &make_document("https://relay.example/v1", APIKind::Anthropic),
            home,
            &empty,
        );
        let paths = pi_paths(home, &empty);
        let models = read_json(&paths.config);
        let provider = &models["providers"]["claude"];
        assert_eq!(provider["api"], "anthropic-messages");
        assert_eq!(provider["baseUrl"], "https://relay.example");
        assert!(provider.get("apiKey").is_none());
        let models_array = provider["models"].as_array().unwrap();
        assert_eq!(models_array.len(), 2);
        let opus = &models_array[0];
        assert_eq!(opus["id"], "opus");
        assert_eq!(opus["contextWindow"], 200_000);
        assert_eq!(opus["maxTokens"], 64_000);
        assert_eq!(opus["input"], json!(["text", "image"]));
        assert_eq!(opus["reasoning"], true);
        let mut keys: Vec<_> = opus["thinkingLevelMap"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        keys.sort();
        assert_eq!(keys, ["high", "low", "medium", "xhigh"]);
        assert_eq!(models_array[1]["reasoning"], false);

        let auth = read_json(&sibling(&paths.config, "auth.json"));
        assert_eq!(auth["claude"]["key"], "sk-relay");

        let settings = read_json(&sibling(&paths.config, "settings.json"));
        assert_eq!(settings["defaultProvider"], "claude");
        assert_eq!(settings["defaultModel"], "opus");
        assert_eq!(settings["defaultThinkingLevel"], "high");

        let mcp = read_json(&sibling(&paths.config, "mcp.json"));
        let servers = &mcp["mcpServers"];
        let dbx = &servers["dbx"];
        assert_eq!(dbx["command"], "dbx-mcp");
        assert_eq!(dbx["args"], json!(["--stdio"]));
        assert_eq!(dbx["env"]["DBX_TOKEN"], "t");
        assert_eq!(servers["docs-server"]["url"], "https://mcp.example/sse");
        assert!(servers.get("docs server").is_none());

        let instructions = std::fs::read_to_string(&paths.instructions).unwrap();
        assert!(!instructions.contains("agentsync:begin"));
        assert_eq!(instructions, "keep diffs small\n");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&paths.config), 0o644);
            assert_eq!(mode(&sibling(&paths.config, "auth.json")), 0o600);
            assert_eq!(mode(&sibling(&paths.config, "settings.json")), 0o644);
            assert_eq!(mode(&sibling(&paths.config, "mcp.json")), 0o644);
        }
    }

    #[test]
    fn pi_apply_prunes_same_url_provider_and_keeps_foreign_ones() {
        let dir = TestDir::new("pi_prune_url");
        let home = dir.path();
        let empty = HashMap::new();
        let paths = pi_paths(home, &empty);
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        std::fs::write(
            &paths.config,
            serde_json::to_vec_pretty(&json!({
                "providers": {
                    "relay": {
                        "api": "openai-completions",
                        "baseUrl": "https://relay.example/v1/",
                        "models": []
                    },
                    "local": {
                        "api": "openai-completions",
                        "baseUrl": "http://127.0.0.1:11434/v1",
                        "models": []
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        apply_pi(
            &make_document("https://relay.example/v1", APIKind::Anthropic),
            home,
            &empty,
        );
        let models = read_json(&paths.config);
        let providers = &models["providers"];
        assert!(providers.get("relay").is_none());
        assert!(providers.get("local").is_some());
        assert!(providers.get("claude").is_some());
    }

    #[test]
    fn pi_apply_respects_coding_agent_directory_env() {
        let dir = TestDir::new("pi_env");
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let custom = dir.path().join("custom-pi");
        let empty = HashMap::new();
        let default_paths = AgentPaths::live(AgentKind::Pi, &home, "", &empty);
        assert_eq!(default_paths.config, home.join(".pi/agent/models.json"));
        let omp_default = AgentPaths::live(AgentKind::Omp, &home, "", &empty);
        assert_eq!(omp_default.config, home.join(".omp/agent/models.yml"));

        let env = HashMap::from([(
            "PI_CODING_AGENT_DIR".into(),
            custom.to_string_lossy().into_owned(),
        )]);
        let paths = AgentPaths::live(AgentKind::Pi, &home, "", &env);
        assert_eq!(paths.config, custom.join("models.json"));

        let tilde = HashMap::from([("OMP_AGENT_DIR".into(), "~/omp".into())]);
        let omp = AgentPaths::live(AgentKind::Omp, &home, "", &tilde);
        assert_eq!(omp.config, home.join("omp/models.yml"));

        apply_pi(
            &make_document("https://relay.example/v1", APIKind::Anthropic),
            &home,
            &env,
        );
        assert!(custom.join("models.json").exists());
        assert!(custom.join("auth.json").exists());
        assert!(custom.join("settings.json").exists());
        assert!(custom.join("mcp.json").exists());
        assert!(!home.join(".pi/agent/models.json").exists());
        assert_eq!(
            std::fs::read_to_string(home.join(".pi/AGENTS.md")).unwrap(),
            "keep diffs small\n"
        );
    }

    #[test]
    fn apply_normalizes_pasted_endpoint_for_pi() {
        let dir = TestDir::new("pi_endpoint");
        let home = dir.path();
        let empty = HashMap::new();
        for pasted in [
            "https://relay.invalid/v1/messages",
            "https://relay.invalid/v1",
        ] {
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
                    efforts: Some(vec!["low".into(), "high".into()]),
                    ..ModelEntry::new("opus", "Opus")
                }],
            }];
            let settings = AgentSettings {
                model: "claude/opus".into(),
                effort: "high".into(),
                ..AgentSettings::default()
            };
            document.set_settings(settings, AgentKind::Pi);
            apply_pi(&document, home, &empty);
            let paths = pi_paths(home, &empty);
            let pi = read_json(&paths.config);
            assert_eq!(
                pi["providers"]["claude"]["baseUrl"],
                "https://relay.invalid"
            );
        }
    }

    #[test]
    fn apply_keeps_version_for_openai_protocols() {
        let dir = TestDir::new("pi_openai");
        let home = dir.path();
        let empty = HashMap::new();
        let mut document = AppDocument::default();
        document.providers = vec![Provider {
            id: "g".into(),
            name: "GJ".into(),
            base_url: "https://relay.invalid/v1/responses".into(),
            api: APIKind::Responses,
            catalog_url: String::new(),
            api_key: "sk-relay".into(),
            compat_base_url: String::new(),
            models: vec![ModelEntry::new("flash", "Flash")],
        }];
        document.set_settings(
            AgentSettings {
                model: "g/flash".into(),
                ..AgentSettings::default()
            },
            AgentKind::Pi,
        );
        apply_pi(&document, home, &empty);
        let paths = pi_paths(home, &empty);
        let provider = &read_json(&paths.config)["providers"]["g"];
        assert_eq!(provider["api"], "openai-responses");
        assert_eq!(provider["baseUrl"], "https://relay.invalid/v1");
    }

    #[test]
    fn pi_settings_clears_default_of_retired_provider() {
        let dir = TestDir::new("pi_retired_default");
        let home = dir.path();
        let empty = HashMap::new();
        let paths = pi_paths(home, &empty);
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        let settings_path = sibling(&paths.config, "settings.json");
        std::fs::write(
            &settings_path,
            serde_json::to_vec_pretty(&json!({"defaultProvider": "b", "defaultModel": "m2"}))
                .unwrap(),
        )
        .unwrap();
        apply_pi(&two_provider_document(), home, &empty);
        let root = read_json(&settings_path);
        assert_eq!(root["defaultProvider"], "a");
        assert_eq!(root["defaultModel"], "m1");
    }

    #[test]
    fn pi_prune_drops_unpicked_provider_entries() {
        let dir = TestDir::new("pi_retired_prune");
        let home = dir.path();
        let empty = HashMap::new();
        let paths = pi_paths(home, &empty);
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        std::fs::write(
            &paths.config,
            serde_json::to_vec_pretty(&json!({
                "providers": {
                    "a": {"baseUrl": "https://a.example/v1", "api": "openai-completions"},
                    "b": {"baseUrl": "https://b.example/v1", "api": "openai-completions"}
                }
            }))
            .unwrap(),
        )
        .unwrap();
        apply_pi(&two_provider_document(), home, &empty);
        let providers = &read_json(&paths.config)["providers"];
        assert!(providers.get("a").is_some());
        assert!(providers.get("b").is_none());
    }

    #[test]
    fn pi_apply_twice_writes_nothing() {
        let dir = TestDir::new("pi_idem");
        let home = dir.path();
        let empty = HashMap::new();
        let paths = pi_paths(home, &empty);
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        std::fs::write(
            &paths.config,
            serde_json::to_vec_pretty(&json!({
                "note": "keep me",
                "providers": {
                    "claude": {
                        "api": "anthropic-messages",
                        "baseUrl": "https://old.example",
                        "modelOverrides": {"opus": {"temperature": 0.2}},
                        "headers": {"X-User": "yes"},
                        "compat": {"keep": true},
                        "models": [{"id": "stale"}]
                    },
                    "local": {
                        "api": "openai-completions",
                        "baseUrl": "http://127.0.0.1:11434/v1",
                        "models": []
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            sibling(&paths.config, "auth.json"),
            serde_json::to_vec_pretty(&json!({
                "google": {"type": "oauth", "key": "tok"},
                "claude": {"type": "api_key", "key": "sk-old"}
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            sibling(&paths.config, "settings.json"),
            serde_json::to_vec_pretty(&json!({
                "theme": "dark",
                "defaultProvider": "local",
                "defaultModel": "llama"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            sibling(&paths.config, "mcp.json"),
            serde_json::to_vec_pretty(&json!({
                "mcpServers": {
                    "keep-me": {"command": "keep", "args": []},
                    "dbx": {"command": "old-dbx", "args": []}
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let document = make_document("https://relay.example/v1", APIKind::Anthropic);
        let first = apply_pi(&document, home, &empty);
        assert!(!first.written.is_empty());
        let models = read_json(&paths.config);
        assert_eq!(models["note"], "keep me");
        assert_eq!(
            models["providers"]["claude"]["modelOverrides"]["opus"]["temperature"],
            0.2
        );
        assert_eq!(models["providers"]["claude"]["headers"]["X-User"], "yes");
        assert_eq!(models["providers"]["claude"]["compat"]["keep"], true);
        assert!(models["providers"].get("local").is_some());
        let auth = read_json(&sibling(&paths.config, "auth.json"));
        assert_eq!(auth["google"]["type"], "oauth");
        assert_eq!(auth["claude"]["key"], "sk-relay");
        let settings = read_json(&sibling(&paths.config, "settings.json"));
        assert_eq!(settings["theme"], "dark");
        let mcp = read_json(&sibling(&paths.config, "mcp.json"));
        assert!(mcp["mcpServers"].get("keep-me").is_some());

        let second = apply_pi(&document, home, &empty);
        assert!(
            second.written.is_empty(),
            "second apply wrote {:?}",
            second.written
        );
    }

    #[test]
    fn force_adaptive_thinking_compat() {
        assert!(is_adaptive_thinking("claude-opus-4-6"));
        assert!(is_adaptive_thinking("sonnet-4.6"));
        assert!(is_adaptive_thinking("haiku-5"));
        assert!(!is_adaptive_thinking("opus"));
        assert!(compat(APIKind::Anthropic, "claude-opus-5").is_some());
        assert!(compat(APIKind::Responses, "claude-opus-5").is_none());
        assert_eq!(sanitize_mcp_name("docs server"), "docs-server");
        assert_eq!(sanitize_mcp_name("@@@"), "-");
        assert_eq!(sanitize_mcp_name(""), "mcp");
    }
}
