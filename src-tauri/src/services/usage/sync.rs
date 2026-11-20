use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, UNIX_EPOCH};

use chrono::{Local, TimeZone};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::adapters::{to_millis, user_home};

use super::db::{
    count_events, get_parser_version, load_source_watermarks, open_usage_db, prune_missing_sources,
    replace_source_events, set_parser_version, usage_db_path, SourceWatermark,
    USAGE_PARSER_VERSION,
};
use super::pricing::{extract_model_from_value, extract_model_id, normalize_model_id};
use super::{UsageEvent, UsageSyncStatus};

static USAGE_SYNC_LOCK: Mutex<()> = Mutex::new(());
crate::lazy_regex!(
    USAGE_CODEX_UUID_RE,
    r"([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})"
);

#[derive(Debug, Clone)]
pub(crate) struct DiscoveredSource {
    pub source_key: String,
    pub platform: &'static str,
    pub path: PathBuf,
    pub mtime_ms: i64,
    pub size_bytes: i64,
    pub kind: SourceKind,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SourceKind {
    ClaudeJsonl,
    OpenClaudeJsonl,
    CodexJsonl,
    PiJsonl,
    OmpJsonl,
    AntigravityJsonl,
    CodeBuddyJsonl,
    WorkBuddyJsonl,
    QwenJsonl,
    OpenCodeSqlite,
    CodeWizSqlite,
    ZCodeSqlite,
    DshProjJson,
    GrokUsageJson,
}

pub(crate) fn day_and_hour_keys(ts_ms: i64) -> (String, String) {
    let nts = to_millis(ts_ms);
    if nts <= 0 {
        let now = Local::now();
        return (
            now.format("%Y-%m-%d").to_string(),
            now.format("%Y-%m-%d %H:00").to_string(),
        );
    }
    if let Some(dt) = Local.timestamp_millis_opt(nts).single() {
        (
            dt.format("%Y-%m-%d").to_string(),
            dt.format("%Y-%m-%d %H:00").to_string(),
        )
    } else {
        let now = Local::now();
        (
            now.format("%Y-%m-%d").to_string(),
            now.format("%Y-%m-%d %H:00").to_string(),
        )
    }
}

fn file_watermark(path: &Path) -> Option<(i64, i64)> {
    let meta = fs::metadata(path).ok()?;
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let size_bytes = meta.len() as i64;
    Some((mtime_ms, size_bytes))
}

const MAX_DERIVED_LATENCY_MS: i64 = 600_000;

fn delta_ms(start: i64, end: i64) -> u64 {
    if start > 0 && end > start && (end - start) < MAX_DERIVED_LATENCY_MS {
        (end - start) as u64
    } else {
        0
    }
}

fn parse_ts_value(v: &Value, fallback_ms: i64) -> i64 {
    if let Some(n) = v.as_i64() {
        let ms = to_millis(n);
        if ms > 0 {
            return ms;
        }
    }
    if let Some(s) = v.as_str() {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
            return dt.timestamp_millis();
        }
    }
    fallback_ms
}

#[allow(clippy::too_many_arguments)]
fn make_event(
    source_key: &str,
    platform: &str,
    session_id: &str,
    workspace: &str,
    model_raw: &str,
    ts_ms: i64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    reasoning_tokens: u64,
    granularity: &str,
) -> Option<UsageEvent> {
    make_event_timed(
        source_key,
        platform,
        session_id,
        workspace,
        model_raw,
        ts_ms,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
        reasoning_tokens,
        granularity,
        0,
        0,
    )
}

#[allow(clippy::too_many_arguments)]
fn make_event_timed(
    source_key: &str,
    platform: &str,
    session_id: &str,
    workspace: &str,
    model_raw: &str,
    ts_ms: i64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    reasoning_tokens: u64,
    granularity: &str,
    duration_ms: u64,
    ttft_ms: u64,
) -> Option<UsageEvent> {
    if input_tokens == 0
        && output_tokens == 0
        && cache_read_tokens == 0
        && cache_write_tokens == 0
        && reasoning_tokens == 0
    {
        return None;
    }
    let extracted_model = extract_model_id(model_raw);
    let clean_model = if extracted_model.is_empty() {
        platform.to_string()
    } else {
        extracted_model
    };
    let (day_key, hour_key) = day_and_hour_keys(ts_ms);
    Some(UsageEvent {
        source_key: source_key.to_string(),
        platform: platform.to_string(),
        session_id: session_id.to_string(),
        workspace: workspace.to_string(),
        model_raw: clean_model.clone(),
        model_norm: normalize_model_id(&clean_model),
        ts_ms: to_millis(ts_ms),
        day_key,
        hour_key,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
        reasoning_tokens,
        granularity: granularity.to_string(),
        duration_ms,
        ttft_ms,
    })
}

