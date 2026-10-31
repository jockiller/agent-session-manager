use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use super::display_name;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Opencode,
    Dsh,
    Zcode,
    Grok,
    Pi,
    Omp,
}

impl AgentKind {
    pub const ALL: [AgentKind; 6] = [
        AgentKind::Opencode,
        AgentKind::Dsh,
        AgentKind::Zcode,
        AgentKind::Grok,
        AgentKind::Pi,
        AgentKind::Omp,
    ];

    pub fn from_raw(raw: &str) -> Option<Self> {
        match raw {
            "opencode" => Some(Self::Opencode),
            "dsh" => Some(Self::Dsh),
            "zcode" => Some(Self::Zcode),
            "grok" => Some(Self::Grok),
            "pi" => Some(Self::Pi),
            "omp" => Some(Self::Omp),
            _ => None,
        }
    }

    pub fn raw(self) -> &'static str {
        match self {
            Self::Opencode => "opencode",
            Self::Dsh => "dsh",
            Self::Zcode => "zcode",
            Self::Grok => "grok",
            Self::Pi => "pi",
            Self::Omp => "omp",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Opencode => "OpenCode",
            Self::Dsh => "DSH",
            Self::Zcode => "ZCode",
            Self::Grok => "Grok",
            Self::Pi => "Pi",
            Self::Omp => "omp",
        }
    }

    pub fn config_hint(self) -> String {
        self.resolved_hint("")
    }

    pub fn resolved_hint(self, profile: &str) -> String {
        match self {
            Self::Opencode => "~/.config/opencode/opencode.json".to_string(),
            Self::Dsh => format!(
                "~/.dsh/profiles/{}/cordis.patch.yml",
                sanitize_dsh_profile(profile)
            ),
            Self::Zcode => "~/.zcode/v2/config.json".to_string(),
            Self::Grok => "~/.grok/config.toml".to_string(),
            Self::Pi => "~/.pi/agent/models.json".to_string(),
            Self::Omp => "~/.omp/agent/models.yml".to_string(),
        }
    }

    pub fn supports(self, api: APIKind) -> bool {
        match api {
            APIKind::Google => matches!(self, Self::Opencode | Self::Pi | Self::Omp),
            _ => true,
        }
    }
}

pub const DSH_PROFILE_FALLBACK: &str = "web";

pub fn sanitize_dsh_profile(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DSH_PROFILE_FALLBACK.to_string();
    }
    let mapped: String = trimmed
        .chars()
        .map(|character| {
            if character.is_alphabetic() || character.is_numeric() || character == '-' || character == '_'
            {
                character
            } else {
                '-'
            }
        })
        .collect();
    let value = mapped.trim_matches('-').to_string();
    if value.is_empty() {
        DSH_PROFILE_FALLBACK.to_string()
    } else {
        value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum APIKind {
    #[default]
    Responses,
    Completions,
    Anthropic,
    Google,
}

impl APIKind {
    pub const ALL: [APIKind; 4] = [
        APIKind::Responses,
        APIKind::Completions,
        APIKind::Anthropic,
        APIKind::Google,
    ];

    pub fn raw(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::Completions => "completions",
            Self::Anthropic => "anthropic",
            Self::Google => "google",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Responses => "OpenAI Responses",
            Self::Completions => "Chat Completions",
            Self::Anthropic => "Anthropic Messages",
            Self::Google => "Google",
        }
    }

    pub fn open_code_package(self) -> &'static str {
        match self {
            Self::Responses => "aisdk:@ai-sdk/openai",
            Self::Completions => "aisdk:@ai-sdk/openai-compatible",
            Self::Anthropic => "aisdk:@ai-sdk/anthropic",
            Self::Google => "aisdk:@ai-sdk/google",
        }
    }

    pub fn zcode_kind(self) -> &'static str {
        match self {
            Self::Responses => "openai",
            Self::Completions => "openai-compatible",
            Self::Anthropic => "anthropic",
            Self::Google => "openai-compatible",
        }
    }

    pub fn grok_backend(self) -> &'static str {
        match self {
            Self::Anthropic => "messages",
            _ => "chat_completions",
        }
    }

    pub fn pi_api(self) -> &'static str {
        match self {
            Self::Responses => "openai-responses",
            Self::Completions => "openai-completions",
            Self::Anthropic => "anthropic-messages",
            Self::Google => "google-generative-ai",
        }
    }

    pub fn zcode_api_type(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic-messages",
            Self::Responses => "openai-responses",
            Self::Completions | Self::Google => "openai-chat-completions",
        }
    }

    pub fn dsh_api(self) -> &'static str {
        match self {
            Self::Responses => "openai-responses",
            Self::Completions => "openai-completions",
            Self::Anthropic => "anthropic-messages",
            Self::Google => "google-generative-ai",
        }
    }
}

