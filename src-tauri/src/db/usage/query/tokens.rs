//! 复算用 token 构成聚合。

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::db;

use super::{COST, since_secs};

/// 复算用的 token 构成聚合(按 provider × model,仅成功请求)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageTokenMix {
    pub provider_id: String,
    pub model_id: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    pub stored_nano: i64,
    pub unpriced_requests: i64,
}

/// 按当前定价试算的输入:历史 token 合计 + 落库成本合计(只读)。
pub fn token_mix_on(conn: &Connection, days: u32) -> rusqlite::Result<Vec<UsageTokenMix>> {
    let since = since_secs(days);
    let sql = format!(
        "SELECT provider_id, model_id, COUNT(*),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cached_tokens), 0),
                COALESCE(SUM({COST}), 0),
                SUM(CASE WHEN {COST} IS NULL THEN 1 ELSE 0 END)
         FROM usage_record WHERE created_at >= ?1 AND status = 'success'
         GROUP BY provider_id, model_id
         ORDER BY provider_id, model_id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![since], |r| {
            Ok(UsageTokenMix {
                provider_id: r.get(0)?,
                model_id: r.get(1)?,
                requests: r.get(2)?,
                input_tokens: r.get(3)?,
                output_tokens: r.get(4)?,
                cached_tokens: r.get(5)?,
                stored_nano: r.get(6)?,
                unpriced_requests: r.get::<_, Option<i64>>(7)?.unwrap_or(0),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 复算输入(全局连接)。
pub fn token_mix(days: u32) -> Result<Vec<UsageTokenMix>, String> {
    let conn = db::conn().lock().unwrap_or_else(|e| e.into_inner());
    token_mix_on(&conn, days).map_err(|e| e.to_string())
}
