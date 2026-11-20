use std::collections::HashMap;

use serde_json::Value;

use crate::config::models::{AppDocument, ModelCost};
use crate::config::profile::{self, CatalogHit};

use super::PricingRate;

pub fn extract_model_from_value(val: &Value) -> Option<String> {
    match val {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else if trimmed.starts_with('{') || trimmed.starts_with('"') {
                let inner = extract_model_id(trimmed);
                if inner.is_empty() { None } else { Some(inner) }
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Object(map) => {
            for key in ["id", "modelID", "model_id", "model", "name"] {
                if let Some(v) = map.get(key) {
                    if let Some(s) = v.as_str() {
                        let t = s.trim();
                        if !t.is_empty() {
                            return Some(t.to_string());
                        }
                    } else if v.is_object() {
                        if let Some(nested) = extract_model_from_value(v) {
                            return Some(nested);
                        }
                    }
                }
            }
            for key in ["providerID", "provider_id", "provider"] {
                if let Some(s) = map.get(key).and_then(Value::as_str) {
                    let t = s.trim();
                    if !t.is_empty() {
                        return Some(t.to_string());
                    }
                }
            }
            None
        }
        _ => None,
    }
}

pub fn extract_model_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with('{') || trimmed.starts_with('"') {
        if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
            match val {
                Value::String(s) => return extract_model_id(&s),
                Value::Object(_) => {
                    if let Some(found) = extract_model_from_value(&val) {
                        return found;
                    }
                }
                _ => {}
            }
        }
    }
    trimmed.to_string()
}

pub fn normalize_model_id(raw: &str) -> String {
    let extracted = extract_model_id(raw);
    let trimmed = extracted.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        return String::new();
    }
    let no_effort = trimmed.split('#').next().unwrap_or(&trimmed);
    let no_variant = no_effort.split(':').next().unwrap_or(no_effort);
    let tail = no_variant.rsplit('/').next().unwrap_or(no_variant);
    tail.trim().to_string()
}

fn rate_from_cost(cost: &ModelCost) -> Option<PricingRate> {
    if cost.is_empty() {
        return None;
    }
    Some(PricingRate {
        input: cost.input.unwrap_or(0.0).max(0.0),
        output: cost.output.unwrap_or(0.0).max(0.0),
        cache_read: cost.cache_read.unwrap_or(0.0).max(0.0),
        cache_write: cost.cache_write.unwrap_or(0.0).max(0.0),
    })
}

pub struct PricingResolver {
    overrides: HashMap<String, PricingRate>,
    provider_rates: HashMap<String, PricingRate>,
    catalog: HashMap<String, CatalogHit>,
}

impl PricingResolver {
    pub fn new(
        overrides: HashMap<String, PricingRate>,
        document: &AppDocument,
        catalog: HashMap<String, CatalogHit>,
    ) -> Self {
        let mut provider_rates = HashMap::new();
        for provider in &document.providers {
            for model in &provider.models {
                if let Some(cost) = &model.cost {
                    if let Some(rate) = rate_from_cost(cost) {
                        for key in [
                            normalize_model_id(&model.id),
                            model.request_id.as_deref().map(normalize_model_id).unwrap_or_default(),
                        ] {
                            if !key.is_empty() {
                                provider_rates.entry(key).or_insert(rate);
                            }
                        }
                    }
                }
            }
        }
        Self {
            overrides,
            provider_rates,
            catalog,
        }
    }

