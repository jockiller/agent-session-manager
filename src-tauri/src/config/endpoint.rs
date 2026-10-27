use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use super::models::{APIKind, Provider};

static GOOGLE_VERSION_SEGMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^v\d+((alpha|beta)\d*)?$").unwrap());

pub fn normalize(raw: &str) -> String {
    let mut value = raw.trim().to_string();
    while value.ends_with('/') {
        value.pop();
    }
    value
}

pub fn incoming_urls(providers: &[Provider]) -> HashSet<String> {
    let mut urls = HashSet::new();
    for provider in providers {
        let raw = normalize(&provider.base_url);
        if raw.is_empty() {
            continue;
        }
        urls.insert(raw.clone());
        urls.insert(for_provider(&raw, provider.api));
        urls.insert(for_sdk(&raw, provider.api));
    }
    urls
}

fn endpoint_suffixes(api: APIKind) -> &'static [&'static str] {
    match api {
        APIKind::Anthropic => &["/messages"],
        APIKind::Responses => &["/responses"],
        APIKind::Completions => &["/chat/completions"],
        APIKind::Google => &["/models"],
    }
}

pub fn strip_endpoint_path(raw: &str, api: APIKind) -> String {
    let mut value = normalize(raw);
    let suffixes = endpoint_suffixes(api);
    let mut changed = true;
    while changed && !value.is_empty() {
        changed = false;
        let lower = value.to_lowercase();
        for suffix in suffixes {
            if lower.ends_with(suffix) {
                value = normalize(&value[..value.len() - suffix.len()]);
                changed = true;
                break;
            }
        }
    }
    value
}

pub fn strip_any_endpoint(raw: &str) -> String {
    let suffixes = [
        "/messages",
        "/chat/completions",
        "/responses",
        "/completions",
        "/models",
    ];
    let mut value = normalize(raw);
    let mut changed = true;
    while changed && !value.is_empty() {
        changed = false;
        let lower = value.to_lowercase();
        for suffix in suffixes {
            if lower.ends_with(suffix) {
                value = normalize(&value[..value.len() - suffix.len()]);
                changed = true;
                break;
            }
        }
    }
    value
}

pub fn for_anthropic_sdk(raw: &str) -> String {
    let mut value = normalize(raw);
    while value.to_lowercase().ends_with("/v1") {
        value = normalize(&value[..value.len() - 3]);
    }
    value
}

fn last_path_segment(value: &str) -> &str {
    value.rsplit('/').next().unwrap_or(value)
}

fn ensure_google_version(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    if GOOGLE_VERSION_SEGMENT.is_match(last_path_segment(value)) {
        value.to_string()
    } else {
        format!("{value}/v1beta")
    }
}

fn strip_google_version(value: &str) -> String {
    let value = normalize(value);
    if value.is_empty() {
        return value;
    }
    if GOOGLE_VERSION_SEGMENT.is_match(last_path_segment(&value)) {
        if let Some((head, _)) = value.rsplit_once('/') {
            return normalize(head);
        }
    }
    value
}

