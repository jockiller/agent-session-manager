use std::collections::HashMap;

use serde_json::Value;

use super::models::{EffortLevel, ModelEntry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogHit {
    pub context: Option<i64>,
    pub output: Option<i64>,
    pub input: Vec<String>,
    pub efforts: Vec<String>,
    pub tool_call: bool,
    pub reasoning_known: bool,
}

impl Default for CatalogHit {
    fn default() -> Self {
        Self {
            context: None,
            output: None,
            input: vec!["text".to_string()],
            efforts: Vec::new(),
            tool_call: true,
            reasoning_known: false,
        }
    }
}

impl CatalogHit {
    pub fn new() -> Self {
        Self::default()
    }
}

pub const INPUT_KINDS: [&str; 3] = ["text", "image", "pdf"];

/// Build an id → CatalogHit index from a parsed models.dev `api.json` object.
/// Fetch/cache of that JSON is phase 3.
pub fn index(models_dev: &Value) -> HashMap<String, CatalogHit> {
    let Some(object) = models_dev.as_object() else {
        return HashMap::new();
    };
    let mut ranked: HashMap<String, (CatalogHit, i32)> = HashMap::new();
    for (provider_id, raw) in object {
        let Some(provider) = raw.as_object() else {
            continue;
        };
        let empty = serde_json::Map::new();
        let models = provider
            .get("models")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        for (model_id, raw_model) in models {
            let Some(model) = raw_model.as_object() else {
                continue;
            };
            let hit = hit_from(model);
            let score = catalog_score(provider_id, model);
            if let Some((_, existing_score)) = ranked.get(model_id) {
                if *existing_score >= score {
                    continue;
                }
            }
            ranked.insert(model_id.clone(), (hit, score));
        }
    }
    ranked.into_iter().map(|(id, (hit, _))| (id, hit)).collect()
}

pub fn r#match(id: &str, index: &HashMap<String, CatalogHit>) -> Option<CatalogHit> {
    if let Some(hit) = index.get(id) {
        return Some(hit.clone());
    }
    let base = id.split_once(':').map(|(head, _)| head).unwrap_or(id);
    if base != id {
        if let Some(hit) = index.get(base) {
            return Some(hit.clone());
        }
    }
    let mut tokens: Vec<&str> = base.split('-').collect();
    while tokens.len() > 1 {
        tokens.pop();
        let candidate = tokens.join("-");
        if let Some(hit) = index.get(&candidate) {
            return Some(hit.clone());
        }
    }
    None
}

pub fn infer(id: &str) -> CatalogHit {
    let text = id.to_lowercase();
    let mut input = vec!["text".to_string()];
    if ["gpt", "claude", "gemini", "grok", "glm", "qwen", "kimi"]
        .iter()
        .any(|needle| text.contains(needle))
    {
        input.push("image".to_string());
    }
    if text.contains("claude") || text.contains("gemini") {
        input.push("pdf".to_string());
    }
    let reasoning_hints = [
        "opus", "sonnet", "haiku", "o1", "o3", "o4", "gpt-5", "gpt-4", "grok", "deepseek",
        "gemini", "glm", "kimi", "thinking", "reasoner", "codex",
    ];
    let mut efforts = Vec::new();
    if reasoning_hints.iter().any(|needle| text.contains(needle)) {
        efforts = vec!["low".to_string(), "medium".to_string(), "high".to_string()];
        if text.contains("gpt") || text.contains("grok") || text.contains("codex") {
            efforts.push("xhigh".to_string());
        }
    }
    CatalogHit {
        input,
        efforts,
        reasoning_known: false,
        ..CatalogHit::default()
    }
}

pub fn enrich(model: &ModelEntry, index: &HashMap<String, CatalogHit>) -> ModelEntry {
    let mut next = model.clone();
    let inferred = infer(&model.id);
    let source = r#match(&model.id, index).unwrap_or(inferred);
    if next.context.is_none() {
        next.context = source.context;
    }
    if next.output.is_none() {
        next.output = source.output;
    }
    if next.input.is_empty() {
        next.input = if source.input.is_empty() {
            vec!["text".to_string()]
        } else {
            source.input
        };
    }
    if next.efforts.is_none() {
        next.efforts = Some(source.efforts);
    }
    next
}

pub fn merge(fetched: &ModelEntry, existing: &ModelEntry) -> ModelEntry {
    let mut next = fetched.clone();
    next.name = existing.name.clone();
    if existing.context.is_some() {
        next.context = existing.context;
    }
    if existing.output.is_some() {
        next.output = existing.output;
    }
    if !existing.input.is_empty() {
        next.input = existing.input.clone();
    }
    if existing.efforts.is_some() {
        next.efforts = existing.efforts.clone();
    }
    next.tool_call = existing.tool_call;
    next
}

fn hit_from(model: &serde_json::Map<String, Value>) -> CatalogHit {
    let empty = serde_json::Map::new();
    let limit = model
        .get("limit")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let modalities = model
        .get("modalities")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let listed = string_list(modalities.get("input"));
    let input: Vec<String> = INPUT_KINDS
        .iter()
        .filter(|kind| listed.iter().any(|item| item == *kind))
        .map(|kind| kind.to_string())
        .collect();
    let efforts = parse_efforts(model);
    let tool_call = model
        .get("tool_call")
        .and_then(Value::as_bool)
        .or_else(|| {
            model
                .get("capabilities")
                .and_then(Value::as_object)
                .and_then(|caps| caps.get("tools"))
                .and_then(Value::as_bool)
        })
        .unwrap_or(true);
    CatalogHit {
        context: int_value(limit.get("context")),
        output: int_value(limit.get("output")),
        input: if input.is_empty() {
            vec!["text".to_string()]
        } else {
            input
        },
        efforts: efforts.0,
        tool_call,
        reasoning_known: efforts.1,
    }
}