    pub fn resolve(&self, model_raw: &str, model_norm: &str) -> Option<PricingRate> {
        let key = if model_norm.is_empty() {
            normalize_model_id(model_raw)
        } else {
            model_norm.to_string()
        };
        if key.is_empty() {
            return None;
        }
        if let Some(rate) = self.overrides.get(&key) {
            return Some(*rate);
        }
        if let Some(rate) = self.provider_rates.get(&key) {
            return Some(*rate);
        }
        if let Some(hit) = profile::r#match(&key, &self.catalog)
            .or_else(|| profile::r#match(model_raw, &self.catalog))
        {
            if let Some(cost) = &hit.cost {
                return rate_from_cost(cost);
            }
        }
        None
    }
}

pub fn compute_cost_usd(
    rate: &PricingRate,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
) -> f64 {
    let total = (input_tokens as f64) * rate.input
        + (output_tokens as f64) * rate.output
        + (cache_read_tokens as f64) * rate.cache_read
        + (cache_write_tokens as f64) * rate.cache_write;
    total / 1_000_000.0
}

pub fn compute_cache_savings_usd(rate: &PricingRate, cache_read_tokens: u64) -> f64 {
    let diff = (rate.input - rate.cache_read).max(0.0);
    (cache_read_tokens as f64) * diff / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::{ModelEntry, Provider};

    #[test]
    fn normalize_strips_provider_variant_and_effort() {
        assert_eq!(normalize_model_id("openai/gpt-5:free#high"), "gpt-5");
        assert_eq!(normalize_model_id("Claude-Sonnet-4-5"), "claude-sonnet-4-5");
    }

    #[test]
    fn extract_and_normalize_handles_opencode_json_models() {
        let j1 = r#"{"id":"gemini-4-hydrogen","providerID":"google","variant":"high"}"#;
        assert_eq!(extract_model_id(j1), "gemini-4-hydrogen");
        assert_eq!(normalize_model_id(j1), "gemini-4-hydrogen");

        let j2 = r#"{"id":"gpt-6.2-sol","providerID":"boke"}"#;
        assert_eq!(extract_model_id(j2), "gpt-6.2-sol");
        assert_eq!(normalize_model_id(j2), "gpt-6.2-sol");

        let nested = r#"{"model":{"id":"claude-sonnet-4-5"}}"#;
        assert_eq!(extract_model_id(nested), "claude-sonnet-4-5");
        assert_eq!(normalize_model_id(nested), "claude-sonnet-4-5");

        let quoted = r#""openai/gpt-5""#;
        assert_eq!(extract_model_id(quoted), "openai/gpt-5");
        assert_eq!(normalize_model_id(quoted), "gpt-5");
    }

    #[test]
    fn resolver_respects_priority_override_gt_provider_gt_catalog() {
        let mut doc = AppDocument::default();
        let mut prov = Provider::new("p1", "P1", "https://x");
        let mut m = ModelEntry::new("claude-sonnet-4-5", "Sonnet");
        m.cost = Some(ModelCost {
            input: Some(4.0),
            output: Some(20.0),
            cache_read: Some(0.4),
            cache_write: Some(5.0),
        });
        let mut m2 = ModelEntry::new("gpt-5", "GPT 5");
        m2.cost = Some(ModelCost {
            input: Some(2.0),
            output: Some(10.0),
            cache_read: Some(0.2),
            cache_write: None,
        });
        prov.models = vec![m, m2];
        doc.providers.push(prov);

        let mut overrides = HashMap::new();
        overrides.insert(
            "claude-sonnet-4-5".to_string(),
            PricingRate {
                input: 3.0,
                output: 15.0,
                cache_read: 0.3,
                cache_write: 3.75,
            },
        );

        let mut catalog = HashMap::new();
        catalog.insert(
            "gemini-2.5-pro".to_string(),
            CatalogHit {
                cost: Some(ModelCost {
                    input: Some(1.25),
                    output: Some(10.0),
                    cache_read: Some(0.31),
                    cache_write: None,
                }),
                ..Default::default()
            },
        );

        let resolver = PricingResolver::new(overrides, &doc, catalog);
        // Override wins over provider
        assert_eq!(
            resolver.resolve("claude-sonnet-4-5", "claude-sonnet-4-5").unwrap().input,
            3.0
        );
        // Provider wins when no override
        assert_eq!(resolver.resolve("p1/gpt-5", "gpt-5").unwrap().input, 2.0);
        // Catalog fallback works with date suffix stripped
        assert_eq!(
            resolver.resolve("gemini-2.5-pro-20250601", "gemini-2.5-pro-20250601").unwrap().input,
            1.25
        );
        // Unknown returns None
        assert!(resolver.resolve("unknown-model", "unknown-model").is_none());
    }

    #[test]
    fn computes_cost_and_cache_savings_accurately() {
        let rate = PricingRate {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
        };
        let cost = compute_cost_usd(&rate, 1_000_000, 100_000, 2_000_000, 0);
        // 3.0 + 1.5 + 0.6 = 5.1
        assert!((cost - 5.1).abs() < 1e-9);
        let savings = compute_cache_savings_usd(&rate, 2_000_000);
        // 2M * (3.0 - 0.3) / 1M = 5.4
        assert!((savings - 5.4).abs() < 1e-9);
    }
}
