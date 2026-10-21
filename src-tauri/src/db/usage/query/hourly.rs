//! 热力图(星期 × 小时)聚合。

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::db;

use super::since_secs;

/// 热力图的一格:星期(0=周日) × 小时。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageHour {
    /// 星期(0=周日 … 6=周六)
    pub dow: i64,
    /// 小时(0-23)
    pub hour: i64,
    pub requests: i64,
    pub tokens: i64,
}

/// 热力图数据:星期 × 小时的请求数 / Token 数(按本地时区分桶)。
pub fn hourly_on(conn: &Connection, days: u32) -> rusqlite::Result<Vec<UsageHour>> {
    let since = since_secs(days);
    let mut stmt = conn.prepare(
        "SELECT created_at, COALESCE(total_tokens, 0)
         FROM usage_record WHERE created_at >= ?1",
    )?;
    let rows = stmt
        .query_map(params![since], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    use chrono::{Datelike, TimeZone, Timelike};
    let mut buckets: std::collections::HashMap<(i64, i64), (i64, i64)> =
        std::collections::HashMap::new();
    for (secs, tokens) in rows {
        let Some(dt) = chrono::Local.timestamp_opt(secs, 0).single() else {
            continue;
        };
        let key = (dt.weekday().num_days_from_sunday() as i64, dt.hour() as i64);
        let e = buckets.entry(key).or_insert((0, 0));
        e.0 += 1;
        e.1 += tokens;
    }
    let mut out: Vec<UsageHour> = buckets
        .into_iter()
        .map(|((dow, hour), (requests, tokens))| UsageHour {
            dow,
            hour,
            requests,
            tokens,
        })
        .collect();
    out.sort_by_key(|h| (h.dow, h.hour));
    Ok(out)
}

/// 热力图数据(全局连接)。
pub fn hourly(days: u32) -> Result<Vec<UsageHour>, String> {
    let conn = db::conn().lock().unwrap_or_else(|e| e.into_inner());
    hourly_on(&conn, days).map_err(|e| e.to_string())
}
