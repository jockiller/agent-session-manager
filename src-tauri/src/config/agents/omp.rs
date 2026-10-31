use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::config::apply::{AgentPaths, ApplyContext, ApplyRequest};
use crate::config::endpoint;
use crate::config::fileio::{read_text, ApplyError};
use crate::config::models::{APIKind, ModelEntry, Provider};
use crate::config::yaml_text::{self, YamlValue};

use super::pi;

pub fn apply(
    request: &ApplyRequest,
    paths: &AgentPaths,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    write_models(request, &paths.config, ctx)?;
    write_config(request, &pi::sibling(&paths.config, "config.yml"), ctx)?;
    pi::apply_mcp(request, &pi::sibling(&paths.config, "mcp.json"), ctx)?;
    ctx.write_instructions(
        request.kind,
        &paths.instructions,
        &request.instructions,
        request.settings.instruction_on,
    )?;
    Ok(())
}

fn write_models(
    request: &ApplyRequest,
    path: &Path,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    let existing = read_text(path);
    let mut lines = yaml_lines(&existing);
    lines = yaml_text::trim_trailing_blank(lines);

    let urls = request.prune_urls();
    let ours: HashSet<&str> = request
        .providers
        .iter()
        .map(|provider| provider.id.as_str())
        .collect();
    let retired = request.retired_ids();
    let header = yaml_text::section_header("providers", &lines);
    let mut kept: Vec<yaml_text::Block> = Vec::new();
    let mut preserved: HashMap<String, Vec<String>> = HashMap::new();
    if let Some(header) = header {
        let range = (header + 1)..yaml_text::section_end(header, &lines);
        for block in yaml_text::child_blocks(range, &lines) {
            let replaced = ours.contains(block.key.as_str())
                || retired.contains(&block.key)
                || urls.contains(&base_url_of(&block));
            if replaced {
                preserved.insert(
                    block.key.clone(),
                    subtrees(&block, &["modelOverrides", "headers", "compat"]),
                );
            } else {
                kept.push(block);
            }
        }
    }

    let mut body: Vec<String> = Vec::new();
    for block in kept {
        body.extend(block.lines);
    }
    for provider in &request.providers {
        body.push(format!("  {}:", yaml_text::scalar_key(&provider.id)));
        let key = request.keys.get(&provider.id).cloned().unwrap_or_default();
        yaml_text::emit_children(&children(provider, &key), 4, &mut body);
        if let Some(extra) = preserved.get(&provider.id) {
            body.extend(extra.iter().cloned());
        }
    }
    lines = yaml_text::replace_section("providers", &lines, "", &body);
    write_yaml(ctx, request.kind.raw(), path, lines, 0o600)
}

fn write_config(
    request: &ApplyRequest,
    path: &Path,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    let existing = read_text(path);
    let mut lines = yaml_lines(&existing);
    let base = request.default_model_base();
    let effort = request.settings.effort.trim();

    let mut roles: Vec<(String, String)> = Vec::new();
    if let Some(header) = yaml_text::section_header("modelRoles", &lines) {
        let range = (header + 1)..yaml_text::section_end(header, &lines);
        for block in yaml_text::child_blocks(range, &lines) {
            if block.key == "default" {
                if !base.is_empty() {
                    roles.push((block.key, base.clone()));
                }
                continue;
            }
            if let Some(value) = yaml_text::value_of_child(&block.key, header, &lines) {
                roles.push((block.key, value));
            }
        }
    }
    if !base.is_empty() && !roles.iter().any(|(key, _)| key == "default") {
        roles.insert(0, ("default".into(), base));
    }
    let role_body: Vec<String> = roles
        .iter()
        .map(|(key, value)| format!("  {key}: {}", yaml_text::quote(value)))
        .collect();
    lines = yaml_text::replace_section("modelRoles", &lines, "", &role_body);

    if pi::is_exported_thinking_level(effort) {
        let line = format!("defaultThinkingLevel: {}", yaml_text::quote(effort));
        if let Some(header) = yaml_text::section_header("defaultThinkingLevel", &lines) {
            lines[header] = line;
        } else {
            lines.push(line);
        }
    } else if let Some(header) = yaml_text::section_header("defaultThinkingLevel", &lines) {
        lines.remove(header);
    }
    write_yaml(ctx, request.kind.raw(), path, lines, 0o644)
}

