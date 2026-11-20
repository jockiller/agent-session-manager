use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use chrono::Local;
use rusqlite::params;

use crate::config::catalog_cache;
use crate::config::fileio;

use super::db::{get_currency_pref, load_pricing_override_map, open_usage_db, usage_db_path};
use super::pricing::{
    compute_cache_savings_usd, compute_cost_usd, extract_model_id, normalize_model_id,
    PricingResolver,
};
use super::{
    BreakdownRow, SessionUsageDetail, TimeseriesPoint, TodayUsageSummary, TopSessionRow,
    UsageDashboardData, UsageFilter, UsageKpis, UsageLogPage, UsageLogRow,
};

struct RawEventRow {
    platform: String,
    session_id: String,
    workspace: String,
    model_raw: String,
    model_norm: String,
    ts_ms: i64,
    day_key: String,
    hour_key: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    reasoning_tokens: u64,
    granularity: String,
}

#[derive(Default)]
struct AggBucket {
    label: String,
    cost_usd: f64,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    sessions: HashSet<String>,
    events: usize,
    priced: bool,
}

#[derive(Default)]
struct SessionAgg {
    platform: String,
    workspace: String,
    model: String,
    cost_usd: f64,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    last_ts_ms: i64,
    granularity: String,
}

fn cache_hit_rate(input: u64, cache_read: u64) -> f64 {
    let denom = input + cache_read;
    if denom == 0 {
        return 0.0;
    }
    ((cache_read as f64 / denom as f64) * 1000.0).round() / 10.0
}

fn sanitize_model_fields(raw: String, norm: String) -> (String, String) {
    if raw.trim_start().starts_with('{') || norm.starts_with('{') {
        let clean_raw = extract_model_id(&raw);
        let clean_norm = normalize_model_id(&clean_raw);
        (clean_raw, clean_norm)
    } else {
        (raw, norm)
    }
}

fn load_resolver(conn: &rusqlite::Connection, app_data: &Path) -> PricingResolver {
    let overrides = load_pricing_override_map(conn);
    let doc = fileio::load_document(&app_data.join("config-state.json"));
    let catalog = catalog_cache::global().snapshot();
    PricingResolver::new(overrides, &doc, catalog)
}

