use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use super::models::ModelEntry;

static KEY_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\s\-_.·/]+").unwrap());
static VERSION_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^v?\d+(?:\.\d+)*[a-z]?$").unwrap());

const CASING: &[(&str, &str)] = &[
    ("deepseek", "DeepSeek"),
    ("gpt", "GPT"),
    ("glm", "GLM"),
    ("kimi", "Kimi"),
    ("qwen", "Qwen"),
    ("claude", "Claude"),
    ("gemini", "Gemini"),
    ("grok", "Grok"),
    ("llama", "Llama"),
    ("mistral", "Mistral"),
    ("mixtral", "Mixtral"),
    ("gemma", "Gemma"),
    ("opus", "Opus"),
    ("sonnet", "Sonnet"),
    ("haiku", "Haiku"),
    ("fable", "Fable"),
    ("flash", "Flash"),
    ("pro", "Pro"),
    ("max", "Max"),
    ("mini", "Mini"),
    ("nano", "Nano"),
    ("lite", "Lite"),
    ("turbo", "Turbo"),
    ("thinking", "Thinking"),
    ("reasoner", "Reasoner"),
    ("codex", "Codex"),
];

pub fn key(value: &str) -> String {
    KEY_PATTERN.replace_all(value, "").to_lowercase()
}

pub fn looks_raw(name: &str, model_id: &str) -> bool {
    let text = name.trim();
    text.is_empty() || text.eq_ignore_ascii_case(model_id.trim())
}

pub fn derive(model_id: &str) -> String {
    let core = model_id.trim();
    if core.is_empty() {
        return String::new();
    }
    let segment = core.rsplit('/').next().unwrap_or(core);
    let tokens: Vec<String> = segment
        .split(|c: char| c == '-' || c == '_' || c.is_whitespace())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect();
    if tokens.is_empty() {
        return String::new();
    }

    let mut merged: Vec<String> = Vec::new();
    for token in tokens {
        if let Some(last) = merged.last() {
            if token.len() == 1
                && token.chars().all(|c| c.is_ascii_digit())
                && last.len() == 1
                && last.chars().all(|c| c.is_ascii_digit())
            {
                let combined = format!("{last}.{token}");
                *merged.last_mut().unwrap() = combined;
                continue;
            }
        }
        merged.push(token);
    }

    merged
        .into_iter()
        .map(|token| render_token(&token))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn resolve(names: &HashMap<String, String>) -> HashMap<String, String> {
    let mut resolved: HashMap<String, String> = HashMap::new();
    for (model_id, name) in names {
        let mut current = name.trim().to_string();
        if looks_raw(&current, model_id) {
            let derived = derive(model_id);
            if !derived.is_empty() && derived != *model_id {
                current = derived;
            }
        }
        resolved.insert(
            model_id.clone(),
            if current.is_empty() {
                model_id.clone()
            } else {
                current
            },
        );
    }

    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    for (model_id, name) in &resolved {
        groups
            .entry(key(name))
            .or_default()
            .push(model_id.clone());
    }

    for group in groups.values() {
        if group.len() < 2 {
            continue;
        }
        let owners: Vec<&String> = group
            .iter()
            .filter(|id| key(resolved.get(*id).map(String::as_str).unwrap_or("")) == key(id))
            .collect();
        let wrong: Vec<String> = if owners.len() == 1 {
            group
                .iter()
                .filter(|id| *id != owners[0])
                .cloned()
                .collect()
        } else {
            let mut sorted = group.clone();
            sorted.sort();
            sorted.into_iter().skip(1).collect()
        };
        for mid in wrong {
            let candidate = derive(&mid);
            let occupied: HashSet<String> = resolved
                .iter()
                .filter(|(other, _)| *other != &mid)
                .map(|(_, value)| key(value))
                .collect();
            if !candidate.is_empty() && !occupied.contains(&key(&candidate)) {
                resolved.insert(mid, candidate);
            } else if let Some(current) = resolved.get(&mid).cloned() {
                resolved.insert(mid.clone(), format!("{current} ({mid})"));
            }
        }
    }
    resolved
}

pub fn apply(models: &[ModelEntry]) -> Vec<ModelEntry> {
    let names: HashMap<String, String> = models
        .iter()
        .map(|model| (model.id.clone(), model.name.clone()))
        .collect();
    let resolved = resolve(&names);
    models
        .iter()
        .map(|model| {
            let mut copy = model.clone();
            if let Some(name) = resolved.get(&model.id) {
                if !name.is_empty() {
                    copy.name = name.clone();
                }
            }
            copy
        })
        .collect()
}

fn render_token(token: &str) -> String {
    if let Some((_, branded)) = CASING
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(token))
    {
        return branded.to_string();
    }
    if VERSION_PATTERN.is_match(token) {
        let mut chars = token.chars();
        let Some(first) = chars.next() else {
            return token.to_string();
        };
        return first.to_uppercase().collect::<String>() + chars.as_str();
    }
    if token == token.to_uppercase() && token.chars().any(|c| c.is_alphabetic()) {
        return token.to_string();
    }
    let mut chars = token.chars();
    let Some(first) = chars.next() else {
        return token.to_string();
    };
    first.to_uppercase().collect::<String>() + chars.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn derive_turns_bare_ids_into_readable_names() {
        assert_eq!(derive("deepseek-v4.1-flash"), "DeepSeek V4.1 Flash");
        assert_eq!(derive("claude-opus-5-5"), "Claude Opus 5.5");
        assert_eq!(derive("glm-5.3-flash"), "GLM 5.3 Flash");
        assert_eq!(derive("grok-4.6"), "Grok 4.6");
    }

    #[test]
    fn resolve_replaces_raw_ids_and_keeps_custom_alias() {
        let resolved = resolve(&HashMap::from([
            (
                "claude-opus-5-5".to_string(),
                "claude-opus-5-5".to_string(),
            ),
            ("flash".to_string(), "Opus 5.5".to_string()),
        ]));
        assert_eq!(
            resolved.get("claude-opus-5-5").map(String::as_str),
            Some("Claude Opus 5.5")
        );
        assert_eq!(resolved.get("flash").map(String::as_str), Some("Opus 5.5"));
    }

    #[test]
    fn resolve_disambiguates_same_display_name() {
        let resolved = resolve(&HashMap::from([
            (
                "claude-opus-5-5".to_string(),
                "Claude Opus 5.5".to_string(),
            ),
            ("opus-alias".to_string(), "Claude Opus 5.5".to_string()),
        ]));
        assert_eq!(
            resolved.get("claude-opus-5-5").map(String::as_str),
            Some("Claude Opus 5.5")
        );
        assert_eq!(
            resolved.get("opus-alias").map(String::as_str),
            Some("Opus Alias")
        );
    }
}
