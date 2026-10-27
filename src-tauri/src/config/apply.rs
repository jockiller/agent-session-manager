use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::agents;
use super::display_name;
use super::endpoint;
use super::fileio::{
    expand_user_path, prune_backups, write_instruction_file, ApplyError, WriteSession,
};
use super::models::{
    sanitize_dsh_profile, APIKind, AgentKind, AgentSettings, AppDocument, MCPServer, ModelEntry,
    Provider, model_ref,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPaths {
    pub config: PathBuf,
    pub instructions: PathBuf,
    pub extra: Option<PathBuf>,
}

fn env_directory(env: &HashMap<String, String>, key: &str, home: &Path, fallback: &str) -> PathBuf {
    if let Some(raw) = env.get(key) {
        if !raw.trim().is_empty() {
            return expand_user_path(raw, home);
        }
    }
    home.join(fallback)
}

impl AgentPaths {
    pub fn live(kind: AgentKind, home: &Path, profile: &str, env: &HashMap<String, String>) -> Self {
        match kind {
            AgentKind::Opencode => Self {
                config: home.join(".config/opencode/opencode.json"),
                instructions: home.join(".config/opencode/AGENTS.md"),
                extra: None,
            },
            AgentKind::Dsh => {
                let name = sanitize_dsh_profile(profile);
                Self {
                    config: home.join(format!(".dsh/profiles/{name}/cordis.patch.yml")),
                    instructions: home.join(".dsh/AGENTS.md"),
                    extra: Some(home.join(".dsh/.env")),
                }
            }
            AgentKind::Zcode => Self {
                config: home.join(".zcode/v2/config.json"),
                instructions: home.join(".zcode/AGENTS.md"),
                extra: Some(home.join(".zcode/v2/provider_config.json")),
            },
            AgentKind::Grok => Self {
                config: home.join(".grok/config.toml"),
                instructions: home.join(".grok/AGENTS.md"),
                extra: None,
            },
            AgentKind::Pi => {
                let root = env_directory(env, "PI_CODING_AGENT_DIR", home, ".pi/agent");
                Self {
                    config: root.join("models.json"),
                    instructions: home.join(".pi/AGENTS.md"),
                    extra: None,
                }
            }
            AgentKind::Omp => {
                let root = env_directory(env, "OMP_AGENT_DIR", home, ".omp/agent");
                Self {
                    config: root.join("models.yml"),
                    instructions: home.join(".omp/AGENTS.md"),
                    extra: None,
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApplyRequest {
    pub kind: AgentKind,
    pub providers: Vec<Provider>,
    pub retired: Vec<Provider>,
    pub keys: BTreeMap<String, String>,
    pub settings: AgentSettings,
    pub instructions: String,
    pub mcp: Vec<MCPServer>,
}

impl ApplyRequest {
    pub fn prune_urls(&self) -> HashSet<String> {
        let mut combined = self.providers.clone();
        combined.extend(self.retired.iter().cloned());
        endpoint::incoming_urls(&combined)
    }

    pub fn retired_ids(&self) -> HashSet<String> {
        self.retired.iter().map(|provider| provider.id.clone()).collect()
    }

    pub fn references_retired_model(&self, r#ref: &str) -> bool {
        let base = r#ref.split_once('#').map(|(head, _)| head).unwrap_or(r#ref);
        let mut parts = base.splitn(2, '/');
        let Some(provider_id) = parts.next() else {
            return false;
        };
        if parts.next().is_none() {
            return false;
        }
        self.retired_ids().contains(provider_id)
    }

    pub fn clears_default(&self, current: &str) -> bool {
        let trimmed = current.trim();
        if trimmed.is_empty() {
            return false;
        }
        self.providers.is_empty() || self.references_retired_model(trimmed)
    }

    pub fn catalog_refs(&self) -> Vec<String> {
        self.providers
            .iter()
            .flat_map(|provider| {
                provider
                    .models
                    .iter()
                    .map(|model| model_ref(&provider.id, &model.id))
            })
            .collect()
    }

    pub fn default_model_base(&self) -> String {
        let wanted = self.settings.model.trim();
        let catalog = self.catalog_refs();
        if !wanted.is_empty() && catalog.iter().any(|item| item == wanted) {
            return wanted.to_string();
        }
        catalog.first().cloned().unwrap_or_default()
    }

    pub fn default_model_ref(&self) -> String {
        let base = self.default_model_base();
        if base.is_empty() {
            return String::new();
        }
        let effort = self.settings.effort.trim();
        if !effort.is_empty() {
            if let Some(model) = self.model(&base) {
                if model.efforts.as_ref().is_some_and(|list| list.iter().any(|item| item == effort)) {
                    return format!("{base}#{effort}");
                }
            }
        }
        base
    }

    pub fn model(&self, r#ref: &str) -> Option<&ModelEntry> {
        let core = r#ref.split_once('#').map(|(head, _)| head).unwrap_or(r#ref);
        let (provider_id, model_id) = core.split_once('/')?;
        self.providers
            .iter()
            .find(|provider| provider.id == provider_id)?
            .models
            .iter()
            .find(|model| model.id == model_id)
    }

    pub fn default_provider_id(&self) -> String {
        let base = self.default_model_base();
        let parts: Vec<&str> = base.splitn(2, '/').collect();
        if parts.len() == 2 {
            parts[0].to_string()
        } else {
            self.providers
                .first()
                .map(|provider| provider.id.clone())
                .unwrap_or_default()
        }
    }

    pub fn default_bare_model_id(&self) -> String {
        let base = self.default_model_base();
        let parts: Vec<&str> = base.splitn(2, '/').collect();
        if parts.len() == 2 {
            parts[1].to_string()
        } else {
            self.providers
                .first()
                .and_then(|provider| provider.models.first())
                .map(|model| model.id.clone())
                .unwrap_or_default()
        }
    }
}

const GOOGLE_THINKING_LEVELS: &[&str] = &["minimal", "low", "medium", "high"];

fn filter_google_efforts(provider: &mut Provider) {
    if provider.api != APIKind::Google {
        return;
    }
    for model in &mut provider.models {
        let Some(efforts) = &model.efforts else {
            continue;
        };
        if efforts.is_empty() {
            continue;
        }
        model.efforts = Some(
            efforts
                .iter()
                .filter(|item| GOOGLE_THINKING_LEVELS.contains(&item.as_str()))
                .cloned()
                .collect(),
        );
    }
}

pub fn apply_request(document: &AppDocument, kind: AgentKind) -> ApplyRequest {
    let settings = document.settings(kind);
    let catalog = document.catalog_refs();
    let mut filtered = Vec::new();
    let mut retired = Vec::new();
    for provider in &document.providers {
        let mut copy = provider.clone();
        let models: Vec<ModelEntry> = provider
            .models
            .iter()
            .filter(|model| settings.is_picked(&model_ref(&provider.id, &model.id), &catalog))
            .cloned()
            .collect();
        copy.models = display_name::apply(&models);
        if copy.models.is_empty() {
            retired.push(copy);
            continue;
        }
        if kind.supports(provider.api) {
            filter_google_efforts(&mut copy);
            filtered.push(copy);
            continue;
        }
        if provider.api == APIKind::Google && !provider.compat_base_url.trim().is_empty() {
            filter_google_efforts(&mut copy);
            copy.api = APIKind::Completions;
            copy.base_url = provider.compat_base_url.trim().to_string();
            filtered.push(copy);
            continue;
        }
        retired.push(copy);
    }
    let mcp = document
        .library
        .mcp
        .iter()
        .filter(|server| settings.mcp_ids.contains(&server.id))
        .cloned()
        .collect();
    let keys = filtered
        .iter()
        .map(|provider| (provider.id.clone(), provider.api_key.clone()))
        .collect();
    ApplyRequest {
        kind,
        providers: filtered,
        retired,
        keys,
        settings,
        instructions: document.library.instructions.clone(),
        mcp,
    }
}

pub struct ApplyContext<'a> {
    pub home: &'a Path,
    pub env: &'a HashMap<String, String>,
    pub session: &'a mut WriteSession,
}

impl ApplyContext<'_> {
    pub fn write_instructions(
        &mut self,
        kind: AgentKind,
        path: &Path,
        instructions: &str,
        on: bool,
    ) -> Result<bool, ApplyError> {
        write_instruction_file(self.session, kind.raw(), path, instructions, on)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApplyOutcome {
    pub written: Vec<PathBuf>,
    pub backups: Vec<PathBuf>,
}

pub fn apply(
    document: &AppDocument,
    kind: AgentKind,
    home: &Path,
    env: &HashMap<String, String>,
    backups_root: &Path,
) -> Result<ApplyOutcome, ApplyError> {
    let request = apply_request(document, kind);
    let paths = AgentPaths::live(kind, home, &request.settings.profile, env);
    let mut session = WriteSession::new(backups_root, home);
    {
        let mut ctx = ApplyContext {
            home,
            env,
            session: &mut session,
        };
        match kind {
            AgentKind::Opencode => agents::opencode::apply(&request, &paths, &mut ctx)?,
            AgentKind::Zcode => agents::zcode::apply(&request, &paths, &mut ctx)?,
            AgentKind::Grok => agents::grok::apply(&request, &paths, &mut ctx)?,
            AgentKind::Dsh => agents::dsh::apply(&request, &paths, &mut ctx)?,
            AgentKind::Pi => agents::pi::apply(&request, &paths, &mut ctx)?,
            AgentKind::Omp => agents::omp::apply(&request, &paths, &mut ctx)?,
        }
    }
    if !session.backups.is_empty() {
        prune_backups(backups_root, 20)?;
    }
    Ok(ApplyOutcome {
        written: session.written,
        backups: session.backups,
    })
}

pub fn apply_with_session(
    request: &ApplyRequest,
    paths: &AgentPaths,
    ctx: &mut ApplyContext<'_>,
) -> Result<(), ApplyError> {
    match request.kind {
        AgentKind::Opencode => agents::opencode::apply(request, paths, ctx),
        AgentKind::Zcode => agents::zcode::apply(request, paths, ctx),
        AgentKind::Grok => agents::grok::apply(request, paths, ctx),
        AgentKind::Dsh => agents::dsh::apply(request, paths, ctx),
        AgentKind::Pi => agents::pi::apply(request, paths, ctx),
        AgentKind::Omp => agents::omp::apply(request, paths, ctx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::{APIKind, AgentKind, AgentSettings, AppDocument, ModelEntry, Provider};

    #[test]
    fn agent_paths_live_and_env_override() {
        let home = PathBuf::from("/tmp/home");
        let empty = HashMap::new();
        let paths = AgentPaths::live(AgentKind::Opencode, &home, "", &empty);
        assert_eq!(
            paths.config,
            PathBuf::from("/tmp/home/.config/opencode/opencode.json")
        );
        let dsh = AgentPaths::live(AgentKind::Dsh, &home, "Prod West", &empty);
        assert_eq!(
            dsh.config,
            PathBuf::from("/tmp/home/.dsh/profiles/Prod-West/cordis.patch.yml")
        );
        let env = HashMap::from([("PI_CODING_AGENT_DIR".into(), "~/custom-pi".into())]);
        let pi = AgentPaths::live(AgentKind::Pi, &home, "", &env);
        assert_eq!(pi.config, PathBuf::from("/tmp/home/custom-pi/models.json"));
    }

    #[test]
    fn apply_request_carries_retired_providers() {
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
        let request = apply_request(&document, AgentKind::Zcode);
        assert_eq!(
            request.providers.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["a"]
        );
        assert_eq!(
            request.retired.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["b"]
        );
        assert!(request.prune_urls().contains("https://b.example/v1"));
        assert_eq!(request.retired_ids(), HashSet::from(["b".to_string()]));
    }

    fn google_and_openai_document(default_model: &str) -> AppDocument {
        let mut flash = ModelEntry::new("flash", "Flash");
        flash.efforts = Some(vec![
            "xhigh".into(),
            "high".into(),
            "max".into(),
            "minimal".into(),
            "low".into(),
        ]);
        let mut nothink = ModelEntry::new("nothi", "NoThink");
        nothink.efforts = Some(vec![]);
        let mut only_xhigh = ModelEntry::new("onlyx", "OnlyX");
        only_xhigh.efforts = Some(vec!["xhigh".into(), "max".into()]);
        let mut document = AppDocument::default();
        document.providers = vec![
            Provider {
                id: "g".into(),
                name: "Gemini".into(),
                base_url: "https://generativelanguage.googleapis.com".into(),
                api: APIKind::Google,
                catalog_url: String::new(),
                api_key: "sk-google".into(),
                compat_base_url: String::new(),
                models: vec![flash, nothink, only_xhigh],
            },
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
        ];
        for kind in AgentKind::ALL {
            document.set_settings(
                AgentSettings {
                    model: default_model.into(),
                    ..AgentSettings::default()
                },
                kind,
            );
        }
        document
    }

    #[test]
    fn apply_request_excludes_unsupported_google_and_filters_efforts() {
        let document = google_and_openai_document("g/flash");
        for kind in [AgentKind::Zcode, AgentKind::Dsh, AgentKind::Grok] {
            let request = apply_request(&document, kind);
            assert_eq!(
                request.providers.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
                ["a"],
                "{kind:?}"
            );
            assert_eq!(
                request.retired.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
                ["g"],
                "{kind:?}"
            );
            assert!(!request.keys.contains_key("g"), "{kind:?}");
            assert!(request.keys.contains_key("a"), "{kind:?}");
            assert_eq!(request.default_model_base(), "a/m1", "{kind:?}");
        }
        for kind in [AgentKind::Opencode, AgentKind::Pi, AgentKind::Omp] {
            let request = apply_request(&document, kind);
            let google = request
                .providers
                .iter()
                .find(|provider| provider.id == "g")
                .expect("google exported");
            assert!(!request.retired.iter().any(|provider| provider.id == "g"));
            assert_eq!(
                google.models[0].efforts,
                Some(vec!["high".into(), "minimal".into(), "low".into()])
            );
            assert_eq!(google.models[1].efforts, Some(Vec::new()));
            assert_eq!(google.models[2].efforts, Some(Vec::new()));
            assert_eq!(request.default_model_base(), "g/flash", "{kind:?}");
            assert_eq!(google.api, APIKind::Google);
            assert_eq!(
                google.base_url,
                "https://generativelanguage.googleapis.com"
            );
        }
    }

    #[test]
    fn apply_request_exports_google_compat_for_non_native_and_ignores_for_native() {
        let mut document = google_and_openai_document("g/flash");
        document.providers[0].compat_base_url = " https://relay.x/v1 ".into();
        for kind in [AgentKind::Zcode, AgentKind::Dsh, AgentKind::Grok] {
            let request = apply_request(&document, kind);
            let google = request
                .providers
                .iter()
                .find(|provider| provider.id == "g")
                .expect("compat exported");
            assert!(!request.retired.iter().any(|provider| provider.id == "g"));
            assert_eq!(google.api, APIKind::Completions, "{kind:?}");
            assert_eq!(google.base_url, "https://relay.x/v1", "{kind:?}");
            assert_eq!(
                google.models[0].efforts,
                Some(vec!["high".into(), "minimal".into(), "low".into()])
            );
            assert_eq!(google.models[1].efforts, Some(Vec::new()));
            assert_eq!(google.models[2].efforts, Some(Vec::new()));
            assert!(request.keys.contains_key("g"), "{kind:?}");
            assert_eq!(request.default_model_base(), "g/flash", "{kind:?}");
        }
        for kind in [AgentKind::Opencode, AgentKind::Pi, AgentKind::Omp] {
            let request = apply_request(&document, kind);
            let google = request
                .providers
                .iter()
                .find(|provider| provider.id == "g")
                .expect("native google");
            assert_eq!(google.api, APIKind::Google, "{kind:?}");
            assert_eq!(
                google.base_url,
                "https://generativelanguage.googleapis.com",
                "{kind:?}"
            );
        }
    }

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_apply_{}_{}",
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

    #[test]
    fn apply_removes_google_provider_from_non_native_agent() {
        let dir = TestDir::new("google_retire");
        let home = dir.path();
        let paths = AgentPaths::live(AgentKind::Zcode, home, "", &HashMap::new());
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        std::fs::write(
            &paths.config,
            r#"{
                "provider": {
                    "g": {
                        "kind": "openai-compatible",
                        "options": {
                            "baseURL": "https://generativelanguage.googleapis.com/v1beta",
                            "apiKey": "old-google"
                        }
                    }
                }
            }"#,
        )
        .unwrap();

        let document = google_and_openai_document("a/m1");
        apply(
            &document,
            AgentKind::Zcode,
            home,
            &HashMap::new(),
            &home.join("asm-backups"),
        )
        .unwrap();

        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&paths.config).unwrap()).unwrap();
        let providers = root
            .get("provider")
            .and_then(serde_json::Value::as_object)
            .unwrap();
        assert!(providers.get("g").is_none());
        assert!(providers.get("a").is_some());
        let text = std::fs::read_to_string(&paths.config).unwrap();
        assert!(!text.contains("generativelanguage.googleapis.com"));
        assert!(!text.contains("google-generative-ai"));
        assert!(!text.contains("old-google"));
    }

    #[test]
    fn apply_writes_google_compat_for_grok_and_is_idempotent() {
        let dir = TestDir::new("google_compat_grok");
        let home = dir.path();
        let paths = AgentPaths::live(AgentKind::Grok, home, "", &HashMap::new());
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        std::fs::write(
            &paths.config,
            r#"
[model."flash"]
model = "flash"
name = "Old"
base_url = "https://generativelanguage.googleapis.com/v1beta"
api_key = "old-google"
api_backend = "chat_completions"
"#,
        )
        .unwrap();

        let mut document = google_and_openai_document("g/flash");
        document.providers[0].compat_base_url = "https://relay.x/v1".into();
        let backups = home.join("asm-backups");
        let first = apply(
            &document,
            AgentKind::Grok,
            home,
            &HashMap::new(),
            &backups,
        )
        .unwrap();
        assert!(!first.written.is_empty());
        let text = std::fs::read_to_string(&paths.config).unwrap();
        assert!(text.contains("base_url = \"https://relay.x/v1\""));
        assert!(text.contains("api_backend = \"chat_completions\""));
        assert!(text.contains("api_key = \"sk-google\""));
        assert!(!text.contains("generativelanguage.googleapis.com"));
        assert!(text.contains("[model.\"flash\"]"));
        assert!(text.contains("[model.\"m1\"]"));

        let second = apply(
            &document,
            AgentKind::Grok,
            home,
            &HashMap::new(),
            &backups,
        )
        .unwrap();
        assert!(second.written.is_empty());
        assert_eq!(std::fs::read_to_string(&paths.config).unwrap(), text);
    }
}
