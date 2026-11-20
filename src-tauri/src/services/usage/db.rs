use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OpenFlags};

use super::{CurrencyPref, PricingOverrideRow, PricingRate, UsageEvent};

pub const USAGE_PARSER_VERSION: &str = "6";

pub fn usage_db_path(app_data: &Path) -> PathBuf {
    app_data.join("usage.db")
}

fn ensure_usage_events_columns(conn: &Connection) -> Result<(), String> {
    let mut cols = HashSet::new();
    {
        let mut stmt = conn
            .prepare("PRAGMA table_info(usage_events)")
            .map_err(|e| format!("Inspect usage_events columns failed: {e}"))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| format!("Read usage_events columns failed: {e}"))?;
        for name in rows.flatten() {
            cols.insert(name);
        }
    }
    if !cols.contains("duration_ms") {
        conn.execute(
            "ALTER TABLE usage_events ADD COLUMN duration_ms INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .map_err(|e| format!("Add duration_ms column failed: {e}"))?;
    }
    if !cols.contains("ttft_ms") {
        conn.execute(
            "ALTER TABLE usage_events ADD COLUMN ttft_ms INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .map_err(|e| format!("Add ttft_ms column failed: {e}"))?;
    }
    Ok(())
}

pub fn open_usage_db(db_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Create usage db dir failed: {e}"))?;
    }
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )
    .map_err(|e| format!("Open usage.db failed: {e}"))?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;

        CREATE TABLE IF NOT EXISTS usage_sources (
            source_key TEXT PRIMARY KEY,
            platform TEXT NOT NULL,
            mtime_ms INTEGER NOT NULL,
            size_bytes INTEGER NOT NULL,
            last_synced_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS usage_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_key TEXT NOT NULL,
            platform TEXT NOT NULL,
            session_id TEXT NOT NULL,
            workspace TEXT NOT NULL,
            model_raw TEXT NOT NULL,
            model_norm TEXT NOT NULL,
            ts_ms INTEGER NOT NULL,
            day_key TEXT NOT NULL,
            hour_key TEXT NOT NULL,
            input_tokens INTEGER NOT NULL,
            output_tokens INTEGER NOT NULL,
            cache_read_tokens INTEGER NOT NULL,
            cache_write_tokens INTEGER NOT NULL,
            reasoning_tokens INTEGER NOT NULL,
            granularity TEXT NOT NULL,
            duration_ms INTEGER NOT NULL DEFAULT 0,
            ttft_ms INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_usage_events_day_plat ON usage_events(day_key, platform);
        CREATE INDEX IF NOT EXISTS idx_usage_events_session ON usage_events(session_id);
        CREATE INDEX IF NOT EXISTS idx_usage_events_source ON usage_events(source_key);
        CREATE INDEX IF NOT EXISTS idx_usage_events_ts ON usage_events(ts_ms DESC);

        CREATE TABLE IF NOT EXISTS usage_pricing_overrides (
            model_norm TEXT PRIMARY KEY,
            input REAL NOT NULL,
            output REAL NOT NULL,
            cache_read REAL NOT NULL,
            cache_write REAL NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS usage_prefs (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        ",
    )
    .map_err(|e| format!("Init usage.db schema failed: {e}"))?;
    ensure_usage_events_columns(&conn)?;
    Ok(conn)
}

#[derive(Debug, Clone, Copy)]
pub struct SourceWatermark {
    pub mtime_ms: i64,
    pub size_bytes: i64,
}

pub fn load_source_watermarks(conn: &Connection) -> HashMap<String, SourceWatermark> {
    let mut map = HashMap::new();
    let Ok(mut stmt) = conn.prepare("SELECT source_key, mtime_ms, size_bytes FROM usage_sources")
    else {
        return map;
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            SourceWatermark {
                mtime_ms: row.get(1)?,
                size_bytes: row.get(2)?,
            },
        ))
    }) else {
        return map;
    };
    for (k, w) in rows.flatten() {
        map.insert(k, w);
    }
    map
}

pub fn replace_source_events(
    conn: &mut Connection,
    source_key: &str,
    platform: &str,
    mtime_ms: i64,
    size_bytes: i64,
    synced_at_ms: i64,
    events: &[UsageEvent],
) -> Result<(), String> {
    let tx = conn
        .transaction()
        .map_err(|e| format!("Begin usage tx failed: {e}"))?;
    tx.execute(
        "DELETE FROM usage_events WHERE source_key = ?1",
        params![source_key],
    )
    .map_err(|e| format!("Delete stale events failed: {e}"))?;

    {
        let mut insert = tx
            .prepare(
                "INSERT INTO usage_events (
                    source_key, platform, session_id, workspace, model_raw, model_norm,
                    ts_ms, day_key, hour_key, input_tokens, output_tokens,
                    cache_read_tokens, cache_write_tokens, reasoning_tokens, granularity,
                    duration_ms, ttft_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            )
            .map_err(|e| format!("Prepare insert usage_event failed: {e}"))?;
        for ev in events {
            insert
                .execute(params![
                    source_key,
                    ev.platform,
                    ev.session_id,
                    ev.workspace,
                    ev.model_raw,
                    ev.model_norm,
                    ev.ts_ms,
                    ev.day_key,
                    ev.hour_key,
                    ev.input_tokens as i64,
                    ev.output_tokens as i64,
                    ev.cache_read_tokens as i64,
                    ev.cache_write_tokens as i64,
                    ev.reasoning_tokens as i64,
                    ev.granularity,
                    ev.duration_ms as i64,
                    ev.ttft_ms as i64,
                ])
                .map_err(|e| format!("Insert usage_event failed: {e}"))?;
        }
    }

    tx.execute(
        "INSERT INTO usage_sources (source_key, platform, mtime_ms, size_bytes, last_synced_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(source_key) DO UPDATE SET
            platform = excluded.platform,
            mtime_ms = excluded.mtime_ms,
            size_bytes = excluded.size_bytes,
            last_synced_at = excluded.last_synced_at",
        params![source_key, platform, mtime_ms, size_bytes, synced_at_ms],
    )
    .map_err(|e| format!("Upsert usage_sources failed: {e}"))?;

    tx.commit()
        .map_err(|e| format!("Commit usage tx failed: {e}"))?;
    Ok(())
}