fn parse_claude_like(source: &DiscoveredSource, platform: &str) -> Vec<UsageEvent> {
    let Ok(file) = File::open(&source.path) else {
        return Vec::new();
    };
    let sid = source
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let mut cwd = String::new();
    let mut dedup: HashMap<String, UsageEvent> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut last_trigger_ts: Option<i64> = None;
    let mut first_chunk_ts: HashMap<String, i64> = HashMap::new();
    let mut trigger_for_msg: HashMap<String, i64> = HashMap::new();
    let reader = BufReader::new(file);

    for (idx, line) in reader.lines().map_while(Result::ok).enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if cwd.is_empty() {
            if let Some(c) = val.get("cwd").and_then(Value::as_str) {
                cwd = c.to_string();
            }
        }
        let kind = val.get("type").and_then(Value::as_str).unwrap_or("");
        if kind == "user" {
            let uts = val.get("timestamp").map(|t| parse_ts_value(t, 0)).unwrap_or(0);
            if uts > 0 {
                last_trigger_ts = Some(uts);
            }
            continue;
        }
        if kind == "system" && val.get("subtype").and_then(Value::as_str) == Some("turn_duration") {
            if let Some(dur) = val
                .get("durationMs")
                .or_else(|| val.get("duration_ms"))
                .and_then(Value::as_u64)
            {
                if let Some(last_key) = order.last() {
                    if let Some(ev) = dedup.get_mut(last_key) {
                        if dur > 0 {
                            ev.duration_ms = dur;
                        }
                    }
                }
            }
            continue;
        }
        if kind != "assistant" {
            continue;
        }
        let Some(msg) = val.get("message") else {
            continue;
        };
        let Some(usage) = msg.get("usage") else {
            continue;
        };
        let model = msg
            .get("model")
            .and_then(Value::as_str)
            .filter(|m| *m != "<synthetic>")
            .unwrap_or(platform);
        let ts_ms = val
            .get("timestamp")
            .map(|t| parse_ts_value(t, source.mtime_ms))
            .unwrap_or(source.mtime_ms);
        let msg_key = msg
            .get("id")
            .and_then(Value::as_str)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("line-{idx}"));
        let first_ts = *first_chunk_ts.entry(msg_key.clone()).or_insert(ts_ms);
        let trig_ts = *trigger_for_msg
            .entry(msg_key.clone())
            .or_insert_with(|| last_trigger_ts.unwrap_or(0));
        let explicit_dur = val
            .get("durationMs")
            .or_else(|| val.get("duration_ms"))
            .or_else(|| msg.get("durationMs"))
            .or_else(|| msg.get("duration_ms"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let explicit_ttft = val
            .get("ttftMs")
            .or_else(|| val.get("ttft_ms"))
            .or_else(|| msg.get("ttftMs"))
            .or_else(|| msg.get("ttft_ms"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let ttft_ms = if explicit_ttft > 0 {
            explicit_ttft
        } else {
            delta_ms(trig_ts, first_ts)
        };
        let dur_ms = if explicit_dur > 0 {
            explicit_dur
        } else {
            let from_trig = delta_ms(trig_ts, ts_ms);
            if from_trig > 0 {
                from_trig
            } else {
                delta_ms(first_ts, ts_ms)
            }
        };
        let inp = usage.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
        let out = usage.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
        let cr = usage
            .get("cache_read_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let cw = usage
            .get("cache_creation_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if let Some(ev) = make_event_timed(
            &source.source_key,
            platform,
            &sid,
            &cwd,
            model,
            ts_ms,
            inp,
            out,
            cr,
            cw,
            0,
            "message",
            dur_ms,
            ttft_ms,
        ) {
            if !dedup.contains_key(&msg_key) {
                order.push(msg_key.clone());
            }
            dedup.insert(msg_key, ev);
        }
    }

    order.into_iter().filter_map(|k| dedup.remove(&k)).collect()
}

fn parse_codex_jsonl(source: &DiscoveredSource) -> Vec<UsageEvent> {
    let Ok(file) = File::open(&source.path) else {
        return Vec::new();
    };
    let stem = source
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let sid = USAGE_CODEX_UUID_RE
        .find_iter(stem)
        .last()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| stem.to_string());

    let mut cwd = String::new();
    let mut model = String::from("codex");
    let mut events = Vec::new();
    let mut max_total_fallback: Option<UsageEvent> = None;
    let mut last_trigger_ts: Option<i64> = None;
    let mut first_assistant_ts: Option<i64> = None;

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(val) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let kind = val.get("type").and_then(Value::as_str).unwrap_or("");
        let payload = val.get("payload");
        let ts_ms = val
            .get("timestamp")
            .map(|t| parse_ts_value(t, source.mtime_ms))
            .unwrap_or(source.mtime_ms);

        if kind == "session_meta" {
            if let Some(p) = payload {
                if let Some(c) = p.get("cwd").and_then(Value::as_str) {
                    cwd = c.to_string();
                }
            }
        } else if kind == "turn_context" {
            if let Some(p) = payload {
                if let Some(m) = p.get("model").and_then(Value::as_str) {
                    if !m.trim().is_empty() {
                        model = m.to_string();
                    }
                }
            }
            if ts_ms > 0 {
                last_trigger_ts = Some(ts_ms);
                first_assistant_ts = None;
            }
        } else if kind == "response_item" {
            if let Some(p) = payload {
                let ptype = p.get("type").and_then(Value::as_str).unwrap_or("");
                let role = p.get("role").and_then(Value::as_str).unwrap_or("");
                if ptype == "function_call_output" || role == "user" {
                    if ts_ms > 0 {
                        last_trigger_ts = Some(ts_ms);
                        first_assistant_ts = None;
                    }
                } else if (ptype == "reasoning" || role == "assistant") && ts_ms > 0 {
                    first_assistant_ts.get_or_insert(ts_ms);
                }
            }
        } else if kind == "token_usage" || kind == "usage" {
            if let Some(p) = payload {
                let inp = p
                    .get("prompt_tokens")
                    .or_else(|| p.get("input_tokens"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let out = p
                    .get("completion_tokens")
                    .or_else(|| p.get("output_tokens"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let cr = p
                    .get("cached_input_tokens")
                    .or_else(|| p.get("cache_read_input_tokens"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let reason = p
                    .get("reasoning_output_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let ttft_ms = match (last_trigger_ts, first_assistant_ts) {
                    (Some(t0), Some(t1)) => delta_ms(t0, t1),
                    _ => 0,
                };
                let duration_ms = last_trigger_ts.map(|t0| delta_ms(t0, ts_ms)).unwrap_or(0);
                let uncached_inp = inp.saturating_sub(cr);
                if let Some(ev) = make_event_timed(
                    &source.source_key,
                    "codex",
                    &sid,
                    &cwd,
                    &model,
                    ts_ms,
                    uncached_inp,
                    out,
                    cr,
                    0,
                    reason,
                    "message",
                    duration_ms,
                    ttft_ms,
                ) {
                    events.push(ev);
                    last_trigger_ts = Some(ts_ms);
                    first_assistant_ts = None;
                }
            }
        } else if kind == "event_msg" {
            if let Some(p) = payload {
                let ptype = p.get("type").and_then(Value::as_str);
                if ptype == Some("user_message") && ts_ms > 0 {
                    last_trigger_ts = Some(ts_ms);
                    first_assistant_ts = None;
                } else if ptype == Some("token_count") {
                    if let Some(info) = p.get("info") {
                        if let Some(last) = info.get("last_token_usage") {
                            let inp = last.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
                            let out = last.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
                            let cr = last
                                .get("cached_input_tokens")
                                .and_then(Value::as_u64)
                                .unwrap_or(0);
                            let reason = last
                                .get("reasoning_output_tokens")
                                .and_then(Value::as_u64)
                                .unwrap_or(0);
                            let ttft_ms = match (last_trigger_ts, first_assistant_ts) {
                                (Some(t0), Some(t1)) => delta_ms(t0, t1),
                                _ => 0,
                            };
                            let duration_ms = last_trigger_ts.map(|t0| delta_ms(t0, ts_ms)).unwrap_or(0);
                            let uncached_inp = inp.saturating_sub(cr);
                            if let Some(ev) = make_event_timed(
                                &source.source_key,
                                "codex",
                                &sid,
                                &cwd,
                                &model,
                                ts_ms,
                                uncached_inp,
                                out,
                                cr,
                                0,
                                reason,
                                "message",
                                duration_ms,
                                ttft_ms,
                            ) {
                                events.push(ev);
                                last_trigger_ts = Some(ts_ms);
                                first_assistant_ts = None;
                            }
                        } else if let Some(tot) = info.get("total_token_usage") {
                            let inp = tot.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
                            let out = tot.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
                            let cr = tot
                                .get("cached_input_tokens")
                                .and_then(Value::as_u64)
                                .unwrap_or(0);
                            let reason = tot
                                .get("reasoning_output_tokens")
                                .and_then(Value::as_u64)
                                .unwrap_or(0);
                            let uncached_inp = inp.saturating_sub(cr);
                            if let Some(ev) = make_event(
                                &source.source_key,
                                "codex",
                                &sid,
                                &cwd,
                                &model,
                                ts_ms,
                                uncached_inp,
                                out,
                                cr,
                                0,
                                reason,
                                "session",
                            ) {
                                max_total_fallback = Some(ev);
                            }
                        }
                    }
                }
            }
        }
    }

    if events.is_empty() {
        if let Some(ev) = max_total_fallback {
            events.push(ev);
        }
    }
    events
}

fn parse_pi_like(source: &DiscoveredSource, platform: &str) -> Vec<UsageEvent> {
    let Ok(file) = File::open(&source.path) else {
        return Vec::new();
    };
    let mut sid = source
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let mut cwd = String::new();
    let mut current_model = platform.to_string();
    let mut events = Vec::new();
    let mut last_trigger_ts: Option<i64> = None;

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let ltype = val.get("type").and_then(Value::as_str).unwrap_or("");
        if ltype == "session" {
            if let Some(id) = val.get("id").and_then(Value::as_str) {
                if !id.is_empty() {
                    sid = id.to_string();
                }
            }
            if let Some(c) = val.get("cwd").and_then(Value::as_str) {
                cwd = c.to_string();
            }
        } else if ltype == "model_change" {
            if let Some(m) = val.get("modelId").and_then(Value::as_str) {
                if !m.is_empty() {
                    current_model = m.to_string();
                }
            }
        } else if ltype == "message" {
            if let Some(msg) = val.get("message") {
                let role = msg.get("role").and_then(Value::as_str).unwrap_or("");
                let ts_ms = val
                    .get("timestamp")
                    .or_else(|| msg.get("timestamp"))
                    .map(|t| parse_ts_value(t, source.mtime_ms))
                    .unwrap_or(source.mtime_ms);
                if role != "assistant" {
                    if ts_ms > 0 {
                        last_trigger_ts = Some(ts_ms);
                    }
                    continue;
                }
                let model = msg
                    .get("model")
                    .and_then(Value::as_str)
                    .filter(|m| !m.is_empty())
                    .unwrap_or(&current_model);
                if let Some(usage) = msg.get("usage") {
                    let inp = usage.get("input").and_then(Value::as_u64).unwrap_or(0);
                    let out = usage.get("output").and_then(Value::as_u64).unwrap_or(0);
                    let cr = usage.get("cacheRead").and_then(Value::as_u64).unwrap_or(0);
                    let cw = usage.get("cacheWrite").and_then(Value::as_u64).unwrap_or(0);
                    let reason = usage.get("reasoning").and_then(Value::as_u64).unwrap_or(0);
                    let explicit_dur = val
                        .get("durationMs")
                        .or_else(|| msg.get("durationMs"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    let ttft_ms = val
                        .get("ttftMs")
                        .or_else(|| msg.get("ttftMs"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    let dur_ms = if explicit_dur > 0 {
                        explicit_dur
                    } else {
                        last_trigger_ts.map(|t0| delta_ms(t0, ts_ms)).unwrap_or(0)
                    };
                    if let Some(ev) = make_event_timed(
                        &source.source_key,
                        platform,
                        &sid,
                        &cwd,
                        model,
                        ts_ms,
                        inp,
                        out,
                        cr,
                        cw,
                        reason,
                        "message",
                        dur_ms,
                        ttft_ms,
                    ) {
                        events.push(ev);
                        last_trigger_ts = Some(ts_ms);
                    }
                }
            }
        }
    }
    events
}

fn pb_read_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut val = 0u64;
    let mut shift = 0u32;
    while *pos < buf.len() {
        let b = buf[*pos];
        *pos += 1;
        val |= u64::from(b & 0x7f).checked_shl(shift)?;
        if (b & 0x80) == 0 {
            return Some(val);
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    None
}

fn pb_for_each_field<'a>(buf: &'a [u8], mut f: impl FnMut(u32, u8, u64, &'a [u8])) {
    let mut pos = 0usize;
    while pos < buf.len() {
        let Some(tag) = pb_read_varint(buf, &mut pos) else {
            break;
        };
        let fnum = (tag >> 3) as u32;
        let wtype = (tag & 7) as u8;
        match wtype {
            0 => {
                let Some(v) = pb_read_varint(buf, &mut pos) else {
                    break;
                };
                f(fnum, wtype, v, &[]);
            }
            1 => {
                if pos + 8 > buf.len() {
                    break;
                }
                let v = u64::from_le_bytes(buf[pos..pos + 8].try_into().unwrap_or([0; 8]));
                pos += 8;
                f(fnum, wtype, v, &[]);
            }
            2 => {
                let Some(len) = pb_read_varint(buf, &mut pos) else {
                    break;
                };
                let Ok(len_us) = usize::try_from(len) else {
                    break;
                };
                if pos.saturating_add(len_us) > buf.len() {
                    break;
                }
                let slice = &buf[pos..pos + len_us];
                pos += len_us;
                f(fnum, wtype, len, slice);
            }
            5 => {
                if pos + 4 > buf.len() {
                    break;
                }
                pos += 4;
            }
            _ => break,
        }
    }
}

fn pb_timestamp_or_duration_ms(buf: &[u8]) -> i64 {
    let mut secs = 0i64;
    let mut nanos = 0i64;
    pb_for_each_field(buf, |fnum, wtype, v, _| {
        if wtype == 0 {
            if fnum == 1 {
                secs = v as i64;
            } else if fnum == 2 {
                nanos = v as i64;
            }
        }
    });
    secs.saturating_mul(1000).saturating_add(nanos / 1_000_000)
}

#[derive(Default, Clone, Copy)]
struct AgStepTiming {
    start_ms: i64,
    ttft_ms: u64,
    duration_ms: u64,
    reasoning_tokens: u64,
}

fn agy_conv_db_path(transcript_path: &Path, sid: &str) -> Option<PathBuf> {
    if sid.is_empty() {
        return None;
    }
    let root = transcript_path
        .parent()?
        .parent()?
        .parent()?
        .parent()?
        .parent()?;
    Some(root.join("conversations").join(format!("{sid}.db")))
}

fn load_antigravity_step_timings(transcript_path: &Path, sid: &str) -> HashMap<i64, AgStepTiming> {
    let mut timings = HashMap::new();
    let Some(db_path) = agy_conv_db_path(transcript_path, sid) else {
        return timings;
    };
    if !db_path.is_file() {
        return timings;
    }
    let Ok(conn) = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    ) else {
        return timings;
    };

    let mut gen_map: HashMap<i64, (u64, u64)> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT idx, data FROM gen_metadata WHERE data IS NOT NULL") {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))
        {
            for (idx, data) in rows.flatten() {
                let mut ttft = 0i64;
                let mut stream = 0i64;
                pb_for_each_field(&data, |fnum, wtype, _, slice| {
                    if fnum == 1 && wtype == 2 {
                        pb_for_each_field(slice, |sf, sw, _, sub| {
                            if sw == 2 {
                                if sf == 11 {
                                    ttft = pb_timestamp_or_duration_ms(sub);
                                } else if sf == 12 {
                                    stream = pb_timestamp_or_duration_ms(sub);
                                }
                            }
                        });
                    }
                });
                let ttft_u = if ttft > 0 && ttft < 3_600_000 { ttft as u64 } else { 0 };
                let dur_sum = ttft.saturating_add(stream.max(0));
                let total_u = if dur_sum > 0 && dur_sum < 3_600_000 {
                    dur_sum as u64
                } else {
                    0
                };
                if ttft_u > 0 || total_u > 0 {
                    gen_map.insert(idx, (ttft_u, total_u));
                }
            }
        }
    }

    let Ok(mut stmt) =
        conn.prepare("SELECT idx, metadata FROM steps WHERE step_type = 15 AND metadata IS NOT NULL")
    else {
        return timings;
    };
    let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))) else {
        return timings;
    };

    for (idx, meta) in rows.flatten() {
        let mut start_ms = 0i64;
        let mut first_token_ms = 0i64;
        let mut completed_ms = 0i64;
        let mut fallback_end_ms = 0i64;
        let mut reasoning_tokens = 0u64;
        let mut gen_idx = 0i64;

        pb_for_each_field(&meta, |fnum, wtype, _, slice| {
            if wtype != 2 {
                return;
            }
            match fnum {
                1 => start_ms = pb_timestamp_or_duration_ms(slice),
                6 => first_token_ms = pb_timestamp_or_duration_ms(slice),
                7 => completed_ms = pb_timestamp_or_duration_ms(slice),
                8 | 32 => {
                    let ts = pb_timestamp_or_duration_ms(slice);
                    if ts > fallback_end_ms {
                        fallback_end_ms = ts;
                    }
                }
                9 => {
                    pb_for_each_field(slice, |uf, uw, uv, _| {
                        if uf == 9 && uw == 0 {
                            reasoning_tokens = uv;
                        }
                    });
                }
                20 => {
                    pb_for_each_field(slice, |mf, mw, mv, _| {
                        if mf == 3 && mw == 0 {
                            gen_idx = mv as i64;
                        }
                    });
                }
                _ => {}
            }
        });

        let end_ms = if completed_ms > 0 {
            completed_ms
        } else {
            fallback_end_ms
        };
        let mut ttft_ms = if start_ms > 0 && first_token_ms >= start_ms && (first_token_ms - start_ms) < 3_600_000 {
            (first_token_ms - start_ms) as u64
        } else {
            0
        };
        let mut duration_ms = if start_ms > 0 && end_ms >= start_ms && (end_ms - start_ms) < 3_600_000 {
            (end_ms - start_ms) as u64
        } else {
            0
        };
        if let Some(&(g_ttft, g_total)) = gen_map.get(&gen_idx) {
            if ttft_ms == 0 {
                ttft_ms = g_ttft;
            }
            if duration_ms == 0 {
                duration_ms = g_total;
            }
        }
        timings.insert(
            idx,
            AgStepTiming {
                start_ms,
                ttft_ms,
                duration_ms,
                reasoning_tokens,
            },
        );
    }
    timings
}

fn parse_antigravity_jsonl(source: &DiscoveredSource) -> Vec<UsageEvent> {
    let Ok(file) = File::open(&source.path) else {
        return Vec::new();
    };
    // .../.gemini/antigravity/brain/<sid>/.system_generated/logs/transcript.jsonl
    let sid = source
        .path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let db_timings = load_antigravity_step_timings(&source.path, &sid);
    let mut cwd = String::new();
    struct AgStep {
        step_index: Option<i64>,
        kind: String,
        ts_ms: i64,
        model: String,
        inp: u64,
        out: u64,
        cr: u64,
    }
    let mut steps: Vec<AgStep> = Vec::new();

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let mtype = v.get("type").and_then(Value::as_str).unwrap_or("");
        if mtype == "USER_INPUT" && cwd.is_empty() {
            if let Some(content) = v.get("content").and_then(Value::as_str) {
                for l in content.lines() {
                    if let Some(idx) = l.find("Active Workspaces:") {
                        let _ = idx;
                    } else if l.trim_start().starts_with("- /") {
                        cwd = l.trim_start().trim_start_matches("- ").trim().to_string();
                        break;
                    }
                }
            }
        }
        let ts_ms = v
            .get("created_at")
            .map(|t| parse_ts_value(t, 0))
            .unwrap_or(0);
        let step_index = v.get("step_index").and_then(Value::as_i64);
        let (model, inp, out, cr) = if mtype == "PLANNER_RESPONSE" {
            (
                v.get("model")
                    .and_then(Value::as_str)
                    .filter(|m| !m.is_empty())
                    .unwrap_or("gemini-2.5-pro")
                    .to_string(),
                v.get("input_tokens").and_then(Value::as_u64).unwrap_or(0),
                v.get("output_tokens").and_then(Value::as_u64).unwrap_or(0),
                v.get("cache_read_tokens").and_then(Value::as_u64).unwrap_or(0),
            )
        } else {
            (String::new(), 0, 0, 0)
        };
        steps.push(AgStep {
            step_index,
            kind: mtype.to_string(),
            ts_ms,
            model,
            inp,
            out,
            cr,
        });
    }

    let mut events = Vec::new();
    for i in 0..steps.len() {
        if steps[i].kind != "PLANNER_RESPONSE" {
            continue;
        }
        let fallback_dur = if i + 1 < steps.len() && steps[i + 1].kind != "USER_INPUT" {
            delta_ms(steps[i].ts_ms, steps[i + 1].ts_ms)
        } else if i > 0 && steps[i - 1].kind == "USER_INPUT" {
            delta_ms(steps[i - 1].ts_ms, steps[i].ts_ms)
        } else {
            0
        };
        let timing = steps[i].step_index.and_then(|idx| db_timings.get(&idx)).copied();
        let ev_ts = match timing.filter(|t| t.start_ms > 0) {
            Some(t) => t.start_ms,
            None if steps[i].ts_ms > 0 => steps[i].ts_ms,
            None => source.mtime_ms,
        };
        let dur_ms = match timing.filter(|t| t.duration_ms > 0) {
            Some(t) => t.duration_ms,
            None => fallback_dur,
        };
        let ttft_ms = timing.map(|t| t.ttft_ms).unwrap_or(0);
        let reasoning_tokens = timing.map(|t| t.reasoning_tokens).unwrap_or(0);
        if let Some(ev) = make_event_timed(
            &source.source_key,
            "antigravity",
            &sid,
            &cwd,
            &steps[i].model,
            ev_ts,
            steps[i].inp,
            steps[i].out,
            steps[i].cr,
            0,
            reasoning_tokens,
            "message",
            dur_ms,
            ttft_ms,
        ) {
            events.push(ev);
        }
    }
    events
}

fn parse_buddy_jsonl(source: &DiscoveredSource, platform: &str) -> Vec<UsageEvent> {
    let Ok(file) = File::open(&source.path) else {
        return Vec::new();
    };
    let sid = source
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let mut cwd = String::new();
    let mut model = platform.to_string();
    let mut events = Vec::new();
    let mut last_trigger_ts: Option<i64> = None;

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(val) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if cwd.is_empty() {
            if let Some(c) = val.get("cwd").and_then(Value::as_str) {
                cwd = c.to_string();
            }
        }
        if let Some(m) = val.get("model").and_then(Value::as_str) {
            if !m.is_empty() {
                model = m.to_string();
            }
        }
        let ts_ms = val
            .get("timestamp")
            .map(|t| parse_ts_value(t, source.mtime_ms))
            .unwrap_or(source.mtime_ms);
        let usage_node = val
            .get("usage")
            .or_else(|| val.get("providerData").and_then(|p| p.get("usage")));
        if let Some(u) = usage_node {
            let inp = u
                .get("input")
                .or_else(|| u.get("promptTokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let out = u
                .get("output")
                .or_else(|| u.get("completionTokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let cr = u
                .get("cacheRead")
                .or_else(|| u.get("cachedInputTokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let cw = u.get("cacheWrite").and_then(Value::as_u64).unwrap_or(0);
            let reason = u
                .get("reasoning")
                .or_else(|| u.get("reasoningTokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let explicit_dur = val
                .get("durationMs")
                .or_else(|| val.get("duration_ms"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let ttft_ms = val
                .get("ttftMs")
                .or_else(|| val.get("ttft_ms"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let dur_ms = if explicit_dur > 0 {
                explicit_dur
            } else {
                last_trigger_ts.map(|t0| delta_ms(t0, ts_ms)).unwrap_or(0)
            };
            if let Some(ev) = make_event_timed(
                &source.source_key,
                platform,
                &sid,
                &cwd,
                &model,
                ts_ms,
                inp,
                out,
                cr,
                cw,
                reason,
                "message",
                dur_ms,
                ttft_ms,
            ) {
                events.push(ev);
                last_trigger_ts = Some(ts_ms);
            }
        } else if ts_ms > 0 {
            last_trigger_ts = Some(ts_ms);
        }
    }
    events
}

fn parse_qwen_jsonl(source: &DiscoveredSource) -> Vec<UsageEvent> {
    let Ok(file) = File::open(&source.path) else {
        return Vec::new();
    };
    let sid = source
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let mut cwd = String::new();
    let mut model = String::from("qwen-2.5-coder");
    let mut max_prompt = 0u64;
    let mut total_comp = 0u64;
    let mut max_cr = 0u64;
    let mut last_ts = source.mtime_ms;

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(val) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if cwd.is_empty() {
            if let Some(c) = val.get("cwd").and_then(Value::as_str) {
                cwd = c.to_string();
            }
        }
        if let Some(m) = val.get("model").and_then(Value::as_str) {
            if !m.is_empty() {
                model = m.to_string();
            }
        }
        if let Some(ts) = val.get("timestamp") {
            last_ts = parse_ts_value(ts, last_ts);
        }
        if let Some(usage) = val.get("usageMetadata") {
            let p = usage.get("promptTokenCount").and_then(Value::as_u64).unwrap_or(0);
            let c = usage.get("candidatesTokenCount").and_then(Value::as_u64).unwrap_or(0);
            let cr = usage
                .get("cachedContentTokenCount")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if p > max_prompt {
                max_prompt = p;
            }
            total_comp += c;
            if cr > max_cr {
                max_cr = cr;
            }
        }
    }
    make_event(
        &source.source_key,
        "qwen",
        &sid,
        &cwd,
        &model,
        last_ts,
        max_prompt.saturating_sub(max_cr),
        total_comp,
        max_cr,
        0,
        0,
        "session",
    )
    .into_iter()
    .collect()
}

#[allow(clippy::let_and_return)]
fn table_has_column(conn: &Connection, table: &str, column: &str) -> bool {
    let sql = format!("PRAGMA table_info({table})");
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return false;
    };
    let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(1)) else {
        return false;
    };
    let found = rows.flatten().any(|c| c == column);
    found
}

fn parse_opencode_sqlite(source: &DiscoveredSource, platform: &str) -> Vec<UsageEvent> {
    let Ok(conn) = Connection::open_with_flags(
        &source.path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    ) else {
        return Vec::new();
    };
    let has_v2 = conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='session_v2'")
        .and_then(|mut s| s.exists([]))
        .unwrap_or(false);
    let table = if has_v2 { "session_v2" } else { "session" };
    let mut events = Vec::new();
    let mut covered_sids = HashSet::new();

    let has_messages = conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='session_message'")
        .and_then(|mut s| s.exists([]))
        .unwrap_or(false);
    if has_messages {
        let msg_sql = format!(
            "SELECT m.session_id, COALESCE(s.directory, ''), m.time_created, m.data
             FROM session_message m
             LEFT JOIN {table} s ON s.id = m.session_id
             WHERE m.type = 'assistant'
             ORDER BY m.time_created ASC, m.seq ASC"
        );
        if let Ok(mut stmt) = conn.prepare(&msg_sql) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            }) {
                for (sid, dir, row_ts, data_str) in rows.flatten() {
                    let Ok(val) = serde_json::from_str::<Value>(&data_str) else {
                        continue;
                    };
                    let Some(tok) = val.get("tokens") else {
                        continue;
                    };
                    let inp = tok.get("input").and_then(Value::as_u64).unwrap_or(0);
                    let out = tok.get("output").and_then(Value::as_u64).unwrap_or(0);
                    let reason = tok.get("reasoning").and_then(Value::as_u64).unwrap_or(0);
                    let cache = tok.get("cache");
                    let cr = cache
                        .and_then(|c| c.get("read"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    let cw = cache
                        .and_then(|c| c.get("write"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    let model = val
                        .get("model")
                        .and_then(extract_model_from_value)
                        .or_else(|| {
                            val.get("modelID")
                                .and_then(Value::as_str)
                                .map(|s| s.to_string())
                        })
                        .unwrap_or_default();
                    let time_obj = val.get("time");
                    let created_ms = time_obj
                        .and_then(|t| t.get("created"))
                        .map(|v| parse_ts_value(v, row_ts))
                        .unwrap_or_else(|| {
                            if row_ts > 0 {
                                to_millis(row_ts)
                            } else {
                                source.mtime_ms
                            }
                        });
                    let ttft_ms = time_obj
                        .and_then(|t| t.get("firstToken"))
                        .map(|v| parse_ts_value(v, 0))
                        .filter(|&ft| ft >= created_ms && (ft - created_ms) < 3_600_000)
                        .map(|ft| (ft - created_ms) as u64)
                        .unwrap_or(0);
                    let duration_ms = time_obj
                        .and_then(|t| t.get("completed"))
                        .map(|v| parse_ts_value(v, 0))
                        .filter(|&cmp| cmp >= created_ms && (cmp - created_ms) < 3_600_000)
                        .map(|cmp| (cmp - created_ms) as u64)
                        .unwrap_or(0);
                    if let Some(ev) = make_event_timed(
                        &source.source_key,
                        platform,
                        &sid,
                        &dir,
                        &model,
                        created_ms,
                        inp,
                        out,
                        cr,
                        cw,
                        reason,
                        "message",
                        duration_ms,
                        ttft_ms,
                    ) {
                        covered_sids.insert(sid);
                        events.push(ev);
                    }
                }
            }
        }
    }

    if !table_has_column(&conn, table, "tokens_input") {
        return events;
    }
    let has_model = table_has_column(&conn, table, "model");
    let model_expr = if has_model {
        "COALESCE(model, '')"
    } else {
        "''"
    };
    let sql = format!(
        "SELECT id, COALESCE(directory, ''), COALESCE(time_updated, time_created, 0), {model_expr},
                COALESCE(tokens_input, 0), COALESCE(tokens_output, 0),
                COALESCE(tokens_cache_read, 0), COALESCE(tokens_cache_write, 0),
                COALESCE(tokens_reasoning, 0)
         FROM {table}"
    );
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return events;
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, i64>(8)?,
        ))
    }) else {
        return events;
    };
    for (sid, dir, ts, model, inp, out, cr, cw, reason) in rows.flatten() {
        if covered_sids.contains(&sid) {
            continue;
        }
        let ts_ms = if ts > 0 { to_millis(ts) } else { source.mtime_ms };
        if let Some(ev) = make_event(
            &source.source_key,
            platform,
            &sid,
            &dir,
            &model,
            ts_ms,
            inp.max(0) as u64,
            out.max(0) as u64,
            cr.max(0) as u64,
            cw.max(0) as u64,
            reason.max(0) as u64,
            "session",
        ) {
            events.push(ev);
        }
    }
    events
}

fn parse_zcode_sqlite(source: &DiscoveredSource) -> Vec<UsageEvent> {
    let Ok(conn) = Connection::open_with_flags(
        &source.path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    ) else {
        return Vec::new();
    };
    let sql = "
        SELECT
            s.id,
            COALESCE(s.directory, ''),
            COALESCE(s.time_updated, s.time_created, 0),
            COALESCE(m.model_id, ''),
            COALESCE(SUM(m.input_tokens), 0),
            COALESCE(SUM(m.output_tokens), 0),
            COALESCE(SUM(m.cache_read_input_tokens), 0),
            COALESCE(SUM(m.cache_creation_input_tokens), 0),
            COALESCE(SUM(m.reasoning_tokens), 0)
        FROM model_usage m
        JOIN session s ON s.id = m.session_id
        GROUP BY s.id, COALESCE(m.model_id, '')
    ";
    let Ok(mut stmt) = conn.prepare(sql) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, i64>(8)?,
        ))
    }) else {
        return Vec::new();
    };
    let mut events = Vec::new();
    for (sid, dir, ts, model, inp, out, cr, cw, reason) in rows.flatten() {
        let ts_ms = if ts > 0 { to_millis(ts) } else { source.mtime_ms };
        if let Some(ev) = make_event(
            &source.source_key,
            "zcode",
            &sid,
            &dir,
            &model,
            ts_ms,
            inp.max(0) as u64,
            out.max(0) as u64,
            cr.max(0) as u64,
            cw.max(0) as u64,
            reason.max(0) as u64,
            "session",
        ) {
            events.push(ev);
        }
    }
    events
}

fn parse_dsh_proj_json(source: &DiscoveredSource) -> Vec<UsageEvent> {
    let Ok(text) = fs::read_to_string(&source.path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let Some(totals) = value.pointer("/record/rows/tokenUsage/val/totals") else {
        return Vec::new();
    };
    let dirname = source
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let sid = dirname.strip_prefix("session-").unwrap_or(dirname);
    let inp = totals
        .get("uncachedInputTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let out = totals
        .get("outputTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cr = totals
        .get("cacheReadTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cw = totals
        .get("cacheWriteTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let model = value
        .pointer("/record/rows/model/val")
        .and_then(extract_model_from_value)
        .unwrap_or_else(|| "dsh".to_string());
    let workspace = value
        .pointer("/record/rows/cwd/val")
        .and_then(Value::as_str)
        .unwrap_or("");
    make_event(
        &source.source_key,
        "dsh",
        sid,
        workspace,
        &model,
        source.mtime_ms,
        inp,
        out,
        cr,
        cw,
        0,
        "session",
    )
    .into_iter()
    .collect()
}

fn parse_grok_usage_json(source: &DiscoveredSource) -> Vec<UsageEvent> {
    let Ok(text) = fs::read_to_string(&source.path) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let Some(sess) = v.get("session").or(Some(&v)) else {
        return Vec::new();
    };
    let session_dir = source.path.parent();
    let sid = session_dir
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let ws_decoded = session_dir
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        .and_then(|s| urlencoding::decode(s).ok())
        .map(|c| c.into_owned())
        .unwrap_or_default();
    let mut model = String::from("grok-build");
    if let Some(sdir) = session_dir {
        if let Ok(sum_text) = fs::read_to_string(sdir.join("summary.json")) {
            if let Ok(sv) = serde_json::from_str::<Value>(&sum_text) {
                if let Some(m) = sv.get("current_model_id").and_then(Value::as_str) {
                    if !m.is_empty() {
                        model = m.to_string();
                    }
                }
            }
        }
    }
    let inp = sess.get("inputTokens").and_then(Value::as_u64).unwrap_or(0);
    let out = sess.get("outputTokens").and_then(Value::as_u64).unwrap_or(0);
    let cr = sess
        .get("cachedReadTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let reason = sess
        .get("reasoningTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    make_event(
        &source.source_key,
        "grok",
        &sid,
        &ws_decoded,
        &model,
        source.mtime_ms,
        inp,
        out,
        cr,
        0,
        reason,
        "session",
    )
    .into_iter()
    .collect()
}

fn sqlite_watermark(db_path: &Path) -> Option<(i64, i64)> {
    let (mut mtime, mut size) = file_watermark(db_path)?;
    let wal = PathBuf::from(format!("{}-wal", db_path.display()));
    if let Some((wm, ws)) = file_watermark(&wal) {
        if wm > mtime {
            mtime = wm;
        }
        size += ws;
    }
    Some((mtime, size))
}

fn push_walked_files(
    out: &mut Vec<DiscoveredSource>,
    root: &Path,
    ext: &str,
    platform: &'static str,
    kind: SourceKind,
    filter_fn: impl Fn(&Path) -> bool,
) {
    if !root.exists() {
        return;
    }
    for entry in walkdir::WalkDir::new(root).into_iter().flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some(ext) {
            continue;
        }
        if !filter_fn(path) {
            continue;
        }
        if let Some((mtime_ms, size_bytes)) = file_watermark(path) {
            out.push(DiscoveredSource {
                source_key: format!("{platform}:{}", path.display()),
                platform,
                path: path.to_path_buf(),
                mtime_ms,
                size_bytes,
                kind,
            });
        }
    }
}

fn find_opencode_db(dirs: &[PathBuf], platform: &str) -> Option<PathBuf> {
    let plat_db = format!("{platform}.db");
    for dir in dirs {
        for name in [&plat_db, "opencode.db", "db.sqlite", "state.db"] {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

pub(crate) fn discover_sources(home: &Path, env: &HashMap<String, String>) -> Vec<DiscoveredSource> {
    let mut out = Vec::new();

    // Claude Code
    let claude_dir = env
        .get("CLAUDE_CONFIG_DIR")
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));
    push_walked_files(
        &mut out,
        &claude_dir.join("projects"),
        "jsonl",
        "claude",
        SourceKind::ClaudeJsonl,
        |_| true,
    );

    // OpenClaude
    let openclaude_dir = env
        .get("OPENCLAUDE_CONFIG_DIR")
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".openclaude"));
    push_walked_files(
        &mut out,
        &openclaude_dir.join("projects"),
        "jsonl",
        "openclaude",
        SourceKind::OpenClaudeJsonl,
        |_| true,
    );

    // Codex
    for codex_home in [home.join(".codex"), home.join(".tcodex")] {
        for sub in ["sessions", "archived_sessions"] {
            push_walked_files(
                &mut out,
                &codex_home.join(sub),
                "jsonl",
                "codex",
                SourceKind::CodexJsonl,
                |_| true,
            );
        }
    }

    // Pi & omp
    let pi_dir = env
        .get("PI_CODING_AGENT_DIR")
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".pi").join("agent"));
    push_walked_files(
        &mut out,
        &pi_dir.join("sessions"),
        "jsonl",
        "pi",
        SourceKind::PiJsonl,
        |_| true,
    );

    let omp_dir = env
        .get("OMP_AGENT_DIR")
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".omp").join("agent"));
    push_walked_files(
        &mut out,
        &omp_dir.join("sessions"),
        "jsonl",
        "omp",
        SourceKind::OmpJsonl,
        |_| true,
    );

    // Antigravity
    let gemini_home = env
        .get("GEMINI_HOME")
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".gemini"));
    for flavor in ["antigravity", "antigravity-cli", "antigravity-ide"] {
        let start_len = out.len();
        push_walked_files(
            &mut out,
            &gemini_home.join(flavor).join("brain"),
            "jsonl",
            "antigravity",
            SourceKind::AntigravityJsonl,
            |p| p.file_name().and_then(|n| n.to_str()) == Some("transcript.jsonl"),
        );
        for src in &mut out[start_len..] {
            let sid = src
                .path
                .parent()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if let Some(db_path) = agy_conv_db_path(&src.path, sid) {
                if let Some((db_mtime, db_size)) = sqlite_watermark(&db_path) {
                    src.mtime_ms = src.mtime_ms.max(db_mtime);
                    src.size_bytes = src.size_bytes.saturating_add(db_size);
                }
            }
        }
    }

    // CodeBuddy & WorkBuddy
    push_walked_files(
        &mut out,
        &home.join(".codebuddy").join("projects"),
        "jsonl",
        "codebuddy",
        SourceKind::CodeBuddyJsonl,
        |_| true,
    );
    push_walked_files(
        &mut out,
        &home.join(".workbuddy").join("projects"),
        "jsonl",
        "workbuddy",
        SourceKind::WorkBuddyJsonl,
        |_| true,
    );

    // Qwen
    let qwen_home = env
        .get("QWEN_HOME")
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".qwen"));
    push_walked_files(
        &mut out,
        &qwen_home,
        "jsonl",
        "qwen",
        SourceKind::QwenJsonl,
        |_| true,
    );

    // DSH
    let dsh_home = env
        .get("DSH_HOME")
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".dsh"));
    push_walked_files(
        &mut out,
        &dsh_home.join("storages").join("session_projcache").join("sessions"),
        "json",
        "dsh",
        SourceKind::DshProjJson,
        |_| true,
    );

    // Grok
    let grok_home = env
        .get("GROK_HOME")
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".grok"));
    push_walked_files(
        &mut out,
        &grok_home.join("sessions"),
        "json",
        "grok",
        SourceKind::GrokUsageJson,
        |p| p.file_name().and_then(|n| n.to_str()) == Some("usage.json"),
    );

    // ZCode SQLite
    let zcode_db = home.join(".zcode").join("cli").join("db").join("db.sqlite");
    if let Some((mtime_ms, size_bytes)) = sqlite_watermark(&zcode_db) {
        out.push(DiscoveredSource {
            source_key: format!("zcode:{}", zcode_db.display()),
            platform: "zcode",
            path: zcode_db,
            mtime_ms,
            size_bytes,
            kind: SourceKind::ZCodeSqlite,
        });
    }

    // OpenCode & CodeWiz SQLite
    for (plat, kind) in [
        ("opencode", SourceKind::OpenCodeSqlite),
        ("codewiz", SourceKind::CodeWizSqlite),
    ] {
        let mut dirs = Vec::new();
        if let Some(xdg) = env.get("XDG_DATA_HOME").filter(|s| !s.trim().is_empty()) {
            dirs.push(PathBuf::from(xdg).join(plat));
        }
        dirs.push(home.join(".local").join("share").join(plat));
        dirs.push(home.join(format!(".{plat}")));
        dirs.push(home.join("Library").join("Application Support").join(plat));
        if let Some(db) = find_opencode_db(&dirs, plat) {
            if let Some((mtime_ms, size_bytes)) = sqlite_watermark(&db) {
                out.push(DiscoveredSource {
                    source_key: format!("{plat}:{}", db.display()),
                    platform: plat,
                    path: db,
                    mtime_ms,
                    size_bytes,
                    kind,
                });
            }
        }
    }

    out
}