pub fn query_dashboard(app_data: &Path, filter: &UsageFilter) -> Result<UsageDashboardData, String> {
    let conn = open_usage_db(&usage_db_path(app_data))?;
    let resolver = load_resolver(&conn, app_data);
    let pref = get_currency_pref(&conn);
    let rate_mult = pref.rate;

    let available_models: Vec<String> = conn
        .prepare("SELECT DISTINCT model_raw, model_norm FROM usage_events WHERE model_norm != ''")
        .ok()
        .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .ok()
                .map(|rows| {
                    rows.flatten()
                        .map(|(raw, norm)| sanitize_model_fields(raw, norm).1)
                        .filter(|m| !m.is_empty())
                        .collect::<BTreeSet<String>>()
                        .into_iter()
                        .collect()
                })
        })
        .unwrap_or_default();

    let available_workspaces: Vec<String> = conn
        .prepare("SELECT DISTINCT workspace FROM usage_events WHERE workspace != '' ORDER BY workspace ASC")
        .ok()
        .and_then(|mut s| {
            s.query_map([], |r| r.get::<_, String>(0))
                .ok()
                .map(|rows| rows.flatten().collect())
        })
        .unwrap_or_default();

    let start_ms = filter.start_ms.unwrap_or(0);
    let end_ms = filter.end_ms.unwrap_or(i64::MAX);
    let plat_f = filter.platform.as_deref().unwrap_or("all");
    let model_f = filter.model.as_deref().unwrap_or("all");
    let ws_f = filter.workspace.as_deref().unwrap_or("all");

    let mut stmt = conn
        .prepare(
            "SELECT platform, session_id, workspace, model_raw, model_norm, ts_ms, day_key, hour_key,
                    input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens, granularity
             FROM usage_events
             WHERE ts_ms >= ?1 AND ts_ms <= ?2
               AND (?3 = 'all' OR platform = ?3)
               AND (?4 = 'all' OR model_norm = ?4)
               AND (?5 = 'all' OR workspace = ?5)",
        )
        .map_err(|e| format!("Prepare dashboard query failed: {e}"))?;

    let rows = stmt
        .query_map(params![start_ms, end_ms, plat_f, model_f, ws_f], |r| {
            let (model_raw, model_norm) =
                sanitize_model_fields(r.get::<_, String>(3)?, r.get::<_, String>(4)?);
            Ok(RawEventRow {
                platform: r.get(0)?,
                session_id: r.get(1)?,
                workspace: r.get(2)?,
                model_raw,
                model_norm,
                ts_ms: r.get(5)?,
                day_key: r.get(6)?,
                hour_key: r.get(7)?,
                input_tokens: r.get::<_, i64>(8)?.max(0) as u64,
                output_tokens: r.get::<_, i64>(9)?.max(0) as u64,
                cache_read_tokens: r.get::<_, i64>(10)?.max(0) as u64,
                cache_write_tokens: r.get::<_, i64>(11)?.max(0) as u64,
                reasoning_tokens: r.get::<_, i64>(12)?.max(0) as u64,
                granularity: r.get(13)?,
            })
        })
        .map_err(|e| format!("Query usage events failed: {e}"))?;

    let events: Vec<RawEventRow> = rows.flatten().collect();
    let hourly = if let (Some(s), Some(e)) = (filter.start_ms, filter.end_ms) {
        (e - s) <= 2 * 86_400_000
    } else {
        false
    };

    let mut kpis = UsageKpis::default();
    let mut kpi_sessions: HashSet<String> = HashSet::new();
    let mut unpriced: BTreeSet<String> = BTreeSet::new();
    let mut series_map: BTreeMap<String, (f64, u64, u64, u64, u64)> = BTreeMap::new();
    let mut by_model_map: HashMap<String, AggBucket> = HashMap::new();
    let mut by_plat_map: HashMap<String, AggBucket> = HashMap::new();
    let mut by_ws_map: HashMap<String, AggBucket> = HashMap::new();
    let mut sess_map: HashMap<(String, String), SessionAgg> = HashMap::new();

    for ev in events {
        let rate_opt = resolver.resolve(&ev.model_raw, &ev.model_norm);
        let (cost_usd, savings_usd, is_priced) = match rate_opt {
            Some(rate) => (
                compute_cost_usd(
                    &rate,
                    ev.input_tokens,
                    ev.output_tokens,
                    ev.cache_read_tokens,
                    ev.cache_write_tokens,
                ),
                compute_cache_savings_usd(&rate, ev.cache_read_tokens),
                true,
            ),
            None => {
                if !ev.model_norm.is_empty() {
                    unpriced.insert(ev.model_norm.clone());
                }
                (0.0, 0.0, false)
            }
        };

        kpis.total_cost += cost_usd * rate_mult;
        kpis.cache_savings += savings_usd * rate_mult;
        kpis.input_tokens += ev.input_tokens;
        kpis.output_tokens += ev.output_tokens;
        kpis.cache_read_tokens += ev.cache_read_tokens;
        kpis.cache_write_tokens += ev.cache_write_tokens;
        kpis.reasoning_tokens += ev.reasoning_tokens;
        kpis.event_count += 1;
        let sess_key = format!("{}:{}", ev.platform, ev.session_id);
        kpi_sessions.insert(sess_key.clone());

        let bucket_key = if hourly {
            ev.hour_key.clone()
        } else {
            ev.day_key.clone()
        };
        let ts_entry = series_map.entry(bucket_key).or_default();
        ts_entry.0 += cost_usd * rate_mult;
        ts_entry.1 += ev.input_tokens;
        ts_entry.2 += ev.output_tokens;
        ts_entry.3 += ev.cache_read_tokens;
        ts_entry.4 += ev.cache_write_tokens;

        for (map, key, label) in [
            (
                &mut by_model_map,
                if ev.model_norm.is_empty() { "unknown".to_string() } else { ev.model_norm.clone() },
                if ev.model_raw.is_empty() { "unknown".to_string() } else { ev.model_raw.clone() },
            ),
            (&mut by_plat_map, ev.platform.clone(), ev.platform.clone()),
            (
                &mut by_ws_map,
                if ev.workspace.is_empty() { "(unspecified)".to_string() } else { ev.workspace.clone() },
                if ev.workspace.is_empty() { "(unspecified)".to_string() } else { ev.workspace.clone() },
            ),
        ] {
            let agg = map.entry(key).or_insert_with(|| AggBucket {
                label,
                ..Default::default()
            });
            agg.cost_usd += cost_usd;
            agg.input += ev.input_tokens;
            agg.output += ev.output_tokens;
            agg.cache_read += ev.cache_read_tokens;
            agg.cache_write += ev.cache_write_tokens;
            agg.sessions.insert(sess_key.clone());
            agg.events += 1;
            if is_priced {
                agg.priced = true;
            }
        }

        let s_agg = sess_map
            .entry((ev.platform.clone(), ev.session_id.clone()))
            .or_insert_with(|| SessionAgg {
                platform: ev.platform.clone(),
                workspace: ev.workspace.clone(),
                model: ev.model_norm.clone(),
                granularity: ev.granularity.clone(),
                ..Default::default()
            });
        s_agg.cost_usd += cost_usd;
        s_agg.input += ev.input_tokens;
        s_agg.output += ev.output_tokens;
        s_agg.cache_read += ev.cache_read_tokens;
        s_agg.cache_write += ev.cache_write_tokens;
        if ev.ts_ms > s_agg.last_ts_ms {
            s_agg.last_ts_ms = ev.ts_ms;
            if !ev.model_norm.is_empty() {
                s_agg.model = ev.model_norm;
            }
        }
    }

    kpis.total_tokens =
        kpis.input_tokens + kpis.output_tokens + kpis.cache_read_tokens + kpis.cache_write_tokens;
    kpis.cache_hit_rate = cache_hit_rate(kpis.input_tokens, kpis.cache_read_tokens);
    kpis.session_count = kpi_sessions.len();

    let to_rows = |map: HashMap<String, AggBucket>| -> Vec<BreakdownRow> {
        let mut list: Vec<BreakdownRow> = map
            .into_iter()
            .map(|(key, b)| {
                let total_tokens = b.input + b.output + b.cache_read + b.cache_write;
                BreakdownRow {
                    key,
                    label: b.label,
                    cost: b.cost_usd * rate_mult,
                    input_tokens: b.input,
                    output_tokens: b.output,
                    cache_read_tokens: b.cache_read,
                    cache_write_tokens: b.cache_write,
                    total_tokens,
                    cache_hit_rate: cache_hit_rate(b.input, b.cache_read),
                    session_count: b.sessions.len(),
                    event_count: b.events,
                    priced: b.priced,
                }
            })
            .collect();
        list.sort_by(|a, b| {
            b.cost
                .partial_cmp(&a.cost)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.total_tokens.cmp(&a.total_tokens))
        });
        list
    };

    let timeseries = series_map
        .into_iter()
        .map(|(bucket, v)| TimeseriesPoint {
            bucket,
            cost: v.0,
            input_tokens: v.1,
            output_tokens: v.2,
            cache_read_tokens: v.3,
            cache_write_tokens: v.4,
        })
        .collect();

    let mut top_sessions: Vec<TopSessionRow> = sess_map
        .into_iter()
        .map(|((_, sid), s)| TopSessionRow {
            session_id: sid,
            platform: s.platform,
            workspace: s.workspace,
            model: s.model,
            cost: s.cost_usd * rate_mult,
            total_tokens: s.input + s.output + s.cache_read + s.cache_write,
            input_tokens: s.input,
            output_tokens: s.output,
            cache_read_tokens: s.cache_read,
            last_ts_ms: s.last_ts_ms,
            granularity: s.granularity,
        })
        .collect();
    top_sessions.sort_by(|a, b| {
        b.cost
            .partial_cmp(&a.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.total_tokens.cmp(&a.total_tokens))
    });
    top_sessions.truncate(50);

    Ok(UsageDashboardData {
        kpis,
        hourly,
        timeseries,
        by_model: to_rows(by_model_map),
        by_platform: to_rows(by_plat_map),
        by_workspace: to_rows(by_ws_map),
        top_sessions,
        unpriced_models: unpriced.into_iter().collect(),
        available_models,
        available_workspaces,
        currency: pref.currency,
        exchange_rate: rate_mult,
    })
}

