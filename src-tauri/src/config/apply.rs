use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::agents;
use super::display_name;
use super::endpoint;
use super::fileio::{
    expand_user_path, prune_backups, read_json_object, read_text, write_instruction_file,
    ApplyError, WriteSession,
};
use super::mcp_parser;
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
            AgentKind::Antigravity => {
                let root = env_directory(env, "GEMINI_HOME", home, ".gemini").join("config");
                Self {
                    config: root.join("mcp_config.json"),
                    instructions: root.join("AGENTS.md"),
                    extra: None,
                }
            }
            AgentKind::Claude => {
                if let Some(raw) = env.get("CLAUDE_CONFIG_DIR").filter(|s| !s.trim().is_empty()) {
                    let dir = expand_user_path(raw, home);
                    Self {
                        config: dir.join(".claude.json"),
                        instructions: dir.join("CLAUDE.md"),
                        extra: None,
                    }
                } else {
                    Self {
                        config: home.join(".claude.json"),
                        instructions: home.join(".claude/CLAUDE.md"),
                        extra: None,
                    }
                }
            }
            AgentKind::Codex => {
                let root = env_directory(env, "CODEX_HOME", home, ".codex");
                Self {
                    config: root.join("config.toml"),
                    instructions: root.join("AGENTS.md"),
                    extra: None,
                }
            }
            AgentKind::Gemini => {
                let root = env_directory(env, "GEMINI_HOME", home, ".gemini");
                Self {
                    config: root.join("settings.json"),
                    instructions: root.join("GEMINI.md"),
                    extra: None,
                }
            }
            AgentKind::Cursor => Self {
                config: home.join(".cursor/mcp.json"),
                instructions: PathBuf::new(),
                extra: None,
            },
            AgentKind::Qwen => {
                let root = env_directory(env, "QWEN_HOME", home, ".qwen");
                Self {
                    config: root.join("settings.json"),
                    instructions: root.join("QWEN.md"),
                    extra: None,
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyScope {
    Full,
    ProvidersOnly,
    McpOnly,
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
    pub managed_mcp_ids: Vec<String>,
    pub write_providers: bool,
    pub write_mcp: bool,
    pub write_instructions: bool,
}

impl ApplyRequest {
    pub fn managed_mcp_set(&self) -> HashSet<&str> {
        self.managed_mcp_ids
            .iter()
            .chain(self.settings.mcp_ids.iter())
            .map(String::as_str)
            .collect()
    }
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
        let effort = self.settings.effort_for(&base).trim();
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
    apply_request_with_scope(document, kind, ApplyScope::Full)
}

pub fn apply_request_with_scope(
    document: &AppDocument,
    kind: AgentKind,
    scope: ApplyScope,
) -> ApplyRequest {
    let settings = document.settings(kind);
    let catalog = document.catalog_refs();
    let mut filtered = Vec::new();
    let mut retired = Vec::new();
    let mut seen_ids = HashSet::new();
    for provider in &document.providers {
        seen_ids.insert(provider.id.clone());
        let mut copy = provider.clone();
        if !provider.enabled {
            copy.models = display_name::apply(&provider.models);
            retired.push(copy);
            continue;
        }
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
    for provider in &document.retired_providers {
        if !seen_ids.insert(provider.id.clone()) {
            continue;
        }
        let mut copy = provider.clone();
        copy.models = display_name::apply(&provider.models);
        retired.push(copy);
    }
    let mcp = document
        .library
        .mcp
        .iter()
        .filter(|server| settings.mcp_ids.contains(&server.id))
        .cloned()
        .collect();
    let managed_mcp_ids = document
        .library
        .mcp
        .iter()
        .map(|server| server.id.clone())
        .collect();
    let keys = filtered
        .iter()
        .map(|provider| (provider.id.clone(), provider.api_key.clone()))
        .collect();
    let (resolved_instructions, binding_enabled) = document.resolved_instructions_for(kind);
    let mut final_settings = settings;
    final_settings.instruction_on = final_settings.instruction_on && binding_enabled;
    let (write_providers, write_mcp, write_instructions) = match scope {
        ApplyScope::Full => (true, true, true),
        ApplyScope::ProvidersOnly => (true, false, false),
        ApplyScope::McpOnly => (false, true, false),
    };
    ApplyRequest {
        kind,
        providers: filtered,
        retired,
        keys,
        settings: final_settings,
        instructions: resolved_instructions,
        mcp,
        managed_mcp_ids,
        write_providers,
        write_mcp,
        write_instructions,
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
    apply_with_scope(document, kind, ApplyScope::Full, home, env, backups_root)
}

pub fn apply_with_scope(
    document: &AppDocument,
    kind: AgentKind,
    scope: ApplyScope,
    home: &Path,
    env: &HashMap<String, String>,
    backups_root: &Path,
) -> Result<ApplyOutcome, ApplyError> {
    let request = apply_request_with_scope(document, kind, scope);
    let paths = AgentPaths::live(kind, home, &request.settings.profile, env);
    let mut session = WriteSession::new(backups_root, home);
    {
        let mut ctx = ApplyContext {
            home,
            env,
            session: &mut session,
        };
        apply_with_session(&request, &paths, &mut ctx)?;
    }
    if !session.backups.is_empty() {
        prune_backups(backups_root, 20)?;
    }
    Ok(ApplyOutcome {
        written: session.written,
        backups: session.backups,
    })
}

fn agent_mcp_file_exists(kind: AgentKind, paths: &AgentPaths) -> bool {
    match kind {
        AgentKind::Pi | AgentKind::Omp => agents::pi::sibling(&paths.config, "mcp.json").exists(),
        AgentKind::Zcode => {
            paths.config.exists()
                || paths
                    .config
                    .parent()
                    .and_then(Path::parent)
                    .is_some_and(|root| root.join("cli/config.json").exists())
        }
        _ => paths.config.exists(),
    }
}

pub fn sync_mcp_all(
    document: &AppDocument,
    extra_managed_mcp_ids: &[String],
    home: &Path,
    env: &HashMap<String, String>,
    backups_root: &Path,
) -> Result<ApplyOutcome, ApplyError> {
    let mut session = WriteSession::new(backups_root, home);
    {
        let mut ctx = ApplyContext {
            home,
            env,
            session: &mut session,
        };
        for kind in AgentKind::ALL {
            let mut request = apply_request_with_scope(document, kind, ApplyScope::McpOnly);
            for id in extra_managed_mcp_ids {
                if !request.managed_mcp_ids.iter().any(|existing| existing == id) {
                    request.managed_mcp_ids.push(id.clone());
                }
            }
            let paths = AgentPaths::live(kind, home, &request.settings.profile, env);
            if request.mcp.is_empty() && !agent_mcp_file_exists(kind, &paths) {
                continue;
            }
            if kind == AgentKind::Dsh
                && !paths.instructions.parent().is_some_and(|p| p.exists())
                && request.mcp.is_empty()
            {
                continue;
            }
            apply_with_session(&request, &paths, &mut ctx)?;
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredDiskMcp {
    pub server: MCPServer,
    pub agents: Vec<String>,
    pub in_library: bool,
}

pub fn scan_agent_mcp(kind: AgentKind, paths: &AgentPaths) -> Vec<MCPServer> {
    match kind {
        AgentKind::Antigravity
        | AgentKind::Claude
        | AgentKind::Gemini
        | AgentKind::Cursor
        | AgentKind::Qwen => {
            mcp_parser::parse_agent_mcp_json(&read_json_object(&paths.config), &["mcpServers"])
        }
        AgentKind::Pi | AgentKind::Omp => {
            let mcp_path = agents::pi::sibling(&paths.config, "mcp.json");
            mcp_parser::parse_agent_mcp_json(&read_json_object(&mcp_path), &["mcpServers"])
        }
        AgentKind::Opencode => {
            mcp_parser::parse_agent_mcp_json(&read_json_object(&paths.config), &["mcp"])
        }
        AgentKind::Zcode => {
            let mut list =
                mcp_parser::parse_agent_mcp_json(&read_json_object(&paths.config), &["mcp"]);
            let seen: HashSet<String> = list.iter().map(|s| s.id.clone()).collect();
            if let Some(cli) = paths
                .config
                .parent()
                .and_then(Path::parent)
                .map(|root| root.join("cli/config.json"))
            {
                if let Some(serde_json::Value::Object(mcp_obj)) = read_json_object(&cli).get("mcp") {
                    for srv in mcp_parser::parse_agent_mcp_json(mcp_obj, &["servers"]) {
                        if !seen.contains(&srv.id) {
                            list.push(srv);
                        }
                    }
                }
            }
            list.sort_by(|a, b| a.id.cmp(&b.id));
            list
        }
        AgentKind::Codex | AgentKind::Grok => {
            mcp_parser::parse_agent_mcp_toml(&read_text(&paths.config))
        }
        AgentKind::Dsh => agents::dsh::scan_mcp(&read_text(&paths.config)),
    }
}

fn match_library_mcp_id(kind: AgentKind, disk_id: &str, library: &[MCPServer]) -> Option<String> {
    library.iter().find_map(|lib| {
        let matches = lib.id == disk_id
            || ((kind == AgentKind::Pi || kind == AgentKind::Omp)
                && agents::pi::sanitize_mcp_name(&lib.id) == disk_id)
            || (kind == AgentKind::Dsh && agents::dsh::sanitize_mcp(&lib.id) == disk_id);
        matches.then(|| lib.id.clone())
    })
}

pub fn reconcile_disk_mcp(
    document: &mut AppDocument,
    home: &Path,
    env: &HashMap<String, String>,
) -> bool {
    document.ensure_agent_defaults();
    let mut changed = false;
    for kind in AgentKind::ALL {
        let profile = document.settings(kind).profile;
        let paths = AgentPaths::live(kind, home, &profile, env);
        let disk_servers = scan_agent_mcp(kind, &paths);
        if disk_servers.is_empty() {
            continue;
        }
        let mut matched_ids = Vec::new();
        for disk_srv in &disk_servers {
            if let Some(lib_id) = match_library_mcp_id(kind, &disk_srv.id, &document.library.mcp) {
                matched_ids.push(lib_id);
            }
        }
        if let Some(settings) = document.agents.get_mut(kind.raw()) {
            for lib_id in matched_ids {
                if !settings.mcp_ids.contains(&lib_id) {
                    settings.mcp_ids.push(lib_id);
                    changed = true;
                }
            }
        }
    }
    changed
}

pub fn scan_all_disk_mcp(
    document: &AppDocument,
    home: &Path,
    env: &HashMap<String, String>,
) -> Vec<DiscoveredDiskMcp> {
    let mut by_id: BTreeMap<String, DiscoveredDiskMcp> = BTreeMap::new();
    let library_ids: HashSet<&str> = document.library.mcp.iter().map(|s| s.id.as_str()).collect();
    for kind in AgentKind::ALL {
        let profile = document.settings(kind).profile;
        let paths = AgentPaths::live(kind, home, &profile, env);
        for mut srv in scan_agent_mcp(kind, &paths) {
            let normalized_id = match_library_mcp_id(kind, &srv.id, &document.library.mcp)
                .unwrap_or_else(|| srv.id.clone());
            srv.id = normalized_id.clone();
            let in_library = library_ids.contains(normalized_id.as_str());
            let entry = by_id.entry(normalized_id).or_insert_with(|| DiscoveredDiskMcp {
                server: srv,
                agents: Vec::new(),
                in_library,
            });
            let raw_kind = kind.raw().to_string();
            if !entry.agents.contains(&raw_kind) {
                entry.agents.push(raw_kind);
            }
        }
    }
    by_id.into_values().collect()
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
        AgentKind::Antigravity
        | AgentKind::Claude
        | AgentKind::Codex
        | AgentKind::Gemini
        | AgentKind::Cursor
        | AgentKind::Qwen => agents::mcp_only::apply(request, paths, ctx),
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
                enabled: true,
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
                enabled: true,
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
                enabled: true,
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
                enabled: true,
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

    #[test]
    fn apply_retires_disabled_and_deleted_providers_on_export() {
        let dir = TestDir::new("disabled_and_retired");
        let home = dir.path();
        let paths = AgentPaths::live(AgentKind::Opencode, home, "", &HashMap::new());
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        std::fs::write(
            &paths.config,
            r#"{
  "providers": {
    "a": { "npm": "@ai-sdk/openai", "options": { "baseURL": "https://a.example/v1" }, "models": { "m1": { "name": "M1" } } },
    "disabled_p": { "npm": "@ai-sdk/openai", "options": { "baseURL": "https://dis.example/v1" }, "models": { "d1": { "name": "D1" } } },
    "deleted_p": { "npm": "@ai-sdk/openai", "options": { "baseURL": "https://del.example/v1" }, "models": { "x1": { "name": "X1" } } },
    "foreign": { "npm": "@ai-sdk/anthropic", "options": { "baseURL": "https://foreign.example/v1" }, "models": { "f1": { "name": "F1" } } }
  },
  "model": "disabled_p/d1"
}"#,
        )
        .unwrap();

        let mut document = AppDocument::default();
        let mut active = Provider::new("a", "A", "https://a.example/v1");
        active.api_key = "sk-a".into();
        active.models = vec![ModelEntry::new("m1", "M1")];

        let mut disabled = Provider::new("disabled_p", "Disabled", "https://dis.example/v1");
        disabled.enabled = false;
        disabled.models = vec![ModelEntry::new("d1", "D1")];

        let mut deleted = Provider::new("deleted_p", "Deleted", "https://del.example/v1");
        deleted.models = vec![ModelEntry::new("x1", "X1")];

        document.providers = vec![active, disabled];
        document.retired_providers = vec![deleted];
        document.set_settings(
            AgentSettings {
                model: "a/m1".into(),
                ..AgentSettings::default()
            },
            AgentKind::Opencode,
        );

        let backups = home.join("asm-backups");
        apply(&document, AgentKind::Opencode, home, &HashMap::new(), &backups).unwrap();
        let exported: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&paths.config).unwrap()).unwrap();
        let providers = exported["providers"].as_object().unwrap();
        assert!(providers.contains_key("a"));
        assert!(providers.contains_key("foreign"));
        assert!(!providers.contains_key("disabled_p"));
        assert!(!providers.contains_key("deleted_p"));
        assert_eq!(exported["model"], "a/m1");
    }

    #[test]
    fn providers_only_preserves_mcp_and_instructions() {
        let dir = TestDir::new("providers_only_iso");
        let home = dir.path();
        let empty = HashMap::new();
        let backups = home.join("asm-backups");

        // Pre-populate Opencode, Grok, Pi, DSH, ZCode with existing MCP & AGENTS.md
        let oc = AgentPaths::live(AgentKind::Opencode, home, "", &empty);
        std::fs::create_dir_all(oc.config.parent().unwrap()).unwrap();
        std::fs::write(&oc.config, r#"{"mcp":{"keep_oc":{"type":"local","command":["npx","oc"]}}}"#).unwrap();
        std::fs::write(&oc.instructions, "# Keep OpenCode Rules\n").unwrap();

        let grok = AgentPaths::live(AgentKind::Grok, home, "", &empty);
        std::fs::create_dir_all(grok.config.parent().unwrap()).unwrap();
        std::fs::write(&grok.config, "[mcp_servers.foo]\ncommand = \"npx\"\n").unwrap();
        std::fs::write(&grok.instructions, "# Keep Grok Rules\n").unwrap();

        let pi = AgentPaths::live(AgentKind::Pi, home, "", &empty);
        std::fs::create_dir_all(pi.config.parent().unwrap()).unwrap();
        std::fs::create_dir_all(pi.instructions.parent().unwrap()).unwrap();
        let pi_mcp = pi.config.parent().unwrap().join("mcp.json");
        std::fs::write(&pi_mcp, r#"{"mcpServers":{"keep_pi":{"command":"npx"}}}"#).unwrap();
        std::fs::write(&pi.instructions, "# Keep Pi Rules\n").unwrap();

        let dsh = AgentPaths::live(AgentKind::Dsh, home, "web", &empty);
        std::fs::create_dir_all(dsh.config.parent().unwrap()).unwrap();
        std::fs::create_dir_all(dsh.instructions.parent().unwrap()).unwrap();
        std::fs::write(&dsh.config, "- insert:\n  - id: mcp-keep\n    name: dsh-mcp-client\n    config:\n      serverName: keep\n").unwrap();
        std::fs::write(&dsh.instructions, "# Keep DSH Rules\n").unwrap();

        let zcode = AgentPaths::live(AgentKind::Zcode, home, "", &empty);
        std::fs::create_dir_all(zcode.config.parent().unwrap()).unwrap();
        std::fs::create_dir_all(zcode.instructions.parent().unwrap()).unwrap();
        std::fs::write(&zcode.config, r#"{"mcp":{"keep_zc":{"type":"local","command":["npx"]}}}"#).unwrap();
        std::fs::write(&zcode.instructions, "# Keep ZCode Rules\n").unwrap();

        let mut doc = AppDocument::default();
        let mut p = Provider::new("p1", "P1", "https://api.example/v1");
        p.api_key = "sk-test".into();
        p.models = vec![ModelEntry::new("m1", "M1")];
        doc.providers = vec![p];
        doc.library.instructions = "# Overwrite attempt".into();
        doc.library.mcp = vec![MCPServer {
            id: "keep_oc".into(),
            transport: crate::config::models::MCPTransport::Stdio,
            command: "node".into(),
            args: vec![],
            url: String::new(),
            env: BTreeMap::new(),
        }];

        for kind in [
            AgentKind::Opencode,
            AgentKind::Grok,
            AgentKind::Pi,
            AgentKind::Dsh,
            AgentKind::Zcode,
        ] {
            apply_with_scope(&doc, kind, ApplyScope::ProvidersOnly, home, &empty, &backups).unwrap();
        }

        let oc_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&oc.config).unwrap()).unwrap();
        assert!(oc_json["providers"]["p1"].is_object());
        assert!(oc_json["mcp"]["keep_oc"].is_object());
        assert_eq!(std::fs::read_to_string(&oc.instructions).unwrap(), "# Keep OpenCode Rules\n");

        let grok_text = std::fs::read_to_string(&grok.config).unwrap();
        assert!(grok_text.contains("[model.\"m1\"]"));
        assert!(grok_text.contains("[mcp_servers.foo]"));
        assert_eq!(std::fs::read_to_string(&grok.instructions).unwrap(), "# Keep Grok Rules\n");

        assert!(std::fs::read_to_string(&pi_mcp).unwrap().contains("keep_pi"));
        assert_eq!(std::fs::read_to_string(&pi.instructions).unwrap(), "# Keep Pi Rules\n");

        assert!(std::fs::read_to_string(&dsh.config).unwrap().contains("id: mcp-keep"));
        assert_eq!(std::fs::read_to_string(&dsh.instructions).unwrap(), "# Keep DSH Rules\n");

        let zc_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&zcode.config).unwrap()).unwrap();
        assert!(zc_json["provider"]["p1"].is_object());
        assert!(zc_json["mcp"]["keep_zc"].is_object());
        assert_eq!(std::fs::read_to_string(&zcode.instructions).unwrap(), "# Keep ZCode Rules\n");
    }

    #[test]
    fn mcp_only_sync_all_preserves_providers_and_instructions_and_prunes_unchecked() {
        let dir = TestDir::new("mcp_only_iso");
        let home = dir.path();
        let empty = HashMap::new();
        let backups = home.join("asm-backups");

        let oc = AgentPaths::live(AgentKind::Opencode, home, "", &empty);
        std::fs::create_dir_all(oc.config.parent().unwrap()).unwrap();
        std::fs::write(
            &oc.config,
            r#"{"providers":{"orig":{"npm":"@ai-sdk/openai"}},"model":"orig/m1"}"#,
        )
        .unwrap();
        std::fs::write(&oc.instructions, "# Orig OpenCode\n").unwrap();

        let agy = AgentPaths::live(AgentKind::Antigravity, home, "", &empty);
        std::fs::create_dir_all(agy.config.parent().unwrap()).unwrap();
        std::fs::write(&agy.config, r#"{"mcpServers":{"foreign":{"command":"uvx"}}}"#).unwrap();
        std::fs::write(&agy.instructions, "# Orig Agy\n").unwrap();

        let claude = AgentPaths::live(AgentKind::Claude, home, "", &empty);
        let raw_claude = "{\n  \"mcpServers\": {\n    \"custom\": {\"command\": \"echo\"}\n  }\n}\n";
        std::fs::write(&claude.config, raw_claude).unwrap();

        let mut doc = AppDocument::default();
        doc.library.instructions = "# Should not be written by sync_mcp_all".into();
        doc.library.mcp = vec![MCPServer {
            id: "demo".into(),
            transport: crate::config::models::MCPTransport::Stdio,
            command: "npx".into(),
            args: vec!["demo-mcp".into()],
            url: String::new(),
            env: BTreeMap::new(),
        }];
        doc.set_settings(
            AgentSettings {
                mcp_ids: vec!["demo".into()],
                ..AgentSettings::default()
            },
            AgentKind::Opencode,
        );
        doc.set_settings(
            AgentSettings {
                mcp_ids: vec!["demo".into()],
                ..AgentSettings::default()
            },
            AgentKind::Antigravity,
        );

        let first = sync_mcp_all(&doc, &[], home, &empty, &backups).unwrap();
        assert!(first.written.contains(&oc.config));
        assert!(first.written.contains(&agy.config));
        assert!(!first.written.contains(&claude.config));

        let oc_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&oc.config).unwrap()).unwrap();
        assert_eq!(oc_json["model"], "orig/m1");
        assert!(oc_json["providers"]["orig"].is_object());
        assert!(oc_json["mcp"]["demo"].is_object());
        assert_eq!(std::fs::read_to_string(&oc.instructions).unwrap(), "# Orig OpenCode\n");

        let agy_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&agy.config).unwrap()).unwrap();
        assert!(agy_json["mcpServers"]["demo"].is_object());
        assert!(agy_json["mcpServers"]["foreign"].is_object());
        assert_eq!(std::fs::read_to_string(&agy.instructions).unwrap(), "# Orig Agy\n");
        assert_eq!(std::fs::read_to_string(&claude.config).unwrap(), raw_claude);

        // Uncheck demo from Antigravity and sync again: demo must be pruned while foreign stays
        doc.set_settings(AgentSettings::default(), AgentKind::Antigravity);
        let second = sync_mcp_all(&doc, &[], home, &empty, &backups).unwrap();
        assert!(second.written.contains(&agy.config));
        let agy_after: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&agy.config).unwrap()).unwrap();
        assert!(!agy_after["mcpServers"].as_object().unwrap().contains_key("demo"));
        assert!(agy_after["mcpServers"]["foreign"].is_object());
    }

    #[test]
    fn reconcile_disk_mcp_discovers_antigravity_and_respects_disabled() {
        let dir = TestDir::new("reconcile_disk_mcp");
        let home = dir.path();
        let empty = HashMap::new();

        let agy = AgentPaths::live(AgentKind::Antigravity, home, "", &empty);
        std::fs::create_dir_all(agy.config.parent().unwrap()).unwrap();
        std::fs::write(
            &agy.config,
            r#"{
  "mcpServers": {
    "dbx": { "command": "/Users/test/.dbx/bin/dbx-mcp", "disabled": false },
    "off": { "command": "npx", "disabled": true },
    "foreign-only": { "command": "uvx" }
  }
}"#,
        )
        .unwrap();

        let codex = AgentPaths::live(AgentKind::Codex, home, "", &empty);
        std::fs::create_dir_all(codex.config.parent().unwrap()).unwrap();
        std::fs::write(
            &codex.config,
            "[models]\ndefault = \"gpt-5\"\n\n[mcp_servers.dbx]\ncommand = \"dbx-mcp\"\n",
        )
        .unwrap();

        let mut doc = AppDocument::default();
        doc.library.mcp = vec![
            MCPServer {
                id: "dbx".into(),
                transport: crate::config::models::MCPTransport::Stdio,
                command: "/Users/test/.dbx/bin/dbx-mcp".into(),
                args: vec![],
                url: String::new(),
                env: BTreeMap::new(),
            },
            MCPServer {
                id: "off".into(),
                transport: crate::config::models::MCPTransport::Stdio,
                command: "npx".into(),
                args: vec![],
                url: String::new(),
                env: BTreeMap::new(),
            },
        ];

        assert!(reconcile_disk_mcp(&mut doc, home, &empty));
        assert_eq!(doc.settings(AgentKind::Antigravity).mcp_ids, vec!["dbx".to_string()]);
        assert_eq!(doc.settings(AgentKind::Codex).mcp_ids, vec!["dbx".to_string()]);
        assert!(!doc.settings(AgentKind::Antigravity).mcp_ids.contains(&"foreign-only".to_string()));
        assert!(!reconcile_disk_mcp(&mut doc, home, &empty));
    }

    #[test]
    fn scan_all_disk_mcp_groups_agents_and_marks_in_library() {
        let dir = TestDir::new("scan_all_disk_mcp");
        let home = dir.path();
        let empty = HashMap::new();

        let agy = AgentPaths::live(AgentKind::Antigravity, home, "", &empty);
        std::fs::create_dir_all(agy.config.parent().unwrap()).unwrap();
        std::fs::write(
            &agy.config,
            r#"{"mcpServers":{"dbx":{"command":"dbx-mcp","disabled":false},"extra":{"url":"https://e.example/mcp"}}}"#,
        )
        .unwrap();

        let oc = AgentPaths::live(AgentKind::Opencode, home, "", &empty);
        std::fs::create_dir_all(oc.config.parent().unwrap()).unwrap();
        std::fs::write(
            &oc.config,
            r#"{"mcp":{"dbx":{"type":"local","command":["dbx-mcp"]}}}"#,
        )
        .unwrap();

        let mut doc = AppDocument::default();
        doc.library.mcp = vec![MCPServer {
            id: "dbx".into(),
            transport: crate::config::models::MCPTransport::Stdio,
            command: "dbx-mcp".into(),
            args: vec![],
            url: String::new(),
            env: BTreeMap::new(),
        }];

        let found = scan_all_disk_mcp(&doc, home, &empty);
        let dbx = found.iter().find(|item| item.server.id == "dbx").unwrap();
        assert!(dbx.in_library);
        assert!(dbx.agents.contains(&"antigravity".to_string()));
        assert!(dbx.agents.contains(&"opencode".to_string()));

        let extra = found.iter().find(|item| item.server.id == "extra").unwrap();
        assert!(!extra.in_library);
        assert_eq!(extra.agents, vec!["antigravity".to_string()]);
    }
}