fn parse_source(source: &DiscoveredSource) -> Vec<UsageEvent> {
    match source.kind {
        SourceKind::ClaudeJsonl => parse_claude_like(source, "claude"),
        SourceKind::OpenClaudeJsonl => parse_claude_like(source, "openclaude"),
        SourceKind::CodexJsonl => parse_codex_jsonl(source),
        SourceKind::PiJsonl => parse_pi_like(source, "pi"),
        SourceKind::OmpJsonl => parse_pi_like(source, "omp"),
        SourceKind::AntigravityJsonl => parse_antigravity_jsonl(source),
        SourceKind::CodeBuddyJsonl => parse_buddy_jsonl(source, "codebuddy"),
        SourceKind::WorkBuddyJsonl => parse_buddy_jsonl(source, "workbuddy"),
        SourceKind::QwenJsonl => parse_qwen_jsonl(source),
        SourceKind::OpenCodeSqlite => parse_opencode_sqlite(source, "opencode"),
        SourceKind::CodeWizSqlite => parse_opencode_sqlite(source, "codewiz"),
        SourceKind::ZCodeSqlite => parse_zcode_sqlite(source),
        SourceKind::DshProjJson => parse_dsh_proj_json(source),
        SourceKind::GrokUsageJson => parse_grok_usage_json(source),
    }
}