pub fn query_today_summary(app_data: &Path) -> Result<TodayUsageSummary, String> {
    let conn = open_usage_db(&usage_db_path(app_data))?;
    let resolver = load_resolver(&conn, app_data);
    let pref = get_currency_pref(&conn);
    let day_key = Local::now().format("%Y-%m-%d").to_string();

    let mut stmt = conn
        .prepare(
            "SELECT model_raw, model_norm, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens
             FROM usage_events WHERE day_key = ?1",
        )
        .map_err(|e| format!("Prepare today summary failed: {e}"))?;
    let rows = stmt
        .query_map(params![day_key], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?.max(0) as u64,
                r.get::<_, i64>(3)?.max(0) as u64,
                r.get::<_, i64>(4)?.max(0) as u64,
                r.get::<_, i64>(5)?.max(0) as u64,
            ))
        })
        .map_err(|e| format!("Query today summary failed: {e}"))?;

    let mut total_cost = 0.0;
    let mut total_tokens = 0u64;
    let mut has_priced_events = false;
    for (raw, norm, inp, out, cr, cw) in rows.flatten() {
        let (clean_raw, clean_norm) = sanitize_model_fields(raw, norm);
        total_tokens += inp + out + cr + cw;
        if let Some(rate) = resolver.resolve(&clean_raw, &clean_norm) {
            total_cost += compute_cost_usd(&rate, inp, out, cr, cw) * pref.rate;
            has_priced_events = true;
        }
    }
    Ok(TodayUsageSummary {
        day_key,
        total_cost,
        total_tokens,
        currency: pref.currency,
        has_priced_events,
    })
}