pub fn prune_missing_sources(
    conn: &mut Connection,
    live_keys: &HashSet<String>,
) -> Result<usize, String> {
    let existing: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT source_key FROM usage_sources")
            .map_err(|e| format!("Read sources failed: {e}"))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("Query sources failed: {e}"))?;
        rows.flatten().collect()
    };
    let stale: Vec<String> = existing
        .into_iter()
        .filter(|k| !live_keys.contains(k))
        .collect();
    if stale.is_empty() {
        return Ok(0);
    }
    let tx = conn
        .transaction()
        .map_err(|e| format!("Begin prune tx failed: {e}"))?;
    for key in &stale {
        tx.execute("DELETE FROM usage_events WHERE source_key = ?1", params![key])
            .map_err(|e| format!("Prune events failed: {e}"))?;
        tx.execute(
            "DELETE FROM usage_sources WHERE source_key = ?1",
            params![key],
        )
        .map_err(|e| format!("Prune source failed: {e}"))?;
    }
    tx.commit().map_err(|e| format!("Commit prune failed: {e}"))?;
    Ok(stale.len())
}

pub fn count_events(conn: &Connection) -> usize {
    conn.query_row("SELECT COUNT(*) FROM usage_events", [], |row| {
        row.get::<_, i64>(0)
    })
    .unwrap_or(0)
    .max(0) as usize
}

pub fn list_pricing_overrides(conn: &Connection) -> Vec<PricingOverrideRow> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT model_norm, input, output, cache_read, cache_write, updated_at
         FROM usage_pricing_overrides ORDER BY model_norm ASC",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok(PricingOverrideRow {
            model_norm: row.get(0)?,
            input: row.get(1)?,
            output: row.get(2)?,
            cache_read: row.get(3)?,
            cache_write: row.get(4)?,
            updated_at: row.get(5)?,
        })
    }) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

pub fn load_pricing_override_map(conn: &Connection) -> HashMap<String, PricingRate> {
    list_pricing_overrides(conn)
        .into_iter()
        .map(|r| {
            (
                r.model_norm,
                PricingRate {
                    input: r.input,
                    output: r.output,
                    cache_read: r.cache_read,
                    cache_write: r.cache_write,
                },
            )
        })
        .collect()
}

pub fn save_pricing_override(
    conn: &Connection,
    model_norm: &str,
    rate: PricingRate,
    now_ms: i64,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO usage_pricing_overrides (model_norm, input, output, cache_read, cache_write, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(model_norm) DO UPDATE SET
            input = excluded.input,
            output = excluded.output,
            cache_read = excluded.cache_read,
            cache_write = excluded.cache_write,
            updated_at = excluded.updated_at",
        params![
            model_norm,
            rate.input,
            rate.output,
            rate.cache_read,
            rate.cache_write,
            now_ms
        ],
    )
    .map_err(|e| format!("Save pricing override failed: {e}"))?;
    Ok(())
}

pub fn delete_pricing_override(conn: &Connection, model_norm: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM usage_pricing_overrides WHERE model_norm = ?1",
        params![model_norm],
    )
    .map_err(|e| format!("Delete pricing override failed: {e}"))?;
    Ok(())
}

pub fn default_exchange_rate(currency: &str) -> f64 {
    match currency.to_ascii_uppercase().as_str() {
        "CNY" => 7.25,
        "EUR" => 0.92,
        "JPY" => 152.0,
        _ => 1.0,
    }
}

pub fn get_currency_pref(conn: &Connection) -> CurrencyPref {
    let currency = conn
        .query_row(
            "SELECT value FROM usage_prefs WHERE key = 'currency'",
            [],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_else(|_| "USD".to_string());
    let rate_key = format!("rate_{}", currency.to_ascii_uppercase());
    let rate = conn
        .query_row(
            "SELECT value FROM usage_prefs WHERE key = ?1",
            params![rate_key],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|r| r.is_finite() && *r > 0.0)
        .unwrap_or_else(|| default_exchange_rate(&currency));
    CurrencyPref { currency, rate }
}

pub fn set_currency_pref(conn: &Connection, currency: &str, rate: Option<f64>) -> Result<CurrencyPref, String> {
    let norm = currency.trim().to_ascii_uppercase();
    let code = if norm.is_empty() { "USD".to_string() } else { norm };
    conn.execute(
        "INSERT INTO usage_prefs (key, value) VALUES ('currency', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![code],
    )
    .map_err(|e| format!("Save currency failed: {e}"))?;
    if let Some(r) = rate.filter(|v| v.is_finite() && *v > 0.0) {
        let rate_key = format!("rate_{code}");
        conn.execute(
            "INSERT INTO usage_prefs (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![rate_key, r.to_string()],
        )
        .map_err(|e| format!("Save exchange rate failed: {e}"))?;
    }
    Ok(get_currency_pref(conn))
}

pub fn get_parser_version(conn: &Connection) -> Option<String> {
    conn.query_row(
        "SELECT value FROM usage_prefs WHERE key = 'parser_version'",
        [],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

pub fn set_parser_version(conn: &Connection, version: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO usage_prefs (key, value) VALUES ('parser_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![version],
    )
    .map_err(|e| format!("Save parser version failed: {e}"))?;
    Ok(())
}