pub fn sync_usage_with_roots(
    app_data: &Path,
    home: &Path,
    env: &HashMap<String, String>,
    force: bool,
) -> Result<UsageSyncStatus, String> {
    let _guard = USAGE_SYNC_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let started = Instant::now();
    let mut conn = open_usage_db(&usage_db_path(app_data))?;
    let needs_full_resync =
        force || get_parser_version(&conn).as_deref() != Some(USAGE_PARSER_VERSION);
    let watermarks = if needs_full_resync {
        HashMap::new()
    } else {
        load_source_watermarks(&conn)
    };
    let sources = discover_sources(home, env);
    let live_keys: HashSet<String> = sources.iter().map(|s| s.source_key.clone()).collect();
    let now_ms = chrono::Utc::now().timestamp_millis();

    let mut updated_sources = 0usize;
    for source in &sources {
        let unchanged = matches!(
            watermarks.get(&source.source_key),
            Some(SourceWatermark { mtime_ms, size_bytes })
            if *mtime_ms == source.mtime_ms && *size_bytes == source.size_bytes
        );
        if unchanged {
            continue;
        }
        let events = parse_source(source);
        replace_source_events(
            &mut conn,
            &source.source_key,
            source.platform,
            source.mtime_ms,
            source.size_bytes,
            now_ms,
            &events,
        )?;
        updated_sources += 1;
    }

    let pruned_sources = prune_missing_sources(&mut conn, &live_keys)?;
    set_parser_version(&conn, USAGE_PARSER_VERSION)?;
    let total_events = count_events(&conn);
    Ok(UsageSyncStatus {
        scanned_sources: sources.len(),
        updated_sources,
        pruned_sources,
        total_events,
        duration_ms: started.elapsed().as_millis() as u64,
        synced_at_ms: now_ms,
    })
}