pub fn query_session_detail(
    app_data: &Path,
    platform: &str,
    session_id: &str,
) -> Result<Option<SessionUsageDetail>, String> {
    let conn = open_usage_db(&usage_db_path(app_data))?;
    let resolver = load_resolver(&conn, app_data);
    let pref = get_currency_pref(&conn);

    let mut stmt = conn
        .prepare(
            "SELECT model_raw, model_norm, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens, granularity
             FROM usage_events WHERE platform = ?1 AND session_id = ?2",
        )
        .map_err(|e| format!("Prepare session detail failed: {e}"))?;
    let rows = stmt
        .query_map(params![platform, session_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?.max(0) as u64,
                r.get::<_, i64>(3)?.max(0) as u64,
                r.get::<_, i64>(4)?.max(0) as u64,
                r.get::<_, i64>(5)?.max(0) as u64,
                r.get::<_, i64>(6)?.max(0) as u64,
                r.get::<_, String>(7)?,
            ))
        })
        .map_err(|e| format!("Query session detail failed: {e}"))?;

    let mut event_count = 0usize;
    let mut inp_sum = 0u64;
    let mut out_sum = 0u64;
    let mut cr_sum = 0u64;
    let mut cw_sum = 0u64;
    let mut reason_sum = 0u64;
    let mut cost = 0.0;
    let mut savings = 0.0;
    let mut priced = false;
    let mut models = BTreeSet::new();
    let mut granularity = String::from("session");

    for (raw, norm, inp, out, cr, cw, reason, gran) in rows.flatten() {
        let (clean_raw, clean_norm) = sanitize_model_fields(raw, norm);
        event_count += 1;
        inp_sum += inp;
        out_sum += out;
        cr_sum += cr;
        cw_sum += cw;
        reason_sum += reason;
        granularity = gran;
        if !clean_norm.is_empty() {
            models.insert(clean_norm.clone());
        }
        if let Some(rate) = resolver.resolve(&clean_raw, &clean_norm) {
            cost += compute_cost_usd(&rate, inp, out, cr, cw) * pref.rate;
            savings += compute_cache_savings_usd(&rate, cr) * pref.rate;
            priced = true;
        }
    }

    if event_count == 0 {
        return Ok(None);
    }

    Ok(Some(SessionUsageDetail {
        session_id: session_id.to_string(),
        platform: platform.to_string(),
        cost,
        cache_savings: savings,
        currency: pref.currency,
        input_tokens: inp_sum,
        output_tokens: out_sum,
        cache_read_tokens: cr_sum,
        cache_write_tokens: cw_sum,
        reasoning_tokens: reason_sum,
        total_tokens: inp_sum + out_sum + cr_sum + cw_sum,
        cache_hit_rate: cache_hit_rate(inp_sum, cr_sum),
        models: models.into_iter().collect(),
        event_count,
        granularity,
        priced,
    }))
}

