use std::collections::{BTreeMap, HashSet};

use serde_json::{Map, Value};

use crate::config::apply::{AgentPaths, ApplyContext, ApplyRequest};
use crate::config::endpoint;
use crate::config::fileio::{open_code_mcp, write_json_map, ApplyError};
use crate::config::models::{model_ref, APIKind, AgentSettings, EffortLevel, Provider};

pub fn apply(
    request: &ApplyRequest,
    paths: &AgentPaths,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    let mut root = crate::config::fileio::read_json_object(&paths.config);
    if root.get("$schema").is_none() {
        root.insert(
            "$schema".into(),
            Value::String("https://opencode.ai/config.json".into()),
        );
    }

    // Mutate nested objects in place so a second apply cannot reshuffle root keys.
    if !matches!(root.get("providers"), Some(Value::Object(_))) {
        root.insert("providers".into(), Value::Object(Map::new()));
    }
    let mut providers: BTreeMap<String, Value> = match root.get("providers") {
        Some(Value::Object(map)) => map.clone().into_iter().collect(),
        _ => BTreeMap::new(),
    };
    endpoint::prune_providers(&mut providers, &request.providers, &request.retired);
    for provider in &request.providers {
        let key = request.keys.get(&provider.id).cloned().unwrap_or_default();
        providers.insert(
            provider.id.clone(),
            Value::Object(open_code_provider(provider, &key, &request.settings)),
        );
    }
    if let Some(Value::Object(map)) = root.get_mut("providers") {
        *map = providers.into_iter().collect();
    }
    root.remove("provider");

    let model = request.default_model_ref();
    if !model.is_empty() {
        root.insert("model".into(), Value::String(model));
    } else {
        let current = match root.get("model") {
            Some(Value::String(value)) => value.as_str(),
            _ => "",
        };
        if request.clears_default(current) {
            root.remove("model");
        }
    }
    let small = request.settings.small_model.trim();
    if small.is_empty() {
        root.remove("small_model");
    } else {
        root.insert("small_model".into(), Value::String(small.to_string()));
    }

    let mut mcp = match root.get("mcp") {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    };
    let assigned: HashSet<String> = request.mcp.iter().map(|server| server.id.clone()).collect();
    let names: Vec<String> = mcp.keys().cloned().collect();
    for name in names {
        if assigned.contains(&name) || request.settings.mcp_ids.iter().any(|id| id == &name) {
            if !assigned.contains(&name) {
                mcp.remove(&name);
            }
        }
    }
    for server in &request.mcp {
        mcp.insert(server.id.clone(), Value::Object(open_code_mcp(server)));
    }
    if mcp.is_empty() {
        root.remove("mcp");
    } else if let Some(Value::Object(map)) = root.get_mut("mcp") {
        *map = mcp;
    } else {
        root.insert("mcp".into(), Value::Object(mcp));
    }

    let mut plugins = match root.get("plugins") {
        Some(Value::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    let disable = "-opencode.prompt.identity";
    let already = plugins.iter().any(|item| item.as_str() == Some(disable));
    if !already {
        plugins.push(Value::String(disable.into()));
        root.insert("plugins".into(), Value::Array(plugins));
    }

    write_json_map(ctx.session, request.kind.raw(), &root, &paths.config, 0o600)?;
    ctx.write_instructions(
        request.kind,
        &paths.instructions,
        &request.instructions,
        request.settings.instruction_on,
    )?;
    Ok(())
}

fn open_code_provider(provider: &Provider, key: &str, agent: &AgentSettings) -> Map<String, Value> {
    let mut settings = Map::new();
    if !provider.base_url.is_empty() {
        settings.insert(
            "baseURL".into(),
            Value::String(endpoint::for_sdk(&provider.base_url, provider.api)),
        );
    }
    if !key.is_empty() {
        settings.insert("apiKey".into(), Value::String(key.to_string()));
    }
    let mut models = Map::new();
    for model in &provider.models {
        let mut entry = Map::new();
        entry.insert("modelID".into(), Value::String(model.wire_id().to_string()));
        entry.insert("name".into(), Value::String(model.name.clone()));
        match (model.context, model.output) {
            (Some(context), Some(output)) => {
                let mut limit = Map::new();
                limit.insert("context".into(), Value::from(context));
                limit.insert("output".into(), Value::from(output));
                entry.insert("limit".into(), Value::Object(limit));
            }
            (Some(context), None) => {
                let mut limit = Map::new();
                limit.insert("context".into(), Value::from(context));
                entry.insert("limit".into(), Value::Object(limit));
            }
            (None, Some(output)) => {
                let mut limit = Map::new();
                limit.insert("output".into(), Value::from(output));
                entry.insert("limit".into(), Value::Object(limit));
            }
            (None, None) => {}
        }
        let input = if model.input.is_empty() {
            vec!["text".to_string()]
        } else {
            model.input.clone()
        };
        let mut capabilities = Map::new();
        capabilities.insert("tools".into(), Value::Bool(model.tool_call));
        capabilities.insert(
            "input".into(),
            Value::Array(input.into_iter().map(Value::String).collect()),
        );
        capabilities.insert(
            "output".into(),
            Value::Array(vec![Value::String("text".into())]),
        );
        entry.insert("capabilities".into(), Value::Object(capabilities));
        if let Some(efforts) = &model.efforts {
            if !efforts.is_empty() {
                let variants: Vec<Value> = efforts
                    .iter()
                    .map(|level| {
                        let mut variant = Map::new();
                        variant.insert("id".into(), Value::String(level.clone()));
                        variant.insert(
                            "settings".into(),
                            Value::Object(reasoning_settings(provider.api, level)),
                        );
                        Value::Object(variant)
                    })
                    .collect();
                entry.insert("variants".into(), Value::Array(variants));
                let preferred = agent.effort_for(&model_ref(&provider.id, &model.id));
                entry.insert(
                    "settings".into(),
                    Value::Object(reasoning_settings(
                        provider.api,
                        &default_effort(efforts, preferred),
                    )),
                );
            }
        }
        models.insert(model.id.clone(), Value::Object(entry));
    }
    let mut object = Map::new();
    object.insert(
        "name".into(),
        Value::String(if provider.name.is_empty() {
            provider.id.clone()
        } else {
            provider.name.clone()
        }),
    );
    object.insert(
        "package".into(),
        Value::String(provider.api.open_code_package().to_string()),
    );
    object.insert("models".into(), Value::Object(models));
    if !settings.is_empty() {
        object.insert("settings".into(), Value::Object(settings));
    }
    object
}

// Anthropic Messages reads `effort`; Google reads `thinkingConfig.thinkingLevel`; Responses/Completions read `reasoningEffort`.
fn reasoning_settings(api: APIKind, level: &str) -> Map<String, Value> {
    let mut settings = Map::new();
    if api == APIKind::Anthropic {
        let mut thinking = Map::new();
        thinking.insert("type".into(), Value::String("adaptive".into()));
        settings.insert("thinking".into(), Value::Object(thinking));
        settings.insert("effort".into(), Value::String(level.to_string()));
    } else if api == APIKind::Google {
        let mut thinking_config = Map::new();
        thinking_config.insert("includeThoughts".into(), Value::Bool(true));
        thinking_config.insert("thinkingLevel".into(), Value::String(level.to_string()));
        settings.insert("thinkingConfig".into(), Value::Object(thinking_config));
    } else {
        settings.insert("reasoningEffort".into(), Value::String(level.to_string()));
    }
    settings
}

// OpenCode and OpenChamber reset the variant to "Default" on model switch, so the base effort carries the default.
fn default_effort(efforts: &[String], preferred: &str) -> String {
    if efforts.iter().any(|item| item == preferred) {
        return preferred.to_string();
    }
    let mut sorted = efforts.to_vec();
    sorted.sort_by_key(|item| {
        EffortLevel::ALL
            .iter()
            .position(|level| level.raw() == item)
            .unwrap_or(EffortLevel::ALL.len())
    });
    sorted[sorted.len() / 2].clone()
}

#[cfg(test)]
mod tests {
    use crate::config::apply::{apply, AgentPaths};
    use crate::config::fileio::read_json_object;
    use crate::config::models::{
        APIKind, AgentKind, AgentSettings, AppDocument, MCPServer, ModelEntry, Provider,
    };
    use serde_json::Value;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

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

    fn apply_to(document: &AppDocument, home: &Path) -> crate::config::apply::ApplyOutcome {
        apply(
            document,
            AgentKind::Opencode,
            home,
            &HashMap::new(),
            &home.join("backups"),
        )
        .unwrap()
    }

    fn live_paths(home: &Path) -> AgentPaths {
        AgentPaths::live(AgentKind::Opencode, home, "", &HashMap::new())
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

    #[test]
    fn open_code_apply_writes_v2_providers() {
        let dir = TestDir::new("oc_v2");
        let home = dir.path();
        let paths = live_paths(home);
        let mut document = AppDocument::default();
        let mut flash = ModelEntry::new("flash", "Flash");
        flash.context = Some(200_000);
        flash.input = vec!["text".into(), "image".into()];
        flash.efforts = Some(vec!["low".into(), "high".into()]);
        let mut grok = ModelEntry::new("grok", "Grok");
        grok.efforts = Some(vec!["low".into(), "medium".into(), "xhigh".into()]);
        document.providers = vec![Provider {
            id: "g".into(),
            name: "GJ".into(),
            base_url: "https://example.invalid/v1".into(),
            api: APIKind::Responses,
            catalog_url: String::new(),
            api_key: "sk-test".into(),
            compat_base_url: String::new(),
            models: vec![flash, grok],
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
            AgentKind::Opencode,
        );
        document.library.mcp = vec![MCPServer {
            command: "npx".into(),
            args: vec!["-y".into(), "dbx".into()],
            ..MCPServer::new("dbx")
        }];
        apply_to(&document, home);

        let root = read_json_object(&paths.config);
        let providers = root.get("providers").and_then(Value::as_object).unwrap();
        let g = providers.get("g").and_then(Value::as_object).unwrap();
        assert_eq!(
            g.get("package").and_then(Value::as_str),
            Some("aisdk:@ai-sdk/openai")
        );
        let settings = g.get("settings").and_then(Value::as_object).unwrap();
        assert_eq!(
            settings.get("baseURL").and_then(Value::as_str),
            Some("https://example.invalid/v1")
        );
        assert_eq!(
            settings.get("apiKey").and_then(Value::as_str),
            Some("sk-test")
        );
        assert!(settings.get("reasoningEffort").is_none());
        assert_eq!(
            root.get("model").and_then(Value::as_str),
            Some("g/flash#high")
        );
        let flash = g
            .get("models")
            .and_then(Value::as_object)
            .and_then(|models| models.get("flash"))
            .and_then(Value::as_object)
            .unwrap();
        assert_eq!(
            flash
                .get("limit")
                .and_then(Value::as_object)
                .and_then(|limit| limit.get("context"))
                .and_then(Value::as_i64),
            Some(200_000)
        );
        let input = flash
            .get("capabilities")
            .and_then(Value::as_object)
            .and_then(|capabilities| capabilities.get("input"))
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(
            input.iter().filter_map(Value::as_str).collect::<Vec<_>>(),
            ["text", "image"]
        );
        let variants = flash.get("variants").and_then(Value::as_array).unwrap();
        let variant_ids: Vec<_> = variants
            .iter()
            .filter_map(|variant| variant.get("id").and_then(Value::as_str))
            .collect();
        assert_eq!(variant_ids, ["low", "high"]);
        assert_eq!(
            flash
                .get("settings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("reasoningEffort"))
                .and_then(Value::as_str),
            Some("high")
        );
        let grok = g
            .get("models")
            .and_then(Value::as_object)
            .and_then(|models| models.get("grok"))
            .and_then(Value::as_object)
            .unwrap();
        assert_eq!(
            grok.get("settings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("reasoningEffort"))
                .and_then(Value::as_str),
            Some("medium")
        );
        let mcp = root.get("mcp").and_then(Value::as_object).unwrap();
        assert!(mcp.get("dbx").is_some());
        let text = std::fs::read_to_string(&paths.instructions).unwrap();
        assert_eq!(text, "keep diffs small\n");
        assert!(!text.contains("<!-- agentsync:begin -->"));
    }

    #[test]
    fn open_code_writes_per_model_effort() {
        let dir = TestDir::new("oc_per_model_effort");
        let home = dir.path();
        let paths = live_paths(home);
        let mut document = AppDocument::default();
        let mut flash = ModelEntry::new("flash", "Flash");
        flash.efforts = Some(vec!["low".into(), "high".into()]);
        let mut grok = ModelEntry::new("grok", "Grok");
        grok.efforts = Some(vec!["low".into(), "medium".into(), "xhigh".into()]);
        document.providers = vec![Provider {
            id: "g".into(),
            name: "GJ".into(),
            base_url: "https://example.invalid/v1".into(),
            api: APIKind::Responses,
            catalog_url: String::new(),
            api_key: "sk-test".into(),
            compat_base_url: String::new(),
            models: vec![flash, grok],
        }];
        let mut settings = AgentSettings {
            model: "g/flash".into(),
            effort: "high".into(),
            ..AgentSettings::default()
        };
        settings
            .effort_by_model
            .insert("g/flash".into(), "low".into());
        settings
            .effort_by_model
            .insert("g/grok".into(), "xhigh".into());
        document.set_settings(settings, AgentKind::Opencode);
        apply_to(&document, home);

        let root = read_json_object(&paths.config);
        let models = root
            .get("providers")
            .and_then(Value::as_object)
            .and_then(|providers| providers.get("g"))
            .and_then(Value::as_object)
            .and_then(|provider| provider.get("models"))
            .and_then(Value::as_object)
            .unwrap();
        assert_eq!(
            models
                .get("flash")
                .and_then(Value::as_object)
                .and_then(|model| model.get("settings"))
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("reasoningEffort"))
                .and_then(Value::as_str),
            Some("low")
        );
        assert_eq!(
            models
                .get("grok")
                .and_then(Value::as_object)
                .and_then(|model| model.get("settings"))
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("reasoningEffort"))
                .and_then(Value::as_str),
            Some("xhigh")
        );
        assert_eq!(
            root.get("model").and_then(Value::as_str),
            Some("g/flash#low")
        );
    }

    #[test]
    fn open_code_anthropic_writes_effort_not_reasoning_effort() {
        let dir = TestDir::new("oc_anthropic");
        let home = dir.path();
        let paths = live_paths(home);
        let mut document = AppDocument::default();
        document.providers = vec![Provider {
            id: "gja".into(),
            name: "GJA".into(),
            base_url: "https://example.invalid".into(),
            api: APIKind::Anthropic,
            catalog_url: String::new(),
            api_key: "sk-test".into(),
            compat_base_url: String::new(),
            models: vec![ModelEntry {
                efforts: Some(vec!["low".into(), "medium".into(), "high".into()]),
                ..ModelEntry::new("claude-opus-5-5", "Claude Opus 5.5")
            }],
        }];
        document.set_settings(
            AgentSettings {
                model: "gja/claude-opus-5-5".into(),
                effort: "medium".into(),
                ..AgentSettings::default()
            },
            AgentKind::Opencode,
        );
        apply_to(&document, home);

        let root = read_json_object(&paths.config);
        let provider = root
            .get("providers")
            .and_then(Value::as_object)
            .and_then(|providers| providers.get("gja"))
            .and_then(Value::as_object)
            .unwrap();
        assert_eq!(
            provider.get("package").and_then(Value::as_str),
            Some("aisdk:@ai-sdk/anthropic")
        );
        assert_eq!(
            provider
                .get("settings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("baseURL"))
                .and_then(Value::as_str),
            Some("https://example.invalid/v1")
        );
        let model = provider
            .get("models")
            .and_then(Value::as_object)
            .and_then(|models| models.get("claude-opus-5-5"))
            .and_then(Value::as_object)
            .unwrap();
        let variants = model.get("variants").and_then(Value::as_array).unwrap();
        let variant_ids: Vec<_> = variants
            .iter()
            .filter_map(|variant| variant.get("id").and_then(Value::as_str))
            .collect();
        assert_eq!(variant_ids, ["low", "medium", "high"]);
        for variant in variants {
            let settings = variant.get("settings").and_then(Value::as_object).unwrap();
            assert!(settings.get("reasoningEffort").is_none());
            assert_eq!(
                settings.get("effort").and_then(Value::as_str),
                variant.get("id").and_then(Value::as_str)
            );
            assert_eq!(
                settings
                    .get("thinking")
                    .and_then(Value::as_object)
                    .and_then(|thinking| thinking.get("type"))
                    .and_then(Value::as_str),
                Some("adaptive")
            );
        }
        let base = model.get("settings").and_then(Value::as_object).unwrap();
        assert!(base.get("reasoningEffort").is_none());
        assert_eq!(base.get("effort").and_then(Value::as_str), Some("medium"));
    }

    #[test]
    fn open_code_google_writes_thinking_config() {
        let dir = TestDir::new("oc_google");
        let home = dir.path();
        let paths = live_paths(home);
        let mut document = AppDocument::default();
        document.providers = vec![Provider {
            id: "ggl".into(),
            name: "Gemini".into(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".into(),
            api: APIKind::Google,
            catalog_url: String::new(),
            api_key: "sk-test".into(),
            compat_base_url: String::new(),
            models: vec![ModelEntry {
                efforts: Some(vec![
                    "minimal".into(),
                    "low".into(),
                    "medium".into(),
                    "high".into(),
                ]),
                ..ModelEntry::new("gemini-3-flash", "Gemini 3 Flash")
            }],
        }];
        document.set_settings(
            AgentSettings {
                model: "ggl/gemini-3-flash".into(),
                effort: "medium".into(),
                ..AgentSettings::default()
            },
            AgentKind::Opencode,
        );
        let first = apply_to(&document, home);
        assert!(!first.written.is_empty());

        let root = read_json_object(&paths.config);
        let provider = root
            .get("providers")
            .and_then(Value::as_object)
            .and_then(|providers| providers.get("ggl"))
            .and_then(Value::as_object)
            .unwrap();
        assert_eq!(
            provider.get("package").and_then(Value::as_str),
            Some("aisdk:@ai-sdk/google")
        );
        let model = provider
            .get("models")
            .and_then(Value::as_object)
            .and_then(|models| models.get("gemini-3-flash"))
            .and_then(Value::as_object)
            .unwrap();
        let variants = model.get("variants").and_then(Value::as_array).unwrap();
        let variant_ids: Vec<_> = variants
            .iter()
            .filter_map(|variant| variant.get("id").and_then(Value::as_str))
            .collect();
        assert_eq!(variant_ids, ["minimal", "low", "medium", "high"]);
        for variant in variants {
            let settings = variant.get("settings").and_then(Value::as_object).unwrap();
            assert!(settings.get("reasoningEffort").is_none());
            assert!(settings.get("effort").is_none());
            let thinking = settings
                .get("thinkingConfig")
                .and_then(Value::as_object)
                .unwrap();
            assert_eq!(thinking.get("includeThoughts"), Some(&Value::Bool(true)));
            assert_eq!(
                thinking.get("thinkingLevel").and_then(Value::as_str),
                variant.get("id").and_then(Value::as_str)
            );
        }
        let base = model.get("settings").and_then(Value::as_object).unwrap();
        assert!(base.get("reasoningEffort").is_none());
        let thinking = base
            .get("thinkingConfig")
            .and_then(Value::as_object)
            .unwrap();
        assert_eq!(thinking.get("includeThoughts"), Some(&Value::Bool(true)));
        assert_eq!(
            thinking.get("thinkingLevel").and_then(Value::as_str),
            Some("medium")
        );

        let after_first = std::fs::read(&paths.config).unwrap();
        let second = apply_to(&document, home);
        assert!(second.written.is_empty());
        assert_eq!(std::fs::read(&paths.config).unwrap(), after_first);
    }

    #[test]
    fn open_code_apply_removes_same_url_provider() {
        let dir = TestDir::new("oc_same_url");
        let home = dir.path();
        let paths = live_paths(home);
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        let previous = serde_json::json!({
            "providers": {
                "gpt-oooh": {
                    "name": "old",
                    "package": "aisdk:@ai-sdk/openai",
                    "settings": {
                        "baseURL": "https://example.invalid/v1/",
                        "apiKey": "sk-old"
                    },
                    "models": { "flash": { "modelID": "flash" } }
                },
                "other": {
                    "name": "keep",
                    "package": "aisdk:@ai-sdk/openai",
                    "settings": { "baseURL": "https://other.invalid/v1" }
                }
            }
        });
        std::fs::write(&paths.config, serde_json::to_vec(&previous).unwrap()).unwrap();

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
        apply_to(&document, home);

        let root = read_json_object(&paths.config);
        let providers = root.get("providers").and_then(Value::as_object).unwrap();
        assert!(providers.get("gpt-oooh").is_none());
        assert!(providers.get("other").is_some());
        assert!(providers.get("g").is_some());
    }

    #[test]
    fn open_code_anthropic_base_url_gets_v1() {
        let dir = TestDir::new("oc_v1");
        let home = dir.path();
        let paths = live_paths(home);
        let mut document = AppDocument::default();
        document.providers = vec![Provider {
            id: "gja".into(),
            name: "GJA".into(),
            base_url: "https://example.invalid".into(),
            api: APIKind::Anthropic,
            catalog_url: String::new(),
            api_key: "sk-test".into(),
            compat_base_url: String::new(),
            models: vec![ModelEntry::new("sonnet", "Sonnet")],
        }];
        apply_to(&document, home);
        let root = read_json_object(&paths.config);
        let gja = root
            .get("providers")
            .and_then(Value::as_object)
            .and_then(|providers| providers.get("gja"))
            .and_then(Value::as_object)
            .unwrap();
        let settings = gja.get("settings").and_then(Value::as_object).unwrap();
        assert_eq!(
            settings.get("baseURL").and_then(Value::as_str),
            Some("https://example.invalid/v1")
        );
        assert_eq!(
            gja.get("package").and_then(Value::as_str),
            Some("aisdk:@ai-sdk/anthropic")
        );
    }

    #[test]
    fn apply_normalizes_pasted_endpoint_for_opencode() {
        let dir = TestDir::new("oc_paste");
        let home = dir.path();
        let paths = live_paths(home);
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
            document.set_settings(
                AgentSettings {
                    model: "claude/opus".into(),
                    effort: "high".into(),
                    ..AgentSettings::default()
                },
                AgentKind::Opencode,
            );
            apply_to(&document, home);
            let root = read_json_object(&paths.config);
            let settings = root
                .get("providers")
                .and_then(Value::as_object)
                .and_then(|providers| providers.get("claude"))
                .and_then(Value::as_object)
                .and_then(|provider| provider.get("settings"))
                .and_then(Value::as_object)
                .unwrap();
            assert_eq!(
                settings.get("baseURL").and_then(Value::as_str),
                Some("https://relay.invalid/v1"),
                "{pasted}"
            );
        }
    }

    #[test]
    fn open_code_prune_drops_unpicked_provider_entries() {
        let dir = TestDir::new("oc_prune");
        let home = dir.path();
        let paths = live_paths(home);
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        let legacy = serde_json::json!({
            "$schema": "https://opencode.ai/config.json",
            "providers": {
                "a": {
                    "npm": "aisdk:@ai-sdk/openai",
                    "options": { "baseURL": "https://a.example/v1" }
                },
                "b": {
                    "npm": "aisdk:@ai-sdk/openai",
                    "options": { "baseURL": "https://b.example/v1" }
                }
            },
            "model": "b/m2"
        });
        std::fs::write(&paths.config, serde_json::to_vec(&legacy).unwrap()).unwrap();
        apply_to(&two_provider_document(), home);

        let root = read_json_object(&paths.config);
        let providers = root.get("providers").and_then(Value::as_object).unwrap();
        assert!(providers.get("a").is_some());
        assert!(providers.get("b").is_none());
        assert_eq!(root.get("model").and_then(Value::as_str), Some("a/m1"));
    }

    #[test]
    fn apply_twice_is_idempotent_and_preserves_foreign_content() {
        let dir = TestDir::new("oc_idem");
        let home = dir.path();
        let paths = live_paths(home);
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        let existing = serde_json::json!({
            "$schema": "https://opencode.ai/config.json",
            "theme": "nord",
            "username": "jockiller",
            "otherKey": 42,
            "provider": { "legacy": true },
            "providers": {
                "foreign": {
                    "name": "Keep Me",
                    "package": "aisdk:@ai-sdk/openai",
                    "settings": { "baseURL": "https://foreign.invalid/v1" }
                }
            },
            "mcp": {
                "user-mcp": {
                    "type": "local",
                    "command": ["echo"]
                }
            }
        });
        std::fs::write(&paths.config, serde_json::to_vec_pretty(&existing).unwrap()).unwrap();

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
        document.library.instructions = "keep diffs small".into();
        document.library.mcp = vec![MCPServer {
            command: "npx".into(),
            args: vec!["-y".into(), "dbx".into()],
            ..MCPServer::new("dbx")
        }];
        document.set_settings(
            AgentSettings {
                model: "g/flash".into(),
                instruction_on: true,
                mcp_ids: vec!["dbx".into()],
                ..AgentSettings::default()
            },
            AgentKind::Opencode,
        );

        let first = apply_to(&document, home);
        assert!(!first.written.is_empty());
        let after_first = std::fs::read(&paths.config).unwrap();
        let root = read_json_object(&paths.config);
        let providers = root.get("providers").and_then(Value::as_object).unwrap();
        assert!(providers.get("foreign").is_some());
        assert!(providers.get("g").is_some());
        let mcp = root.get("mcp").and_then(Value::as_object).unwrap();
        assert!(mcp.get("user-mcp").is_some());
        assert!(mcp.get("dbx").is_some());
        assert_eq!(root.get("theme").and_then(Value::as_str), Some("nord"));
        assert_eq!(
            root.get("username").and_then(Value::as_str),
            Some("jockiller")
        );
        assert_eq!(root.get("otherKey").and_then(Value::as_i64), Some(42));
        assert!(root.get("provider").is_none());

        let second = apply_to(&document, home);
        assert!(second.written.is_empty());
        assert_eq!(std::fs::read(&paths.config).unwrap(), after_first);
        assert_eq!(
            std::fs::read_to_string(&paths.instructions).unwrap(),
            "keep diffs small\n"
        );
    }

    #[test]
    fn apply_backs_up_changed_opencode_json() {
        let dir = TestDir::new("oc_backup");
        let home = dir.path();
        let paths = live_paths(home);
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        let original = br#"{"theme":"old","model":"x/y"}"#;
        std::fs::write(&paths.config, original).unwrap();

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
        let outcome = apply_to(&document, home);
        assert!(!outcome.backups.is_empty());
        let backup = outcome
            .backups
            .iter()
            .find(|path| path.ends_with("opencode.json"))
            .expect("opencode.json backup");
        assert_eq!(std::fs::read(backup).unwrap(), original);
        let root = read_json_object(&paths.config);
        assert_eq!(root.get("theme").and_then(Value::as_str), Some("old"));
    }
}