fn default_true() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

/// USD per million tokens, the unit every supported agent's config uses.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCost {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<f64>,
}

impl ModelCost {
    pub fn is_empty(&self) -> bool {
        self.input.is_none()
            && self.output.is_none()
            && self.cache_read.is_none()
            && self.cache_write.is_none()
    }

    pub fn normalized(&self) -> Self {
        Self {
            input: Some(self.input.unwrap_or(0.0)),
            output: Some(self.output.unwrap_or(0.0)),
            cache_read: Some(self.cache_read.unwrap_or(0.0)),
            cache_write: Some(self.cache_write.unwrap_or(0.0)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "ModelEntryDe")]
pub struct ModelEntry {
    pub id: String,
    #[serde(rename = "requestId", default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub efforts: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<ModelCost>,
    #[serde(
        rename = "toolCall",
        default = "default_true",
        skip_serializing_if = "is_true"
    )]
    pub tool_call: bool,
}

impl ModelEntry {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        let id = id.into();
        let name = name.into();
        Self {
            id,
            request_id: None,
            name,
            context: None,
            output: None,
            input: Vec::new(),
            efforts: None,
            cost: None,
            tool_call: true,
        }
    }

    pub fn wire_id(&self) -> &str {
        self.request_id.as_deref().unwrap_or(&self.id)
    }

    pub fn profile_summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(context) = self.context {
            parts.push(Self::format_tokens(context));
        }
        let mods = if self.input.is_empty() {
            vec!["text".to_string()]
        } else {
            self.input.clone()
        };
        parts.push(mods.join("/"));
        if let Some(efforts) = &self.efforts {
            parts.push(if efforts.is_empty() {
                "无思考".to_string()
            } else {
                efforts.join(", ")
            });
        }
        parts.join(" · ")
    }

    pub fn format_tokens(value: i64) -> String {
        if value >= 1_000_000 && value % 1_000_000 == 0 {
            format!("{}M", value / 1_000_000)
        } else if value >= 1000 && value % 1000 == 0 {
            format!("{}k", value / 1000)
        } else {
            format!("{value}")
        }
    }
}

#[derive(Deserialize)]
struct ModelEntryDe {
    id: String,
    #[serde(rename = "requestId", default)]
    request_id: Option<String>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    context: Option<i64>,
    #[serde(default)]
    output: Option<i64>,
    #[serde(default)]
    input: Vec<String>,
    #[serde(default)]
    efforts: Option<Vec<String>>,
    #[serde(default)]
    cost: Option<ModelCost>,
    #[serde(rename = "toolCall", default = "default_true")]
    tool_call: bool,
}

impl From<ModelEntryDe> for ModelEntry {
    fn from(de: ModelEntryDe) -> Self {
        let name = if de.name.is_empty() {
            de.id.clone()
        } else {
            de.name
        };
        Self {
            id: de.id,
            request_id: de.request_id,
            name,
            context: de.context,
            output: de.output,
            input: de.input,
            efforts: de.efforts,
            cost: de.cost,
            tool_call: de.tool_call,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "baseURL", default)]
    pub base_url: String,
    #[serde(default)]
    pub api: APIKind,
    #[serde(rename = "catalogURL", default)]
    pub catalog_url: String,
    #[serde(rename = "apiKey", default)]
    pub api_key: String,
    #[serde(
        rename = "compatBaseURL",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub compat_base_url: String,
    #[serde(default)]
    pub models: Vec<ModelEntry>,
}