pub fn google_compat_candidates(raw: &str) -> Vec<String> {
    let google = for_provider(raw, APIKind::Google);
    if google.is_empty() {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    let openai = format!("{}/openai", google.trim_end_matches('/'));
    candidates.push(openai);
    let root = strip_google_version(&google);
    if !root.is_empty() {
        let v1 = format!("{}/v1", root.trim_end_matches('/'));
        if !candidates.iter().any(|item| item == &v1) {
            candidates.push(v1);
        }
    }
    candidates
}

pub fn for_provider(raw: &str, api: APIKind) -> String {
    let value = strip_endpoint_path(raw, api);
    match api {
        APIKind::Anthropic => for_anthropic_sdk(&value),
        APIKind::Google => ensure_google_version(&value),
        _ => value,
    }
}

pub fn for_sdk(raw: &str, api: APIKind) -> String {
    if api == APIKind::Google {
        return for_provider(raw, api);
    }
    let value = strip_endpoint_path(raw, api);
    if api != APIKind::Anthropic || value.is_empty() {
        return value;
    }
    let root = for_anthropic_sdk(&value);
    if root.is_empty() {
        value
    } else {
        format!("{root}/v1")
    }
}

fn object_string(object: &Map<String, Value>, keys: &[&str]) -> String {
    for key in keys {
        if let Some(Value::String(value)) = object.get(*key) {
            if !value.is_empty() {
                return value.clone();
            }
        }
    }
    String::new()
}

pub fn json_base_url(object: &Map<String, Value>) -> String {
    if let Some(Value::Object(nested)) = object.get("settings") {
        let url = object_string(nested, &["baseURL", "baseUrl"]);
        if !url.is_empty() {
            return normalize(&url);
        }
    }
    if let Some(Value::Object(nested)) = object.get("options") {
        let url = object_string(nested, &["baseURL", "baseUrl"]);
        if !url.is_empty() {
            return normalize(&url);
        }
    }
    normalize(&object_string(object, &["baseURL", "baseUrl"]))
}

/// Magpie-key pruning is intentionally omitted (deviation 3).
pub fn prune_providers(
    existing: &mut BTreeMap<String, Value>,
    incoming: &[Provider],
    retired: &[Provider],
) {
    let mut combined = incoming.to_vec();
    combined.extend_from_slice(retired);
    let urls = incoming_urls(&combined);
    let keep: HashSet<&str> = incoming.iter().map(|p| p.id.as_str()).collect();
    let retire: HashSet<&str> = retired.iter().map(|p| p.id.as_str()).collect();
    let keys: Vec<String> = existing.keys().cloned().collect();
    for id in keys {
        if keep.contains(id.as_str()) {
            continue;
        }
        if retire.contains(id.as_str()) {
            existing.remove(&id);
            continue;
        }
        let Some(Value::Object(object)) = existing.get(&id) else {
            continue;
        };
        let url = json_base_url(object);
        if !url.is_empty() && urls.contains(&url) {
            existing.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::{APIKind, Provider};

    #[test]
    fn endpoint_anthropic_accepts_any_shape() {
        for raw in [
            "https://relay.invalid",
            "https://relay.invalid/",
            "https://relay.invalid/v1",
            "https://relay.invalid/v1/",
            "https://relay.invalid/v1/messages",
            "https://relay.invalid/messages",
            "https://relay.invalid/v1/messages/",
        ] {
            assert_eq!(
                for_provider(raw, APIKind::Anthropic),
                "https://relay.invalid",
                "{raw}"
            );
            assert_eq!(
                for_sdk(raw, APIKind::Anthropic),
                "https://relay.invalid/v1",
                "{raw}"
            );
        }
    }

    #[test]
    fn endpoint_keeps_version_for_non_anthropic() {
        assert_eq!(
            for_provider("https://relay.invalid/v1", APIKind::Responses),
            "https://relay.invalid/v1"
        );
        assert_eq!(
            for_sdk("https://relay.invalid", APIKind::Responses),
            "https://relay.invalid"
        );
        assert_eq!(
            for_provider(
                "https://relay.invalid/v1/chat/completions",
                APIKind::Completions
            ),
            "https://relay.invalid/v1"
        );
        assert_eq!(
            for_provider("https://relay.invalid/v1/responses", APIKind::Responses),
            "https://relay.invalid/v1"
        );
        assert_eq!(
            for_provider("https://relay.invalid/v1", APIKind::Google),
            "https://relay.invalid/v1"
        );
    }

    #[test]
    fn endpoint_drops_only_trailing_endpoint_once() {
        assert_eq!(
            for_provider(
                "https://relay.invalid/proxy/v1/chat/completions",
                APIKind::Completions
            ),
            "https://relay.invalid/proxy/v1"
        );
        assert_eq!(for_provider("", APIKind::Anthropic), "");
        assert_eq!(for_sdk("", APIKind::Anthropic), "");
    }

    #[test]
    fn endpoint_prune_urls_cover_written_shapes() {
        let provider = Provider {
            id: "GJA".to_string(),
            name: String::new(),
            base_url: "https://relay.invalid/v1/messages".to_string(),
            api: APIKind::Anthropic,
            catalog_url: String::new(),
            api_key: String::new(),
            compat_base_url: String::new(),
            models: Vec::new(),
        };
        let urls = incoming_urls(&[provider]);
        assert!(urls.contains("https://relay.invalid"));
        assert!(urls.contains("https://relay.invalid/v1"));
        assert!(urls.contains("https://relay.invalid/v1/messages"));
    }

    #[test]
    fn endpoint_google_appends_v1beta_unless_versioned() {
        for (raw, written) in [
            (
                "https://generativelanguage.googleapis.com",
                "https://generativelanguage.googleapis.com/v1beta",
            ),
            ("https://relay.x/gemini", "https://relay.x/gemini/v1beta"),
            ("https://relay.x/v1beta", "https://relay.x/v1beta"),
            ("https://relay.x/v1", "https://relay.x/v1"),
            ("https://relay.x/v1beta/models", "https://relay.x/v1beta"),
        ] {
            assert_eq!(for_provider(raw, APIKind::Google), written, "{raw}");
            assert_eq!(for_sdk(raw, APIKind::Google), written, "{raw}");
        }
    }

    #[test]
    fn endpoint_google_incoming_urls_contain_written_shape() {
        let provider = Provider {
            id: "g".to_string(),
            name: String::new(),
            base_url: "https://generativelanguage.googleapis.com".to_string(),
            api: APIKind::Google,
            catalog_url: String::new(),
            api_key: String::new(),
            compat_base_url: String::new(),
            models: Vec::new(),
        };
        let urls = incoming_urls(&[provider]);
        assert!(urls.contains("https://generativelanguage.googleapis.com"));
        assert!(urls.contains("https://generativelanguage.googleapis.com/v1beta"));

        let pasted = Provider {
            id: "g2".to_string(),
            name: String::new(),
            base_url: "https://relay.x/v1beta/models".to_string(),
            api: APIKind::Google,
            catalog_url: String::new(),
            api_key: String::new(),
            compat_base_url: String::new(),
            models: Vec::new(),
        };
        let urls = incoming_urls(&[pasted]);
        assert!(urls.contains("https://relay.x/v1beta/models"));
        assert!(urls.contains("https://relay.x/v1beta"));
    }

    #[test]
    fn google_compat_candidates_official_and_relay() {
        assert_eq!(
            google_compat_candidates("https://generativelanguage.googleapis.com"),
            [
                "https://generativelanguage.googleapis.com/v1beta/openai",
                "https://generativelanguage.googleapis.com/v1",
            ]
        );
        assert_eq!(
            google_compat_candidates("https://relay.x/gemini"),
            [
                "https://relay.x/gemini/v1beta/openai",
                "https://relay.x/gemini/v1",
            ]
        );
        assert_eq!(
            google_compat_candidates("https://relay.x/v1beta"),
            ["https://relay.x/v1beta/openai", "https://relay.x/v1"]
        );
        assert_eq!(
            google_compat_candidates("https://relay.x/v1"),
            ["https://relay.x/v1/openai", "https://relay.x/v1"]
        );
    }
}