fn children(provider: &Provider, key: &str) -> Vec<(String, YamlValue)> {
    let YamlValue::Map(mut pairs) = yaml_provider(provider) else {
        return Vec::new();
    };
    if !key.is_empty() {
        pairs.push(("apiKey".into(), YamlValue::Scalar(key.to_string())));
    }
    if provider.api == APIKind::Anthropic {
        pairs.push(("auth".into(), YamlValue::Scalar("apiKey".into())));
    }
    pairs
}

fn yaml_provider(provider: &Provider) -> YamlValue {
    let mut pairs = vec![(
        "api".into(),
        YamlValue::Scalar(provider.api.pi_api().to_string()),
    )];
    let name = if provider.name.is_empty() {
        provider.id.as_str()
    } else {
        provider.name.as_str()
    };
    if name != provider.id {
        pairs.push(("name".into(), YamlValue::Scalar(name.to_string())));
    }
    if !provider.base_url.is_empty() {
        pairs.push(("baseUrl".into(), YamlValue::Scalar(pi::base_url(provider))));
    }
    let mut seen = HashSet::new();
    let models: Vec<YamlValue> = provider
        .models
        .iter()
        .filter_map(|model| {
            if seen.insert(model.wire_id().to_string()) {
                Some(yaml_model(model, provider.api))
            } else {
                None
            }
        })
        .collect();
    pairs.push(("models".into(), YamlValue::List(models)));
    YamlValue::Map(pairs)
}

fn yaml_model(model: &ModelEntry, api: APIKind) -> YamlValue {
    let mut pairs = vec![("id".into(), YamlValue::Scalar(model.wire_id().to_string()))];
    if !model.name.is_empty() && model.name != model.wire_id() {
        pairs.push(("name".into(), YamlValue::Scalar(model.name.clone())));
    }
    if let Some(context) = model.context {
        pairs.push(("contextWindow".into(), YamlValue::Int(context)));
    }
    if let Some(output) = model.output {
        pairs.push(("maxTokens".into(), YamlValue::Int(output)));
    }
    if let Some(cost) = &model.cost {
        if !cost.is_empty() {
            let cost = cost.normalized();
            pairs.push((
                "cost".into(),
                YamlValue::Map(vec![
                    ("input".into(), YamlValue::Float(cost.input.unwrap_or(0.0))),
                    ("output".into(), YamlValue::Float(cost.output.unwrap_or(0.0))),
                    (
                        "cacheRead".into(),
                        YamlValue::Float(cost.cache_read.unwrap_or(0.0)),
                    ),
                    (
                        "cacheWrite".into(),
                        YamlValue::Float(cost.cache_write.unwrap_or(0.0)),
                    ),
                ]),
            ));
        }
    }
    if !model.input.is_empty() {
        pairs.push((
            "input".into(),
            YamlValue::List(model.input.iter().cloned().map(YamlValue::Scalar).collect()),
        ));
    }
    if let Some(efforts) = &model.efforts {
        if efforts.is_empty() {
            pairs.push(("reasoning".into(), YamlValue::Bool(false)));
        } else {
            pairs.push(("reasoning".into(), YamlValue::Bool(true)));
            let levels = thinking_efforts(efforts);
            if !levels.is_empty() {
                pairs.push((
                    "thinking".into(),
                    YamlValue::Map(vec![
                        (
                            "mode".into(),
                            YamlValue::Scalar(thinking_mode(api, model.wire_id()).to_string()),
                        ),
                        (
                            "efforts".into(),
                            YamlValue::List(levels.into_iter().map(YamlValue::Scalar).collect()),
                        ),
                    ]),
                ));
            }
        }
    }
    YamlValue::Map(pairs)
}

