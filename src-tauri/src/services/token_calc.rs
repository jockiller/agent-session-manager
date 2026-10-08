use crate::models::TokenStats;

pub fn make_stats(
    model: &str,
    prompt: u64,
    completion: u64,
    cache_read: u64,
    cache_write: u64,
) -> TokenStats {
    make_stats_extended(model, prompt, completion, cache_read, cache_write, 0, None)
}

pub fn make_stats_extended(
    _model: &str,
    prompt: u64,
    completion: u64,
    cache_read: u64,
    cache_write: u64,
    reasoning: u64,
    duration_ms: Option<u64>,
) -> TokenStats {
    let total = prompt + completion + cache_read + cache_write;

    let cache_hit_rate = if prompt + cache_read > 0 {
        let rate = (cache_read as f64 / (prompt + cache_read) as f64) * 100.0;
        Some((rate * 10.0).round() / 10.0)
    } else {
        None
    };

    let tokens_per_second = if let Some(ms) = duration_ms {
        if ms > 50 && completion > 0 {
            let speed = (completion as f64) / (ms as f64 / 1000.0);
            Some((speed * 10.0).round() / 10.0)
        } else {
            None
        }
    } else {
        None
    };

    TokenStats {
        prompt_tokens: prompt,
        completion_tokens: completion,
        cache_read_tokens: cache_read,
        cache_write_tokens: cache_write,
        reasoning_tokens: reasoning,
        total_tokens: total,
        cache_hit_rate,
        tokens_per_second,
    }
}