pub fn query_logs(
    app_data: &Path,
    filter: &UsageFilter,
    page: usize,
    page_size: usize,
) -> Result<UsageLogPage, String> {
    let conn = open_usage_db(&usage_db_path(app_data))?;
    let resolver = load_resolver(&conn, app_data);
    let pref = get_currency_pref(&conn);
    let rate_mult = pref.rate;

    let page = page.max(1);
    let page_size = page_size.clamp(1, 200);
    let offset = (page - 1) * page_size;

    let start_ms = filter.start_ms.unwrap_or(0);
    let end_ms = filter.end_ms.unwrap_or(i64::MAX);
    let plat_f = filter.platform.as_deref().unwrap_or("all");
    let model_f = filter.model.as_deref().unwrap_or("all");
    let ws_f = filter.workspace.as_deref().unwrap_or("all");
    let sid_f = filter.session_id.as_deref().unwrap_or("");

    let total: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM usage_events
             WHERE ts_ms >= ?1 AND ts_ms <= ?2
               AND (?3 = 'all' OR platform = ?3)
               AND (?4 = 'all' OR model_norm = ?4)
               AND (?5 = 'all' OR workspace = ?5)
               AND (?6 = '' OR session_id = ?6)",
            params![start_ms, end_ms, plat_f, model_f, ws_f, sid_f],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| format!("Count usage logs failed: {e}"))?
        .max(0) as usize;

    let mut stmt = conn
        .prepare(
            "SELECT id, ts_ms, platform, session_id, workspace, model_raw, model_norm,
                    input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
                    reasoning_tokens, duration_ms, ttft_ms, granularity
             FROM usage_events
             WHERE ts_ms >= ?1 AND ts_ms <= ?2
               AND (?3 = 'all' OR platform = ?3)
               AND (?4 = 'all' OR model_norm = ?4)
               AND (?5 = 'all' OR workspace = ?5)
               AND (?6 = '' OR session_id = ?6)
             ORDER BY ts_ms DESC, id DESC
             LIMIT ?7 OFFSET ?8",
        )
        .map_err(|e| format!("Prepare usage logs query failed: {e}"))?;

    let rows = stmt
        .query_map(
            params![
                start_ms,
                end_ms,
                plat_f,
                model_f,
                ws_f,
                sid_f,
                page_size as i64,
                offset as i64
            ],
            |r| {
                let (model_raw, model_norm) =
                    sanitize_model_fields(r.get::<_, String>(5)?, r.get::<_, String>(6)?);
                let inp = r.get::<_, i64>(7)?.max(0) as u64;
                let out = r.get::<_, i64>(8)?.max(0) as u64;
                let cr = r.get::<_, i64>(9)?.max(0) as u64;
                let cw = r.get::<_, i64>(10)?.max(0) as u64;
                let reason = r.get::<_, i64>(11)?.max(0) as u64;
                let dur = r.get::<_, i64>(12)?.max(0) as u64;
                let ttft = r.get::<_, i64>(13)?.max(0) as u64;
                let (cost, priced) = match resolver.resolve(&model_raw, &model_norm) {
                    Some(rate) => (compute_cost_usd(&rate, inp, out, cr, cw) * rate_mult, true),
                    None => (0.0, false),
                };
                Ok(UsageLogRow {
                    id: r.get(0)?,
                    ts_ms: r.get(1)?,
                    platform: r.get(2)?,
                    session_id: r.get(3)?,
                    workspace: r.get(4)?,
                    model_raw,
                    model_norm,
                    input_tokens: inp,
                    output_tokens: out,
                    cache_read_tokens: cr,
                    cache_write_tokens: cw,
                    reasoning_tokens: reason,
                    total_tokens: inp + out + cr + cw,
                    cache_hit_rate: cache_hit_rate(inp, cr),
                    cost,
                    priced,
                    duration_ms: dur,
                    ttft_ms: ttft,
                    granularity: r.get(14)?,
                })
            },
        )
        .map_err(|e| format!("Query usage logs failed: {e}"))?;

    Ok(UsageLogPage {
        items: rows.flatten().collect(),
        total,
        page,
        page_size,
        currency: pref.currency,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::usage::db::{save_pricing_override, set_currency_pref};
    use crate::services::usage::sync::sync_usage_with_roots;
    use crate::services::usage::PricingRate;
    use std::path::PathBuf;

    struct TestDir(PathBuf);
    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_usage_query_{}_{}",
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
    fn dashboard_aggregates_tokens_cost_and_session_detail() {
        let dir = TestDir::new("agg");
        let app_data = dir.path().join("app");
        let home = dir.path().join("home");
        let proj = home.join(".claude/projects/repo");
        std::fs::create_dir_all(&proj).unwrap();
        let sess_id = "22222222-2222-2222-2222-222222222222";
        let now_rfc = chrono::Utc::now().to_rfc3339();
        std::fs::write(
            proj.join(format!("{sess_id}.jsonl")),
            format!(
                "{{\"type\":\"assistant\",\"cwd\":\"/tmp/repo\",\"timestamp\":\"{now_rfc}\",\"message\":{{\"id\":\"m1\",\"model\":\"claude-sonnet-4-5\",\"usage\":{{\"input_tokens\":500000,\"output_tokens\":100000,\"cache_read_input_tokens\":500000,\"cache_creation_input_tokens\":0}}}}}}\n"
            ),
        )
        .unwrap();

        sync_usage_with_roots(&app_data, &home, &HashMap::new(), false).unwrap();

        let conn = open_usage_db(&usage_db_path(&app_data)).unwrap();
        save_pricing_override(
            &conn,
            "claude-sonnet-4-5",
            PricingRate {
                input: 3.0,
                output: 15.0,
                cache_read: 0.3,
                cache_write: 3.75,
            },
            0,
        )
        .unwrap();
        set_currency_pref(&conn, "CNY", Some(7.0)).unwrap();
        drop(conn);

        let dash = query_dashboard(&app_data, &UsageFilter::default()).unwrap();
        // USD = 0.5*3 + 0.1*15 + 0.5*0.3 = 1.5 + 1.5 + 0.15 = 3.15; CNY*7 = 22.05
        assert!((dash.kpis.total_cost - 22.05).abs() < 1e-6);
        assert_eq!(dash.kpis.total_tokens, 1_100_000);
        assert_eq!(dash.kpis.cache_hit_rate, 50.0);
        assert_eq!(dash.by_model.len(), 1);
        assert_eq!(dash.top_sessions.len(), 1);

        let detail = query_session_detail(&app_data, "claude", sess_id)
            .unwrap()
            .expect("session detail");
        assert_eq!(detail.total_tokens, 1_100_000);
        assert!((detail.cost - 22.05).abs() < 1e-6);

        let logs = query_logs(
            &app_data,
            &UsageFilter {
                platform: Some("claude".to_string()),
                session_id: Some(sess_id.to_string()),
                ..Default::default()
            },
            1,
            25,
        )
        .unwrap();
        assert_eq!(logs.total, 1);
        assert_eq!(logs.items.len(), 1);
        assert_eq!(logs.items[0].model_norm, "claude-sonnet-4-5");
        assert!((logs.items[0].cache_hit_rate - 50.0).abs() < 1e-6);
        assert!((logs.items[0].cost - 22.05).abs() < 1e-6);
    }
}