fn thinking_mode(api: APIKind, wire_id: &str) -> &'static str {
    match api {
        APIKind::Google => {
            if is_gemini_3_plus(wire_id) {
                "google-level"
            } else {
                "budget"
            }
        }
        APIKind::Anthropic => {
            if pi::is_adaptive_thinking(wire_id) {
                "anthropic-adaptive"
            } else if is_opus_4_5(wire_id) {
                "anthropic-budget-effort"
            } else {
                "budget"
            }
        }
        APIKind::Responses | APIKind::Completions => "effort",
    }
}

fn is_opus_4_5(wire_id: &str) -> bool {
    let id = wire_id.to_lowercase();
    id.contains("opus-4-5") || id.contains("opus-4.5")
}

fn is_gemini_3_plus(wire_id: &str) -> bool {
    let id = wire_id.to_lowercase();
    let Some(rest) = id.split("gemini-").nth(1) else {
        return false;
    };
    rest.chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .parse::<u32>()
        .map(|major| major >= 3)
        .unwrap_or(false)
}

fn thinking_efforts(efforts: &[String]) -> Vec<String> {
    let valid: HashSet<&str> = pi::THINKING_LEVELS
        .iter()
        .copied()
        .filter(|level| *level != "off")
        .collect();
    let mut seen = HashSet::new();
    let mut levels = Vec::new();
    for level in efforts {
        if valid.contains(level.as_str()) && seen.insert(level.as_str()) {
            levels.push(level.clone());
        }
    }
    levels
}

fn base_url_of(block: &yaml_text::Block) -> String {
    for line in &block.lines {
        let body = yaml_text::trimmed(line);
        for prefix in ["baseUrl:", "base_url:", "baseURL:"] {
            if body.starts_with(prefix) {
                let raw = body[prefix.len()..].trim();
                return endpoint::normalize(&yaml_text::unquote(raw));
            }
        }
    }
    String::new()
}

fn subtrees(block: &yaml_text::Block, keys: &[&str]) -> Vec<String> {
    let mut captured = Vec::new();
    let mut index = 0;
    while index < block.lines.len() {
        let line = &block.lines[index];
        let body = yaml_text::trimmed(line);
        let level = yaml_text::indent_of(line);
        if level > 2 && keys.iter().any(|key| body.starts_with(&format!("{key}:"))) {
            let base = level;
            captured.push(line.clone());
            index += 1;
            while index < block.lines.len() {
                let next = &block.lines[index];
                if !yaml_text::is_blank(next) && yaml_text::indent_of(next) <= base {
                    break;
                }
                captured.push(next.clone());
                index += 1;
            }
            continue;
        }
        index += 1;
    }
    captured
}

fn yaml_lines(existing: &str) -> Vec<String> {
    if existing.is_empty() {
        Vec::new()
    } else {
        existing.split('\n').map(String::from).collect()
    }
}

