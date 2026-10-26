//! 明细查询与 Key 健康度。

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::db;

use super::{COST, UsageGroup, since_secs};

/// 明细行(返回给前端;不含任何明文 Key)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEntry {
    pub id: i64,
    pub provider_id: String,
    pub model_id: String,
    pub slot_id: i64,
    pub slot_label: Option<String>,
    pub key_masked: Option<String>,
    pub status: String,
    pub error_kind: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    /// 缓存命中输入 token(V17;NULL = 厂商未返回)
    pub cached_tokens: Option<i64>,
    pub first_token_ms: Option<i64>,
    pub total_ms: Option<i64>,
    pub created_at: i64,
    pub agent_id: Option<String>,
    pub agent_name: Option<String>,
    /// 本次请求有效成本(纳元);None = 未计价
    pub cost_nano: Option<i64>,
    /// 落库当时的计价快照(单价 / 汇率 / 卡 id),供审计复算
    pub pricing_snapshot: Option<String>,
    pub cost_source: Option<String>,
}

/// 明细查询:时间窗口 + 维度下钻全部下沉 SQL,避免"图表近 7 天、明细全历史"的口径错位。
///
/// dimension: None = 不筛选;Some("model"|"provider"|"key"|"agent") + key 值。
pub fn entries_on(
    conn: &Connection,
    limit: u32,
    days: u32,
    dimension: Option<&str>,
    key: Option<&str>,
) -> rusqlite::Result<Vec<UsageEntry>> {
    use rusqlite::types::Value;
    let since = since_secs(days);
    let mut sql = format!(
        "SELECT id, provider_id, model_id, slot_id, slot_label, key_masked, status, error_kind,
                input_tokens, output_tokens, total_tokens, reasoning_tokens, cached_tokens,
                first_token_ms, total_ms, created_at,
                agent_id, agent_name, {COST}, pricing_snapshot, cost_source
         FROM usage_record WHERE created_at >= ?"
    );
    let mut args: Vec<Value> = vec![Value::Integer(since)];

    if let (Some(dim), Some(k)) = (dimension, key)
        && !k.is_empty()
    {
        match dim {
            "provider" => {
                sql.push_str(" AND provider_id = ?");
                args.push(Value::Text(k.to_string()));
            }
            "key" => match k.parse::<i64>() {
                Ok(slot) => {
                    sql.push_str(" AND slot_id = ?");
                    args.push(Value::Integer(slot));
                }
                // 槽位键非数字:不可能命中,直接空结果(避免退化为全表)
                Err(_) => return Ok(Vec::new()),
            },
            "agent" => {
                sql.push_str(" AND COALESCE(NULLIF(agent_id,''),'none') = ?");
                args.push(Value::Text(k.to_string()));
            }
            _ => {
                sql.push_str(" AND model_id = ?");
                args.push(Value::Text(k.to_string()));
            }
        }
    }
    sql.push_str(" ORDER BY id DESC LIMIT ?");
    args.push(Value::Integer(limit.clamp(1, 500) as i64));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(args.iter()), |r| {
            Ok(UsageEntry {
                id: r.get(0)?,
                provider_id: r.get(1)?,
                model_id: r.get(2)?,
                slot_id: r.get(3)?,
                slot_label: r.get(4)?,
                key_masked: r.get(5)?,
                status: r.get(6)?,
                error_kind: r.get(7)?,
                input_tokens: r.get(8)?,
                output_tokens: r.get(9)?,
                total_tokens: r.get(10)?,
                reasoning_tokens: r.get(11)?,
                cached_tokens: r.get(12)?,
                first_token_ms: r.get(13)?,
                total_ms: r.get(14)?,
                created_at: r.get(15)?,
                agent_id: r.get(16)?,
                agent_name: r.get(17)?,
                cost_nano: r.get(18)?,
                pricing_snapshot: r.get(19)?,
                cost_source: r.get(20)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 最近明细(全局连接)。
pub fn entries(
    limit: u32,
    days: u32,
    dimension: Option<&str>,
    key: Option<&str>,
) -> Result<Vec<UsageEntry>, String> {
    let conn = db::conn().lock().unwrap_or_else(|e| e.into_inner());
    entries_on(&conn, limit, days, dimension, key).map_err(|e| e.to_string())
}

/// Key 健康度:某 provider 各槽位成功 / 限流 / 失败计数。
pub fn key_health_on(
    conn: &Connection,
    provider_id: &str,
    days: u32,
) -> rusqlite::Result<Vec<UsageGroup>> {
    let since = since_secs(days);
    let sql = format!(
        "SELECT slot_id, COALESCE(NULLIF(slot_label,''), '槽位 ' || slot_id),
                COUNT(*),
                COALESCE(SUM(total_tokens), 0),
                SUM(CASE WHEN status <> 'success' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'rate_limited' THEN 1 ELSE 0 END),
                MAX(created_at),
                COALESCE(SUM({COST}), 0),
                SUM(CASE WHEN status = 'success' AND {COST} IS NULL THEN 1 ELSE 0 END)
         FROM usage_record WHERE provider_id = ?1 AND created_at >= ?2
         GROUP BY slot_id ORDER BY slot_id ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![provider_id, since], |r| {
            let slot: i64 = r.get(0)?;
            Ok(UsageGroup {
                key: slot.to_string(),
                name: r.get(1)?,
                slot_id: Some(slot),
                cooling: false,
                requests: r.get(2)?,
                tokens: r.get(3)?,
                failed: r.get::<_, Option<i64>>(4)?.unwrap_or(0),
                rate_limited: r.get::<_, Option<i64>>(5)?.unwrap_or(0),
                avg_first_token_ms: None,
                cost_nano: r.get(7)?,
                uncosted_requests: r.get::<_, Option<i64>>(8)?.unwrap_or(0),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Key 健康度(全局连接)。
pub fn key_health(provider_id: &str, days: u32) -> Result<Vec<UsageGroup>, String> {
    let conn = db::conn().lock().unwrap_or_else(|e| e.into_inner());
    key_health_on(&conn, provider_id, days).map_err(|e| e.to_string())
}