pub fn parse_efforts(model: &serde_json::Map<String, Value>) -> (Vec<String>, bool) {
    let allowed: Vec<String> = EffortLevel::ALL
        .iter()
        .map(|level| level.raw().to_string())
        .collect();
    if let Some(options) = model.get("reasoning_options").and_then(Value::as_array) {
        for option in options {
            let Some(object) = option.as_object() else {
                continue;
            };
            if object.get("type").and_then(Value::as_str) == Some("effort") {
                let listed = string_list(object.get("values"));
                if !listed.is_empty() {
                    return (
                        allowed
                            .iter()
                            .filter(|item| listed.contains(item))
                            .cloned()
                            .collect(),
                        true,
                    );
                }
            }
        }
    }
    if let Some(reasoning) = model.get("reasoning").and_then(Value::as_object) {
        let listed = string_list(reasoning.get("effort"));
        if !listed.is_empty() {
            return (
                allowed
                    .iter()
                    .filter(|item| listed.contains(item))
                    .cloned()
                    .collect(),
                true,
            );
        }
        if reasoning.get("supported").and_then(Value::as_bool) == Some(false) {
            return (Vec::new(), true);
        }
        if reasoning.get("supported").and_then(Value::as_bool) == Some(true) {
            return (
                vec!["low".to_string(), "medium".to_string(), "high".to_string()],
                true,
            );
        }
    }
    if let Some(flag) = model.get("reasoning").and_then(Value::as_bool) {
        return (
            if flag {
                vec!["low".to_string(), "medium".to_string(), "high".to_string()]
            } else {
                Vec::new()
            },
            true,
        );
    }
    if let Some(caps) = model.get("capabilities").and_then(Value::as_object) {
        if let Some(reasoning) = caps.get("reasoning").and_then(Value::as_object) {
            let listed = string_list(reasoning.get("effort"));
            if !listed.is_empty() {
                return (
                    allowed
                        .iter()
                        .filter(|item| listed.contains(item))
                        .cloned()
                        .collect(),
                    true,
                );
            }
        }
    }
    (Vec::new(), false)
}

fn string_list(raw: Option<&Value>) -> Vec<String> {
    match raw {
        Some(Value::Array(list)) => list
            .iter()
            .map(|item| match item {
                Value::String(text) => text.clone(),
                other => other.to_string().trim_matches('"').to_string(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn catalog_score(provider_id: &str, model: &serde_json::Map<String, Value>) -> i32 {
    let mut score = 0;
    if [
        "anthropic",
        "openai",
        "google",
        "xai",
        "deepseek",
        "moonshot",
        "zhipu",
        "alibaba",
    ]
    .contains(&provider_id)
    {
        score += 10;
    }
    if model
        .get("limit")
        .and_then(Value::as_object)
        .and_then(|limit| limit.get("context"))
        .is_some()
    {
        score += 5;
    }
    if model.get("reasoning").is_some() || model.get("reasoning_options").is_some() {
        score += 2;
    }
    score
}

fn int_value(raw: Option<&Value>) -> Option<i64> {
    match raw {
        Some(Value::Number(number)) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value as i64)),
        Some(Value::String(text)) => text.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::ModelEntry;
    use serde_json::json;

    #[test]
    fn infer_gives_claude_image_pdf_and_efforts() {
        let hit = infer("claude-opus-5-5");
        assert_eq!(
            hit.input,
            vec![
                "text".to_string(),
                "image".to_string(),
                "pdf".to_string()
            ]
        );
        assert_eq!(
            hit.efforts,
            vec!["low".to_string(), "medium".to_string(), "high".to_string()]
        );
    }

    #[test]
    fn infer_gives_grok_xhigh() {
        let hit = infer("grok-4.6");
        assert!(hit.input.contains(&"image".to_string()));
        assert!(hit.efforts.contains(&"xhigh".to_string()));
    }

    #[test]
    fn enrich_fills_from_catalog_and_keeps_user_context() {
        let index = index(&json!({
            "anthropic": {
                "models": {
                    "claude-opus-5-5": {
                        "limit": {"context": 1_000_000, "output": 64_000},
                        "modalities": {"input": ["text", "image", "pdf"]},
                        "reasoning": {"effort": ["low", "medium", "high", "max"]},
                        "tool_call": true
                    }
                }
            }
        }));
        let filled = enrich(&ModelEntry::new("claude-opus-5-5", "Opus"), &index);
        assert_eq!(filled.context, Some(1_000_000));
        assert_eq!(filled.output, Some(64_000));
        assert_eq!(
            filled.input,
            vec![
                "text".to_string(),
                "image".to_string(),
                "pdf".to_string()
            ]
        );
        assert_eq!(
            filled.efforts.as_deref(),
            Some(
                [
                    "low".to_string(),
                    "medium".to_string(),
                    "high".to_string(),
                    "max".to_string()
                ]
                .as_slice()
            )
        );

        let mut custom = filled.clone();
        custom.context = Some(200_000);
        custom.efforts = Some(vec!["high".to_string()]);
        let merged = merge(&filled, &custom);
        assert_eq!(merged.context, Some(200_000));
        assert_eq!(merged.efforts.as_deref(), Some(["high".to_string()].as_slice()));
    }

    #[test]
    fn format_tokens_abbreviates() {
        assert_eq!(ModelEntry::format_tokens(200_000), "200k");
        assert_eq!(ModelEntry::format_tokens(1_048_576), "1048576");
        assert_eq!(ModelEntry::format_tokens(1_000_000), "1M");
    }
}