fn write_yaml(
    ctx: &mut ApplyContext<'_>,
    agent: &str,
    path: &Path,
    lines: Vec<String>,
    mode: u32,
) -> Result<(), ApplyError> {
    let rendered = yaml_text::render(&yaml_text::trim_trailing_blank(lines));
    ctx.session.write(agent, path, rendered.as_bytes(), mode)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::apply::apply;
    use crate::config::models::{
        AgentKind, AgentSettings, AppDocument, MCPServer, MCPTransport, ModelCost, ModelEntry,
    };
    use std::collections::{BTreeMap, HashMap};
    use std::path::{Path, PathBuf};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_omp_{}_{}",
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
        document.set_settings(settings, AgentKind::Omp);
        document
    }

    fn apply_omp(
        document: &AppDocument,
        home: &Path,
        env: &HashMap<String, String>,
    ) -> crate::config::apply::ApplyOutcome {
        apply(
            document,
            AgentKind::Omp,
            home,
            env,
            &home.join(".asm-backups"),
        )
        .unwrap()
    }

    fn omp_paths(home: &Path, env: &HashMap<String, String>) -> AgentPaths {
        AgentPaths::live(AgentKind::Omp, home, "", env)
    }

    #[test]
    fn omp_apply_writes_yaml_providers_and_model_roles() {
        let dir = TestDir::new("omp_write");
        let home = dir.path();
        let empty = HashMap::new();
        apply_omp(
            &make_document("https://relay.example/v1", APIKind::Anthropic),
            home,
            &empty,
        );
        let paths = omp_paths(home, &empty);
        let models = std::fs::read_to_string(&paths.config).unwrap();
        assert!(models.contains("providers:"));
        assert!(models.contains("  claude:"));
        assert!(models.contains("api: anthropic-messages"));
        assert!(models.contains("baseUrl: https://relay.example"));
        assert!(models.contains("apiKey: sk-relay"));
        assert!(models.contains("auth: apiKey"));
        assert!(!models.contains("thinkingLevelMap:"));
        assert!(models.contains("thinking:"));
        assert!(models.contains("mode: budget"));
        assert!(models.contains("- low"));
        assert!(models.contains("- medium"));
        assert!(models.contains("- high"));
        assert!(models.contains("- xhigh"));
        assert!(models.contains("models:"));

        let config = std::fs::read_to_string(pi::sibling(&paths.config, "config.yml")).unwrap();
        assert!(config.contains("modelRoles:"));
        assert!(config.contains("default: claude/opus"));
        assert!(config.contains("defaultThinkingLevel: high"));

        let mcp = serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(pi::sibling(&paths.config, "mcp.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(mcp["mcpServers"]["dbx"]["command"], "dbx-mcp");
        assert_eq!(
            mcp["mcpServers"]["docs-server"]["url"],
            "https://mcp.example/sse"
        );
        assert_eq!(
            std::fs::read_to_string(&paths.instructions).unwrap(),
            "keep diffs small\n"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&paths.config), 0o600);
            assert_eq!(mode(&pi::sibling(&paths.config, "config.yml")), 0o644);
            assert_eq!(mode(&pi::sibling(&paths.config, "mcp.json")), 0o644);
        }
    }

    #[test]
    fn omp_apply_rewrites_blocks_and_preserves_user_subtrees() {
        let dir = TestDir::new("omp_rewrite");
        let home = dir.path();
        let empty = HashMap::new();
        let paths = omp_paths(home, &empty);
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        std::fs::write(
            &paths.config,
            r#"# my omp config
providers:
  claude:
    api: anthropic-messages
    baseUrl: "https://relay.example/v1/"
    headers:
      X-Old: "1"
    models:
      - id: stale
  local:
    api: openai-completions
    baseUrl: http://127.0.0.1:11434/v1
    models: []
otherSetting: keep
"#,
        )
        .unwrap();

        apply_omp(
            &make_document("https://relay.example/v1", APIKind::Anthropic),
            home,
            &empty,
        );
        let models = std::fs::read_to_string(&paths.config).unwrap();
        assert!(models.contains("# my omp config"));
        assert!(models.contains("otherSetting: keep"));
        assert!(!models.contains("stale"));
        assert!(models.contains("X-Old"));
        assert_eq!(models.split("headers:").count(), 2);
        assert!(models.contains("  local:"));
        assert!(models.contains("  claude:"));
        assert_eq!(models.split("  claude:").count(), 2);
    }

    #[test]
    fn apply_normalizes_pasted_endpoint_for_omp() {
        let dir = TestDir::new("omp_endpoint");
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
            document.set_settings(
                AgentSettings {
                    model: "claude/opus".into(),
                    effort: "high".into(),
                    ..AgentSettings::default()
                },
                AgentKind::Omp,
            );
            apply_omp(&document, home, &empty);
            let models = std::fs::read_to_string(&omp_paths(home, &empty).config).unwrap();
            assert!(models.contains("baseUrl: https://relay.invalid"));
            assert!(!models.contains("relay.invalid/v1"));
        }
    }

    #[test]
    fn omp_apply_respects_agent_directory_env() {
        let dir = TestDir::new("omp_env");
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let custom = dir.path().join("custom-omp");
        let env = HashMap::from([(
            "OMP_AGENT_DIR".into(),
            custom.to_string_lossy().into_owned(),
        )]);
        let paths = AgentPaths::live(AgentKind::Omp, &home, "", &env);
        assert_eq!(paths.config, custom.join("models.yml"));
        apply_omp(
            &make_document("https://relay.example/v1", APIKind::Anthropic),
            &home,
            &env,
        );
        assert!(custom.join("models.yml").exists());
        assert!(custom.join("config.yml").exists());
        assert!(custom.join("mcp.json").exists());
        assert!(!home.join(".omp/agent/models.yml").exists());
        assert_eq!(
            std::fs::read_to_string(home.join(".omp/AGENTS.md")).unwrap(),
            "keep diffs small\n"
        );
    }

    #[test]
    fn omp_apply_twice_writes_nothing() {
        let dir = TestDir::new("omp_idem");
        let home = dir.path();
        let empty = HashMap::new();
        let paths = omp_paths(home, &empty);
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        std::fs::write(
            &paths.config,
            r#"# my omp config
providers:
  claude:
    api: anthropic-messages
    baseUrl: "https://relay.example/v1/"
    modelOverrides:
      opus:
        temperature: 0.2
    headers:
      X-User: yes
    compat:
      keep: true
    models:
      - id: stale
  local:
    api: openai-completions
    baseUrl: http://127.0.0.1:11434/v1
    models: []
otherSetting: keep
"#,
        )
        .unwrap();
        std::fs::write(
            pi::sibling(&paths.config, "config.yml"),
            r#"# keep comment
modelRoles:
  default: old/model
  small: local/llama
otherSetting: keep
defaultThinkingLevel: low
"#,
        )
        .unwrap();
        std::fs::write(
            pi::sibling(&paths.config, "mcp.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "mcpServers": {
                    "keep-me": {"command": "keep", "args": []},
                    "dbx": {"command": "old-dbx", "args": []}
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let document = make_document("https://relay.example/v1", APIKind::Anthropic);
        let first = apply_omp(&document, home, &empty);
        assert!(!first.written.is_empty());
        let models = std::fs::read_to_string(&paths.config).unwrap();
        assert!(models.contains("# my omp config"));
        assert!(models.contains("otherSetting: keep"));
        assert!(models.contains("  local:"));
        assert!(models.contains("modelOverrides:"));
        assert!(models.contains("X-User"));
        assert!(models.contains("keep: true"));
        assert!(!models.contains("stale"));
        let config = std::fs::read_to_string(pi::sibling(&paths.config, "config.yml")).unwrap();
        assert!(config.contains("# keep comment"));
        assert!(config.contains("default: claude/opus"));
        assert!(config.contains("small: local/llama"));
        assert!(config.contains("otherSetting: keep"));
        assert!(config.contains("defaultThinkingLevel: high"));
        let mcp = serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(pi::sibling(&paths.config, "mcp.json")).unwrap(),
        )
        .unwrap();
        assert!(mcp["mcpServers"].get("keep-me").is_some());

        let second = apply_omp(&document, home, &empty);
        assert!(
            second.written.is_empty(),
            "second apply wrote {:?}",
            second.written
        );
    }

    #[test]
    fn omp_google_writes_thinking_mode_google_level() {
        let dir = TestDir::new("omp_google");
        let home = dir.path();
        let empty = HashMap::new();
        let mut document = AppDocument::default();
        document.providers = vec![Provider {
            id: "ggl".into(),
            name: "Gemini".into(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".into(),
            api: APIKind::Google,
            catalog_url: String::new(),
            api_key: "sk-google".into(),
            compat_base_url: String::new(),
            models: vec![
                ModelEntry {
                    efforts: Some(vec![
                        "minimal".into(),
                        "low".into(),
                        "medium".into(),
                        "high".into(),
                    ]),
                    ..ModelEntry::new("gemini-3-flash", "Gemini 3 Flash")
                },
                ModelEntry {
                    efforts: Some(vec!["low".into(), "high".into()]),
                    ..ModelEntry::new("gemini-3.8-flash", "Gemini 3.8 Flash")
                },
                ModelEntry {
                    efforts: Some(vec!["low".into(), "medium".into(), "high".into()]),
                    ..ModelEntry::new("gemini-4-argon", "Gemini 4 Argon")
                },
                ModelEntry {
                    efforts: Some(vec![
                        "minimal".into(),
                        "low".into(),
                        "medium".into(),
                        "high".into(),
                    ]),
                    ..ModelEntry::new("gemini-2.5-flash", "Gemini 2.5 Flash")
                },
            ],
        }];
        document.set_settings(
            AgentSettings {
                model: "ggl/gemini-3-flash".into(),
                effort: "medium".into(),
                ..AgentSettings::default()
            },
            AgentKind::Omp,
        );
        let first = apply_omp(&document, home, &empty);
        assert!(!first.written.is_empty());
        let models = std::fs::read_to_string(&omp_paths(home, &empty).config).unwrap();
        assert!(models.contains("api: google-generative-ai"));
        assert!(models.contains("baseUrl: https://generativelanguage.googleapis.com/v1beta"));
        assert!(!models.contains("thinkingLevelMap:"));
        assert!(models.contains("id: gemini-3-flash"));
        assert!(models.contains("id: gemini-3.8-flash"));
        assert!(models.contains("id: gemini-4-argon"));
        assert!(models.contains("id: gemini-2.5-flash"));
        assert_eq!(models.matches("mode: google-level").count(), 3);
        assert_eq!(models.matches("mode: budget").count(), 1);
        assert!(!models.contains("mode: effort"));
        let second = apply_omp(&document, home, &empty);
        assert!(
            second.written.is_empty(),
            "second apply wrote {:?}",
            second.written
        );
    }

    #[test]
    fn omp_thinking_mode_follows_protocol() {
        let dir = TestDir::new("omp_modes");
        let home = dir.path();
        let empty = HashMap::new();
        let cases = [
            (
                APIKind::Responses,
                "gpt-5",
                "https://relay.example/v1",
                "openai-responses",
                "mode: effort",
            ),
            (
                APIKind::Completions,
                "gpt-4o",
                "https://relay.example/v1",
                "openai-completions",
                "mode: effort",
            ),
            (
                APIKind::Anthropic,
                "claude-opus-4-6",
                "https://relay.example",
                "anthropic-messages",
                "mode: anthropic-adaptive",
            ),
            (
                APIKind::Anthropic,
                "claude-sonnet-4-5",
                "https://relay.example",
                "anthropic-messages",
                "mode: budget",
            ),
            (
                APIKind::Anthropic,
                "claude-opus-4-5",
                "https://relay.example",
                "anthropic-messages",
                "mode: anthropic-budget-effort",
            ),
            (
                APIKind::Anthropic,
                "claude-opus-4.5",
                "https://relay.example",
                "anthropic-messages",
                "mode: anthropic-budget-effort",
            ),
            (
                APIKind::Google,
                "gemini-3",
                "https://generativelanguage.googleapis.com/v1beta",
                "google-generative-ai",
                "mode: google-level",
            ),
        ];
        for (api, model_id, base_url, api_line, mode_line) in cases {
            let mut document = AppDocument::default();
            document.providers = vec![Provider {
                id: "p".into(),
                name: "P".into(),
                base_url: base_url.into(),
                api,
                catalog_url: String::new(),
                api_key: "sk".into(),
                compat_base_url: String::new(),
                models: vec![ModelEntry {
                    efforts: Some(vec!["low".into(), "medium".into(), "high".into()]),
                    ..ModelEntry::new(model_id, model_id)
                }],
            }];
            document.set_settings(
                AgentSettings {
                    model: format!("p/{model_id}"),
                    effort: "medium".into(),
                    ..AgentSettings::default()
                },
                AgentKind::Omp,
            );
            apply_omp(&document, home, &empty);
            let models = std::fs::read_to_string(&omp_paths(home, &empty).config).unwrap();
            assert!(models.contains(api_line), "{model_id}: {models}");
            assert!(models.contains(mode_line), "{model_id}: {models}");
            assert!(!models.contains("thinkingLevelMap:"), "{model_id}");
            assert!(models.contains("reasoning: true"), "{model_id}");
            assert!(models.contains("- low"), "{model_id}");
        }
    }

    #[test]
    fn omp_anthropic_opus_4_5_uses_budget_effort_not_adaptive() {
        let dir = TestDir::new("omp_opus45");
        let home = dir.path();
        let empty = HashMap::new();
        let mut document = AppDocument::default();
        document.providers = vec![Provider {
            id: "claude".into(),
            name: "Claude".into(),
            base_url: "https://relay.example".into(),
            api: APIKind::Anthropic,
            catalog_url: String::new(),
            api_key: "sk".into(),
            compat_base_url: String::new(),
            models: vec![
                ModelEntry {
                    efforts: Some(vec!["low".into(), "high".into()]),
                    ..ModelEntry::new("claude-opus-4-5", "Opus 4.5")
                },
                ModelEntry {
                    efforts: Some(vec!["low".into(), "high".into()]),
                    ..ModelEntry::new("claude-opus-4.5-thinking", "Opus 4.5 dotted")
                },
                ModelEntry {
                    efforts: Some(vec!["low".into(), "high".into()]),
                    ..ModelEntry::new("claude-opus-4-6", "Opus 4.6")
                },
                ModelEntry {
                    efforts: Some(vec!["low".into(), "high".into()]),
                    ..ModelEntry::new("claude-haiku-4-5", "Haiku 4.5")
                },
            ],
        }];
        document.set_settings(
            AgentSettings {
                model: "claude/claude-opus-4-5".into(),
                effort: "high".into(),
                ..AgentSettings::default()
            },
            AgentKind::Omp,
        );
        let first = apply_omp(&document, home, &empty);
        assert!(!first.written.is_empty());
        let models = std::fs::read_to_string(&omp_paths(home, &empty).config).unwrap();
        assert_eq!(models.matches("mode: anthropic-budget-effort").count(), 2, "{models}");
        assert_eq!(models.matches("mode: anthropic-adaptive").count(), 1, "{models}");
        assert_eq!(models.matches("mode: budget").count(), 1, "{models}");
        let second = apply_omp(&document, home, &empty);
        assert!(
            second.written.is_empty(),
            "second apply wrote {:?}",
            second.written
        );
    }

    #[test]
    fn yaml_model_writes_cost_with_all_rates() {
        let mut model = ModelEntry::new("flash", "Flash");
        model.cost = Some(ModelCost {
            input: Some(1.5),
            output: Some(7.5),
            cache_read: Some(0.15),
            cache_write: None,
        });
        let YamlValue::Map(pairs) = yaml_model(&model, APIKind::Completions) else {
            panic!("model is not a map");
        };
        let cost = pairs
            .iter()
            .find(|(key, _)| key == "cost")
            .map(|(_, value)| value)
            .expect("cost");
        let YamlValue::Map(rates) = cost else {
            panic!("cost is not a map");
        };
        let rate = |key: &str| {
            rates
                .iter()
                .find(|(name, _)| name == key)
                .and_then(|(_, value)| match value {
                    YamlValue::Float(number) => Some(*number),
                    _ => None,
                })
        };
        assert_eq!(rate("input"), Some(1.5));
        assert_eq!(rate("output"), Some(7.5));
        assert_eq!(rate("cacheRead"), Some(0.15));
        assert_eq!(rate("cacheWrite"), Some(0.0));
    }
}