impl Provider {
    pub fn exportable_to(&self, kind: AgentKind) -> bool {
        kind.supports(self.api)
            || (self.api == APIKind::Google && !self.compat_base_url.trim().is_empty())
    }

    pub fn new(id: impl Into<String>, name: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            base_url: base_url.into(),
            api: APIKind::Responses,
            catalog_url: String::new(),
            api_key: String::new(),
            compat_base_url: String::new(),
            models: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MCPTransport {
    #[default]
    Stdio,
    Http,
    Sse,
}

impl MCPTransport {
    pub fn raw(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MCPServer {
    pub id: String,
    #[serde(default)]
    pub transport: MCPTransport,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl MCPServer {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            transport: MCPTransport::Stdio,
            command: String::new(),
            args: Vec::new(),
            url: String::new(),
            env: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Library {
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub mcp: Vec<MCPServer>,
}

fn default_effort() -> String {
    "high".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSettings {
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_effort")]
    pub effort: String,
    /// Per-model thinking level. Agents that cannot write it ignore the map.
    #[serde(
        rename = "effortByModel",
        default,
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub effort_by_model: BTreeMap<String, String>,
    #[serde(rename = "smallModel", default)]
    pub small_model: String,
    #[serde(rename = "pickedModelIDs", default)]
    pub picked_model_ids: Vec<String>,
    #[serde(rename = "instructionOn", default = "default_true")]
    pub instruction_on: bool,
    #[serde(rename = "mcpIDs", default)]
    pub mcp_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub profile: String,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            model: String::new(),
            effort: "high".to_string(),
            effort_by_model: BTreeMap::new(),
            small_model: String::new(),
            picked_model_ids: Vec::new(),
            instruction_on: true,
            mcp_ids: Vec::new(),
            profile: String::new(),
        }
    }
}

impl AgentSettings {
    pub fn defaults_for(_kind: AgentKind) -> Self {
        Self::default()
    }

    pub fn is_picked(&self, r#ref: &str, _catalog: &[String]) -> bool {
        if self.picked_model_ids.is_empty() {
            return true;
        }
        self.picked_model_ids.iter().any(|id| id == r#ref)
    }

    pub fn set_picked(&mut self, r#ref: &str, on: bool, catalog: &[String]) {
        let mut selected: HashSet<String> = if self.picked_model_ids.is_empty() {
            catalog.iter().cloned().collect()
        } else {
            self.picked_model_ids.iter().cloned().collect()
        };
        if on {
            selected.insert(r#ref.to_string());
        } else {
            selected.remove(r#ref);
        }
        if !catalog.is_empty() && catalog.iter().all(|item| selected.contains(item)) {
            self.picked_model_ids.clear();
        } else {
            self.picked_model_ids = catalog
                .iter()
                .filter(|item| selected.contains(*item))
                .cloned()
                .collect();
        }
        if !self.model.is_empty() && !self.is_picked(&self.model, catalog) {
            self.model.clear();
        }
        if !self.small_model.is_empty() && !self.is_picked(&self.small_model, catalog) {
            self.small_model.clear();
        }
        if !self.picked_model_ids.is_empty() {
            let picked = self.picked_model_ids.clone();
            self.effort_by_model
                .retain(|r#ref, _| picked.iter().any(|id| id == r#ref));
        }
    }

    pub fn effort_for(&self, r#ref: &str) -> &str {
        self.effort_by_model
            .get(r#ref)
            .map(|value| value.as_str())
            .unwrap_or(self.effort.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelChoice {
    pub r#ref: String,
    pub title: String,
}

impl ModelChoice {
    pub fn new(r#ref: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            r#ref: r#ref.into(),
            title: title.into(),
        }
    }

    pub fn id(&self) -> &str {
        &self.r#ref
    }
}

pub fn model_ref(provider_id: &str, model_id: &str) -> String {
    format!("{provider_id}/{model_id}")
}

fn known_agent_map<'de, D, T>(deserializer: D) -> Result<BTreeMap<String, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    let raw = BTreeMap::<String, serde_json::Value>::deserialize(deserializer)?;
    let mut out = BTreeMap::new();
    for (key, value) in raw {
        if AgentKind::from_raw(&key).is_none() {
            continue;
        }
        let parsed = T::deserialize(value).map_err(serde::de::Error::custom)?;
        out.insert(key, parsed);
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppDocument {
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default)]
    pub providers: Vec<Provider>,
    #[serde(default)]
    pub library: Library,
    #[serde(default, deserialize_with = "known_agent_map")]
    pub agents: BTreeMap<String, AgentSettings>,
    #[serde(rename = "appliedAt", default, deserialize_with = "known_agent_map")]
    pub applied_at: BTreeMap<String, String>,
}

fn default_version() -> i64 {
    1
}

impl Default for AppDocument {
    fn default() -> Self {
        Self {
            version: 1,
            providers: Vec::new(),
            library: Library::default(),
            agents: BTreeMap::new(),
            applied_at: BTreeMap::new(),
        }
    }
}

impl AppDocument {
    pub fn ensure_agent_defaults(&mut self) {
        for kind in AgentKind::ALL {
            self.agents
                .entry(kind.raw().to_string())
                .or_insert_with(|| AgentSettings::defaults_for(kind));
        }
        self.pretty_model_names();
    }

    pub fn pretty_model_names(&mut self) {
        for provider in &mut self.providers {
            provider.models = display_name::apply(&provider.models);
        }
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

    pub fn picked_choices(&self, kind: AgentKind) -> Vec<ModelChoice> {
        let settings = self.settings(kind);
        let catalog = self.catalog_refs();
        let multi = self
            .providers
            .iter()
            .filter(|provider| provider.exportable_to(kind) && !provider.models.is_empty())
            .count()
            > 1;
        let mut choices = Vec::new();
        for provider in &self.providers {
            if !provider.exportable_to(kind) {
                continue;
            }
            let provider_name = if provider.name.is_empty() {
                provider.id.as_str()
            } else {
                provider.name.as_str()
            };
            for model in &provider.models {
                let r#ref = model_ref(&provider.id, &model.id);
                if !settings.is_picked(&r#ref, &catalog) {
                    continue;
                }
                let title = if multi {
                    format!("{} · {}", model.name, provider_name)
                } else {
                    model.name.clone()
                };
                choices.push(ModelChoice::new(r#ref, title));
            }
        }
        choices
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

    pub fn settings(&self, kind: AgentKind) -> AgentSettings {
        self.agents
            .get(kind.raw())
            .cloned()
            .unwrap_or_else(|| AgentSettings::defaults_for(kind))
    }

    pub fn set_settings(&mut self, settings: AgentSettings, kind: AgentKind) {
        self.agents.insert(kind.raw().to_string(), settings);
    }

    pub fn rename_provider_refs(&mut self, old_id: &str, new_id: &str) {
        if old_id == new_id || old_id.is_empty() || new_id.is_empty() {
            return;
        }
        let prefix = format!("{old_id}/");
        let remap = |r#ref: &str| -> String {
            if let Some(rest) = r#ref.strip_prefix(&prefix) {
                format!("{new_id}/{rest}")
            } else {
                r#ref.to_string()
            }
        };
        for kind in AgentKind::ALL {
            let mut settings = self.settings(kind);
            settings.model = remap(&settings.model);
            settings.small_model = remap(&settings.small_model);
            settings.picked_model_ids = settings
                .picked_model_ids
                .iter()
                .map(|id| remap(id))
                .collect();
            settings.effort_by_model = settings
                .effort_by_model
                .iter()
                .map(|(key, value)| (remap(key), value.clone()))
                .collect();
            self.set_settings(settings, kind);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffortLevel {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl EffortLevel {
    pub const ALL: [EffortLevel; 5] = [
        EffortLevel::Low,
        EffortLevel::Medium,
        EffortLevel::High,
        EffortLevel::Xhigh,
        EffortLevel::Max,
    ];

    pub fn raw(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SidebarItem {
    Agents,
    Providers,
    Library,
}

impl SidebarItem {
    pub fn raw(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Providers => "providers",
            Self::Library => "library",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Agents => "Agents",
            Self::Providers => "Providers",
            Self::Library => "Library",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_kind_does_not_include_codex() {
        assert_eq!(
            AgentKind::ALL.map(|kind| kind.raw()),
            ["opencode", "dsh", "zcode", "grok", "pi", "omp"]
        );
    }

    #[test]
    fn agent_kind_supports_api_matrix() {
        for kind in AgentKind::ALL {
            for api in APIKind::ALL {
                let expected = !matches!(
                    (kind, api),
                    (
                        AgentKind::Zcode | AgentKind::Dsh | AgentKind::Grok,
                        APIKind::Google
                    )
                );
                assert_eq!(kind.supports(api), expected, "{kind:?} {api:?}");
            }
        }
    }

    #[test]
    fn picked_choices_skips_unsupported_providers() {
        let mut document = AppDocument::default();
        document.providers = vec![
            Provider {
                id: "g".into(),
                name: "Gemini".into(),
                base_url: "https://generativelanguage.googleapis.com".into(),
                api: APIKind::Google,
                catalog_url: String::new(),
                api_key: String::new(),
                compat_base_url: String::new(),
                models: vec![ModelEntry::new("gemini-flash", "Flash")],
            },
            Provider {
                id: "a".into(),
                name: "OpenAI".into(),
                base_url: "https://example.invalid/v1".into(),
                api: APIKind::Responses,
                catalog_url: String::new(),
                api_key: String::new(),
                compat_base_url: String::new(),
                models: vec![ModelEntry::new("m1", "M1")],
            },
        ];
        let grok = document
            .picked_choices(AgentKind::Grok)
            .into_iter()
            .map(|choice| choice.r#ref)
            .collect::<Vec<_>>();
        assert_eq!(grok, ["a/m1"]);
        let opencode = document
            .picked_choices(AgentKind::Opencode)
            .into_iter()
            .map(|choice| choice.r#ref)
            .collect::<Vec<_>>();
        assert_eq!(opencode, ["g/gemini-flash", "a/m1"]);

        document.providers[0].compat_base_url = "https://relay.x/v1".into();
        let grok_compat = document
            .picked_choices(AgentKind::Grok)
            .into_iter()
            .map(|choice| choice.r#ref)
            .collect::<Vec<_>>();
        assert_eq!(grok_compat, ["g/gemini-flash", "a/m1"]);
    }

    #[test]
    fn empty_picked_means_all_selected() {
        let mut settings = AgentSettings::default();
        let catalog = ["g/flash".to_string(), "g/pro".to_string()];
        assert!(settings.is_picked("g/flash", &catalog));
        assert!(settings.is_picked("g/pro", &catalog));
        settings.set_picked("g/pro", false, &catalog);
        assert_eq!(settings.picked_model_ids, ["g/flash"]);
        assert!(!settings.is_picked("g/pro", &catalog));
        settings.set_picked("g/pro", true, &catalog);
        assert!(settings.picked_model_ids.is_empty());
    }

    #[test]
    fn unchecking_default_clears_it_and_choices_follow_picks() {
        let mut document = AppDocument::default();
        document.providers = vec![Provider {
            id: "g".into(),
            name: "GJ".into(),
            base_url: "https://example.invalid/v1".into(),
            api: APIKind::Responses,
            catalog_url: String::new(),
            api_key: String::new(),
            compat_base_url: String::new(),
            models: vec![
                ModelEntry::new("flash", "Flash"),
                ModelEntry::new("pro", "Pro"),
            ],
        }];
        let mut settings = AgentSettings {
            model: "g/pro".into(),
            ..AgentSettings::default()
        };
        settings.set_picked("g/pro", false, &document.catalog_refs());
        assert_eq!(settings.model, "");
        document.set_settings(settings, AgentKind::Opencode);
        assert_eq!(
            document
                .picked_choices(AgentKind::Opencode)
                .iter()
                .map(|choice| choice.r#ref.as_str())
                .collect::<Vec<_>>(),
            ["g/flash"]
        );
    }

    #[test]
    fn provider_rename_rewrites_model_refs() {
        let mut document = AppDocument::default();
        document.set_settings(
            AgentSettings {
                model: "gj/flash".into(),
                picked_model_ids: vec!["gj/flash".into()],
                ..AgentSettings::default()
            },
            AgentKind::Opencode,
        );
        document.rename_provider_refs("gj", "gja");
        let settings = document.settings(AgentKind::Opencode);
        assert_eq!(settings.model, "gja/flash");
        assert_eq!(settings.picked_model_ids, ["gja/flash"]);
    }

    #[test]
    fn effort_for_falls_back_and_drops_unpicked() {
        let catalog = vec!["g/flash".into(), "g/pro".into()];
        let mut settings = AgentSettings {
            effort: "high".into(),
            ..AgentSettings::default()
        };
        settings
            .effort_by_model
            .insert("g/flash".into(), "low".into());
        settings
            .effort_by_model
            .insert("g/pro".into(), "xhigh".into());
        assert_eq!(settings.effort_for("g/flash"), "low");
        assert_eq!(settings.effort_for("g/other"), "high");
        settings.set_picked("g/pro", false, &catalog);
        assert_eq!(
            settings.effort_by_model.keys().cloned().collect::<Vec<_>>(),
            ["g/flash"]
        );
    }

    #[test]
    fn provider_rename_rewrites_effort_by_model() {
        let mut document = AppDocument::default();
        let mut settings = AgentSettings {
            model: "gj/flash".into(),
            picked_model_ids: vec!["gj/flash".into()],
            ..AgentSettings::default()
        };
        settings
            .effort_by_model
            .insert("gj/flash".into(), "low".into());
        document.set_settings(settings, AgentKind::Opencode);
        document.rename_provider_refs("gj", "gja");
        let settings = document.settings(AgentKind::Opencode);
        assert_eq!(
            settings
                .effort_by_model
                .get("gja/flash")
                .map(String::as_str),
            Some("low")
        );
        assert!(settings.effort_by_model.get("gj/flash").is_none());
    }

    #[test]
    fn model_entry_encode_rules() {
        let mut model = ModelEntry::new("flash", "Flash");
        let json = serde_json::to_value(&model).unwrap();
        assert!(json.get("input").is_none());
        assert!(json.get("toolCall").is_none());
        assert!(json.get("efforts").is_none());

        model.tool_call = false;
        model.input = vec!["text".into()];
        model.efforts = Some(vec![]);
        let json = serde_json::to_value(&model).unwrap();
        assert_eq!(json.get("toolCall"), Some(&serde_json::Value::Bool(false)));
        assert_eq!(json.get("input"), Some(&serde_json::json!(["text"])));
        assert_eq!(json.get("efforts"), Some(&serde_json::json!([])));
    }

    #[test]
    fn model_cost_roundtrips_camel_case() {
        let mut model = ModelEntry::new("flash", "Flash");
        model.cost = Some(ModelCost {
            input: Some(0.75),
            output: Some(3.75),
            cache_read: Some(0.075),
            cache_write: None,
        });
        let json = serde_json::to_value(&model).unwrap();
        let cost = json.get("cost").and_then(serde_json::Value::as_object).unwrap();
        assert_eq!(cost.get("input"), Some(&serde_json::json!(0.75)));
        assert_eq!(cost.get("cacheRead"), Some(&serde_json::json!(0.075)));
        assert!(cost.get("cacheWrite").is_none());
        let back: ModelEntry = serde_json::from_value(json).unwrap();
        assert_eq!(back.cost, model.cost);
    }

    #[test]
    fn profile_summary_and_wire_id() {
        let mut model = ModelEntry::new("flash", "Flash");
        model.request_id = Some("flash-req".into());
        model.context = Some(200_000);
        model.input = vec!["text".into(), "image".into()];
        model.efforts = Some(vec![]);
        assert_eq!(model.wire_id(), "flash-req");
        assert_eq!(model.profile_summary(), "200k · text/image · 无思考");
    }

    #[test]
    fn api_kind_agent_mappings() {
        assert_eq!(APIKind::Responses.open_code_package(), "aisdk:@ai-sdk/openai");
        assert_eq!(APIKind::Completions.zcode_kind(), "openai-compatible");
        assert_eq!(APIKind::Anthropic.grok_backend(), "messages");
        assert_eq!(APIKind::Google.pi_api(), "google-generative-ai");
        assert_eq!(APIKind::Responses.zcode_api_type(), "openai-responses");
        assert_eq!(APIKind::Google.dsh_api(), "google-generative-ai");
    }
}