pub fn sync_usage(app_data: &Path, force: bool) -> Result<UsageSyncStatus, String> {
    let home = user_home().unwrap_or_else(|| PathBuf::from("/"));
    let env: HashMap<String, String> = std::env::vars().collect();
    sync_usage_with_roots(app_data, &home, &env, force)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDir(PathBuf);
    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_usage_sync_{}_{}",
                name,
                std::time::SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let _ = fs::create_dir_all(&path);
            Self(path)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn incremental_sync_dedups_claude_streams_and_prunes_deleted_files() {
        let dir = TestDir::new("inc");
        let app_data = dir.path().join("app");
        let home = dir.path().join("home");
        let proj = home.join(".claude/projects/myproj");
        fs::create_dir_all(&proj).unwrap();
        let sess_file = proj.join("11111111-1111-1111-1111-111111111111.jsonl");
        // Two streaming chunks sharing message.id "msg_1" -> only final chunk kept
        fs::write(
            &sess_file,
            concat!(
                r#"{"type":"assistant","cwd":"/repo/a","timestamp":"2026-11-20T01:00:00Z","message":{"id":"msg_1","model":"claude-sonnet-4-5","usage":{"input_tokens":100,"output_tokens":5,"cache_read_input_tokens":50,"cache_creation_input_tokens":10}}}"#,
                "\n",
                r#"{"type":"assistant","cwd":"/repo/a","timestamp":"2026-11-20T01:00:02Z","message":{"id":"msg_1","model":"claude-sonnet-4-5","usage":{"input_tokens":100,"output_tokens":40,"cache_read_input_tokens":50,"cache_creation_input_tokens":10}}}"#,
                "\n"
            ),
        )
        .unwrap();

        let env = HashMap::new();
        let first = sync_usage_with_roots(&app_data, &home, &env, false).unwrap();
        assert_eq!(first.scanned_sources, 1);
        assert_eq!(first.updated_sources, 1);
        assert_eq!(first.total_events, 1);

        // Second sync without file change skips parsing
        let second = sync_usage_with_roots(&app_data, &home, &env, false).unwrap();
        assert_eq!(second.updated_sources, 0);
        assert_eq!(second.total_events, 1);

        // Deleting file prunes source and events
        fs::remove_file(&sess_file).unwrap();
        let third = sync_usage_with_roots(&app_data, &home, &env, false).unwrap();
        assert_eq!(third.pruned_sources, 1);
        assert_eq!(third.total_events, 0);
    }

    #[test]
    fn opencode_json_model_strings_are_extracted_and_version_bump_triggers_resync() {
        let dir = TestDir::new("opencode_json_model");
        let app_data = dir.path().join("app");
        let home = dir.path().join("home");
        let xdg = dir.path().join("xdg");
        let oc_dir = xdg.join("opencode");
        fs::create_dir_all(&oc_dir).unwrap();
        let oc_db = oc_dir.join("opencode.db");
        let conn = Connection::open(&oc_db).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE session_v2 (
                id TEXT PRIMARY KEY,
                directory TEXT,
                time_created INTEGER,
                time_updated INTEGER,
                model TEXT,
                tokens_input INTEGER,
                tokens_output INTEGER,
                tokens_cache_read INTEGER,
                tokens_cache_write INTEGER,
                tokens_reasoning INTEGER
            );
            INSERT INTO session_v2 VALUES
              ('s1', '/repo/one', 1700000000000, 1700000001000, '{"id":"gemini-4-hydrogen","providerID":"google","variant":"high"}', 1000, 200, 0, 0, 0),
              ('s2', '/repo/two', 1700000002000, 1700000003000, '{"id":"gpt-6.2-sol","providerID":"boke"}', 500, 100, 50, 0, 0);
            "#,
        )
        .unwrap();
        drop(conn);

        let env = HashMap::from([(
            "XDG_DATA_HOME".to_string(),
            xdg.to_string_lossy().into_owned(),
        )]);
        let first = sync_usage_with_roots(&app_data, &home, &env, false).unwrap();
        assert_eq!(first.updated_sources, 1);
        assert_eq!(first.total_events, 2);

        let usage_conn = open_usage_db(&usage_db_path(&app_data)).unwrap();
        let mut models: Vec<(String, String)> = usage_conn
            .prepare("SELECT model_raw, model_norm FROM usage_events ORDER BY session_id ASC")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(
            models,
            vec![
                ("gemini-4-hydrogen".to_string(), "gemini-4-hydrogen".to_string()),
                ("gpt-6.2-sol".to_string(), "gpt-6.2-sol".to_string()),
            ]
        );
        assert!(models.iter().all(|(raw, norm)| !raw.contains('{') && !norm.contains('{')));

        // Running again with unchanged files and same parser version skips re-parsing
        let second = sync_usage_with_roots(&app_data, &home, &env, false).unwrap();
        assert_eq!(second.updated_sources, 0);

        // Downgrading parser_version forces automatic re-sync without force=true
        set_parser_version(&usage_conn, "1").unwrap();
        drop(usage_conn);

        let third = sync_usage_with_roots(&app_data, &home, &env, false).unwrap();
        assert_eq!(third.updated_sources, 1);
        models.clear();
        let usage_conn2 = open_usage_db(&usage_db_path(&app_data)).unwrap();
        assert_eq!(
            get_parser_version(&usage_conn2).as_deref(),
            Some(USAGE_PARSER_VERSION)
        );
    }

    #[test]
    fn opencode_session_message_extracts_timing_and_avoids_double_counting() {
        let dir = TestDir::new("opencode_msgs");
        let app_data = dir.path().join("app");
        let home = dir.path().join("home");
        let xdg = dir.path().join("xdg");
        let oc_dir = xdg.join("opencode");
        fs::create_dir_all(&oc_dir).unwrap();
        let oc_db = oc_dir.join("opencode.db");
        let conn = Connection::open(&oc_db).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE session_v2 (
                id TEXT PRIMARY KEY,
                directory TEXT,
                time_created INTEGER,
                time_updated INTEGER,
                model TEXT,
                tokens_input INTEGER,
                tokens_output INTEGER,
                tokens_cache_read INTEGER,
                tokens_cache_write INTEGER,
                tokens_reasoning INTEGER
            );
            CREATE TABLE session_message (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                type TEXT NOT NULL,
                seq INTEGER NOT NULL,
                time_created INTEGER NOT NULL,
                time_updated INTEGER NOT NULL,
                data TEXT NOT NULL
            );
            INSERT INTO session_v2 VALUES
              ('s1', '/repo/one', 1700000000000, 1700000010000, '{"id":"gpt-6.2-sol"}', 999, 999, 999, 0, 0),
              ('s_fallback', '/repo/two', 1700000020000, 1700000025000, '{"id":"gemini-4"}', 50, 10, 0, 0, 0);
            INSERT INTO session_message VALUES
              ('m1', 's1', 'assistant', 1, 1700000001000, 1700000006500, '{"time":{"created":1700000001000,"firstToken":1700000002200,"completed":1700000006500},"model":{"id":"gpt-6.2-sol"},"tokens":{"input":100,"output":25,"reasoning":10,"cache":{"read":400,"write":5}}}');
            "#,
        )
        .unwrap();
        drop(conn);

        let env = HashMap::from([(
            "XDG_DATA_HOME".to_string(),
            xdg.to_string_lossy().into_owned(),
        )]);
        let status = sync_usage_with_roots(&app_data, &home, &env, false).unwrap();
        assert_eq!(status.total_events, 2);

        let usage_conn = open_usage_db(&usage_db_path(&app_data)).unwrap();
        let rows: Vec<(String, i64, i64, i64, String)> = usage_conn
            .prepare("SELECT session_id, input_tokens, duration_ms, ttft_ms, granularity FROM usage_events ORDER BY session_id ASC")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(
            rows,
            vec![
                ("s1".to_string(), 100, 5500, 1200, "message".to_string()),
                ("s_fallback".to_string(), 50, 0, 0, "session".to_string()),
            ]
        );
    }

    #[test]
    fn derives_claude_and_antigravity_durations_from_session_timestamps() {
        let dir = TestDir::new("derived_timings");
        let app_data = dir.path().join("app");
        let home = dir.path().join("home");

        let claude_proj = home.join(".claude/projects/repo");
        fs::create_dir_all(&claude_proj).unwrap();
        fs::write(
            claude_proj.join("33333333-3333-3333-3333-333333333333.jsonl"),
            concat!(
                r#"{"type":"user","cwd":"/repo","timestamp":"2026-11-20T01:00:00Z","message":{"role":"user","content":"hi"}}"#,
                "\n",
                r#"{"type":"assistant","cwd":"/repo","timestamp":"2026-11-20T01:00:01.500Z","message":{"id":"m1","model":"claude-sonnet-4-5","usage":{"input_tokens":100,"output_tokens":5}}}"#,
                "\n",
                r#"{"type":"assistant","cwd":"/repo","timestamp":"2026-11-20T01:00:05.000Z","message":{"id":"m1","model":"claude-sonnet-4-5","usage":{"input_tokens":100,"output_tokens":40}}}"#,
                "\n"
            ),
        )
        .unwrap();

        let ag_logs = home.join(".gemini/antigravity/brain/44444444-4444-4444-4444-444444444444/.system_generated/logs");
        fs::create_dir_all(&ag_logs).unwrap();
        fs::write(
            ag_logs.join("transcript.jsonl"),
            concat!(
                r#"{"type":"USER_INPUT","created_at":"2026-11-20T02:00:00Z","content":"Active Workspaces:\n- /repo\n"}"#,
                "\n",
                r#"{"type":"PLANNER_RESPONSE","created_at":"2026-11-20T02:00:00Z","model":"gemini-2.5-pro","input_tokens":200,"output_tokens":30,"cache_read_tokens":50}"#,
                "\n",
                r#"{"type":"GENERIC","created_at":"2026-11-20T02:00:07Z","content":"tool"}"#,
                "\n",
                r#"{"type":"PLANNER_RESPONSE","created_at":"2026-11-20T02:00:07Z","model":"gemini-2.5-pro","input_tokens":300,"output_tokens":60,"cache_read_tokens":100}"#,
                "\n",
                r#"{"type":"USER_INPUT","created_at":"2026-11-20T02:05:00Z","content":"next prompt"}"#,
                "\n"
            ),
        )
        .unwrap();

        let env = HashMap::new();
        let status = sync_usage_with_roots(&app_data, &home, &env, false).unwrap();
        assert_eq!(status.total_events, 3);

        let conn = open_usage_db(&usage_db_path(&app_data)).unwrap();
        let rows: Vec<(String, i64, i64, i64)> = conn
            .prepare("SELECT platform, input_tokens, duration_ms, ttft_ms FROM usage_events ORDER BY platform ASC, ts_ms ASC, input_tokens ASC")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(
            rows,
            vec![
                ("antigravity".to_string(), 200, 7000, 0),
                ("antigravity".to_string(), 300, 0, 0),
                ("claude".to_string(), 100, 5000, 1500),
            ]
        );
    }

    fn pb_varint(buf: &mut Vec<u8>, mut v: u64) {
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                buf.push(byte);
                break;
            } else {
                buf.push(byte | 0x80);
            }
        }
    }

    fn pb_len_field(buf: &mut Vec<u8>, fnum: u32, payload: &[u8]) {
        pb_varint(buf, u64::from(fnum) << 3 | 2);
        pb_varint(buf, payload.len() as u64);
        buf.extend_from_slice(payload);
    }

    fn pb_varint_field(buf: &mut Vec<u8>, fnum: u32, v: u64) {
        pb_varint(buf, u64::from(fnum) << 3);
        pb_varint(buf, v);
    }

    fn pb_ts_msg(secs: u64, nanos: u64) -> Vec<u8> {
        let mut out = Vec::new();
        pb_varint_field(&mut out, 1, secs);
        pb_varint_field(&mut out, 2, nanos);
        out
    }

    #[test]
    fn antigravity_conversation_db_extracts_exact_ttft_duration_and_reasoning() {
        let dir = TestDir::new("agy_conv_db");
        let app_data = dir.path().join("app");
        let home = dir.path().join("home");
        let agy_root = home.join(".gemini/antigravity");
        let sid = "55555555-5555-5555-5555-555555555555";
        let logs = agy_root.join(format!("brain/{sid}/.system_generated/logs"));
        let conv_dir = agy_root.join("conversations");
        fs::create_dir_all(&logs).unwrap();
        fs::create_dir_all(&conv_dir).unwrap();

        fs::write(
            logs.join("transcript.jsonl"),
            concat!(
                r#"{"step_index":0,"type":"USER_INPUT","created_at":"2026-11-20T03:00:00Z","content":"Active Workspaces:\n- /repo/agy\n"}"#,
                "\n",
                r#"{"step_index":1,"type":"PLANNER_RESPONSE","created_at":"2026-11-20T03:00:00Z","model":"gemini-4-hydrogen-high","input_tokens":1200,"output_tokens":300,"cache_read_tokens":4000}"#,
                "\n"
            ),
        )
        .unwrap();

        let mut meta = Vec::new();
        pb_len_field(&mut meta, 1, &pb_ts_msg(1795143600, 100_000_000));
        pb_len_field(&mut meta, 6, &pb_ts_msg(1795143602, 600_000_000));
        pb_len_field(&mut meta, 7, &pb_ts_msg(1795143608, 900_000_000));
        let mut usage = Vec::new();
        pb_varint_field(&mut usage, 9, 45);
        pb_len_field(&mut meta, 9, &usage);

        let db = Connection::open(conv_dir.join(format!("{sid}.db"))).unwrap();
        db.execute_batch(
            "CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER, metadata BLOB);",
        )
        .unwrap();
        db.execute(
            "INSERT INTO steps (idx, step_type, metadata) VALUES (1, 15, ?1)",
            rusqlite::params![meta],
        )
        .unwrap();
        drop(db);

        let status = sync_usage_with_roots(&app_data, &home, &HashMap::new(), false).unwrap();
        assert_eq!(status.total_events, 1);

        let usage_conn = open_usage_db(&usage_db_path(&app_data)).unwrap();
        let row: (i64, i64, i64, i64, i64) = usage_conn
            .query_row(
                "SELECT ts_ms, ttft_ms, duration_ms, reasoning_tokens, input_tokens FROM usage_events WHERE session_id = ?1",
                rusqlite::params![sid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(row, (1795143600100, 2500, 8800, 45, 1200));
    }

    #[test]
    fn codex_subtracts_cached_tokens_from_input_tokens() {
        let dir = TestDir::new("codex_cached");
        let app_data = dir.path().join("app");
        let home = dir.path().join("home");
        let sess_dir = home.join(".codex/sessions/2026/11/20");
        fs::create_dir_all(&sess_dir).unwrap();
        let sid = "66666666-6666-6666-6666-666666666666";
        fs::write(
            sess_dir.join(format!("rollout-{sid}.jsonl")),
            concat!(
                r#"{"type":"session_meta","timestamp":"2026-11-20T04:00:00Z","payload":{"cwd":"/repo/codex"}}"#,
                "\n",
                r#"{"type":"turn_context","timestamp":"2026-11-20T04:00:00Z","payload":{"model":"gpt-5.3-codex"}}"#,
                "\n",
                r#"{"type":"response_item","timestamp":"2026-11-20T04:00:01Z","payload":{"type":"reasoning","role":"assistant"}}"#,
                "\n",
                r#"{"type":"event_msg","timestamp":"2026-11-20T04:00:03Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1000,"output_tokens":150,"cached_input_tokens":800,"reasoning_output_tokens":40}}}}"#,
                "\n"
            ),
        )
        .unwrap();

        let status = sync_usage_with_roots(&app_data, &home, &HashMap::new(), false).unwrap();
        assert_eq!(status.total_events, 1);

        let usage_conn = open_usage_db(&usage_db_path(&app_data)).unwrap();
        let row: (i64, i64, i64, i64, i64, i64) = usage_conn
            .query_row(
                "SELECT input_tokens, output_tokens, cache_read_tokens, reasoning_tokens, ttft_ms, duration_ms FROM usage_events WHERE session_id = ?1",
                rusqlite::params![sid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .unwrap();
        assert_eq!(row, (200, 150, 800, 40, 1000, 3000));
    }
}
