//! usage_record 表的写入与聚合(API 流转 · 用量统计)。
//!
//! 对应 DEVPLAN-api-rotation-usage.md §4.1/§4.3。
//!
//! 设计:
//!   - append-only:每次实际发出的请求落一行,成功 / 失败 / 限流都记;
//!   - slot_id / slot_label / key_masked 为 Key 归因主线,统计按 provider/model/slot 分组;
//!   - 只存 Key 掩码,绝不明文。
//!
//! 核心逻辑以 `*_on(&Connection, ..)` 实现便于内存库单测,全局函数仅负责加锁委托。

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::db;

/// 一次请求的用量快照(落库入参)。
#[derive(Debug, Clone, Default)]
pub struct UsageRecord {
    pub request_id: String,
    pub session_id: Option<String>,
    pub provider_id: String,
    pub provider_name: Option<String>,
    pub model_id: String,
    pub slot_id: u32,
    pub slot_label: Option<String>,
    pub key_masked: Option<String>,
    /// success | failed | rate_limited
    pub status: String,
    pub error_kind: Option<String>,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    /// 思考 token(V14;思考开启的请求才有,旧记录为 None)
    pub reasoning_tokens: Option<u32>,
    pub first_token_ms: Option<i64>,
    pub total_ms: Option<i64>,
    /// 会话绑定的 Agent 模板 id(按助手归因;未绑定为 None)
    pub agent_id: Option<String>,
    /// Agent 展示名(便于直接分组展示)
    pub agent_name: Option<String>,
    /// 成本(微元,1e-6 元);None = 无定价信息
    pub cost_micro: Option<i64>,
    /// 计价快照 JSON(落库当时的单价,保证历史成本可复算)
    pub pricing_snapshot: Option<String>,
    /// 计价来源:model_card | custom | none
    pub cost_source: Option<String>,
}

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
    pub first_token_ms: Option<i64>,
    pub total_ms: Option<i64>,
    pub created_at: i64,
    pub agent_id: Option<String>,
    pub agent_name: Option<String>,
    pub cost_micro: Option<i64>,
    pub cost_source: Option<String>,
}

/// 聚合总览。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageTotals {
    pub requests: i64,
    pub tokens: i64,
    pub failed: i64,
    pub rate_limited: i64,
    pub avg_first_token_ms: Option<i64>,
    /// 总成本(微元,1e-6 元)
    pub cost_micro: i64,
}

/// 一个分组(按模型 / provider / Key / 助手)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageGroup {
    /// 分组键(model_id / provider_id / slot 号 / agent_id)
    pub key: String,
    /// 供展示的名字(模型名 / 服务商名 / Key 备注 / 助手名)
    pub name: String,
    /// 仅 Key 维度携带:槽位号
    pub slot_id: Option<i64>,
    /// 仅 Key 维度携带:是否为当前冷却中的槽位
    pub cooling: bool,
    pub requests: i64,
    pub tokens: i64,
    pub failed: i64,
    pub rate_limited: i64,
    pub avg_first_token_ms: Option<i64>,
    /// 该分组成本(微元,1e-6 元)
    pub cost_micro: i64,
}

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

/// 归因汇总(总览 + 分组)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageSummary {
    pub totals: UsageTotals,
    pub groups: Vec<UsageGroup>,
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// 写入一条用量记录(全局连接)。
pub fn insert(rec: &UsageRecord) -> Result<(), String> {
    let conn = db::conn().lock().unwrap_or_else(|e| e.into_inner());
    insert_on(&conn, rec).map_err(|e| e.to_string())
}

/// 写入一条用量记录(给定连接,便于单测)。
pub fn insert_on(conn: &Connection, rec: &UsageRecord) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO usage_record(
            request_id, session_id, provider_id, provider_name, model_id,
            slot_id, slot_label, key_masked, status, error_kind,
            input_tokens, output_tokens, total_tokens, reasoning_tokens,
            first_token_ms, total_ms, created_at,
            agent_id, agent_name, cost_micro, pricing_snapshot, cost_source
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)",
        params![
            rec.request_id,
            rec.session_id,
            rec.provider_id,
            rec.provider_name,
            rec.model_id,
            rec.slot_id as i64,
            rec.slot_label,
            rec.key_masked,
            rec.status,
            rec.error_kind,
            rec.input_tokens.map(|v| v as i64),
            rec.output_tokens.map(|v| v as i64),
            rec.total_tokens.map(|v| v as i64),
            rec.reasoning_tokens.map(|v| v as i64),
            rec.first_token_ms,
            rec.total_ms,
            now_secs(),
            rec.agent_id,
            rec.agent_name,
            rec.cost_micro,
            rec.pricing_snapshot,
            rec.cost_source,
        ],
    )?;
    Ok(())
}

/// 时间下界(秒):now - days*86400。
fn since_secs(days: u32) -> i64 {
    now_secs() - (days.max(1) as i64) * 86_400
}

/// 写入并同时返回(带错误吞掉,统计失败不应影响对话主流程)。
pub fn record(rec: &UsageRecord) {
    if let Err(e) = insert(rec) {
        log::warn!("usage_record 落库失败: {e}");
    }
}

// ---------------- 查询 ----------------

fn totals_on(conn: &Connection, since: i64) -> rusqlite::Result<UsageTotals> {
    conn.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(total_tokens), 0),
                SUM(CASE WHEN status <> 'success' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'rate_limited' THEN 1 ELSE 0 END),
                CAST(AVG(CASE WHEN status = 'success' THEN first_token_ms END) AS INTEGER),
                COALESCE(SUM(cost_micro), 0)
         FROM usage_record WHERE created_at >= ?1",
        params![since],
        |r| {
            Ok(UsageTotals {
                requests: r.get(0)?,
                tokens: r.get(1)?,
                failed: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                rate_limited: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                avg_first_token_ms: r.get(4)?,
                cost_micro: r.get(5)?,
            })
        },
    )
}

/// 按维度聚合(给定连接)。
/// dimension: "model" | "provider" | "key" | "agent"。
pub fn summary_on(
    conn: &Connection,
    dimension: &str,
    days: u32,
) -> rusqlite::Result<UsageSummary> {
    let since = since_secs(days);
    let totals = totals_on(conn, since)?;

    let (key_expr, name_expr) = match dimension {
        "provider" => ("provider_id", "COALESCE(provider_name, provider_id)"),
        "key" => ("CAST(slot_id AS TEXT)", "COALESCE(NULLIF(slot_label,''), '槽位 ' || slot_id)"),
        "agent" => (
            "COALESCE(NULLIF(agent_id,''), 'none')",
            "COALESCE(NULLIF(agent_name,''), NULLIF(agent_id,''), '未绑定助手')",
        ),
        _ => ("model_id", "model_id"),
    };
    let sql = format!(
        "SELECT {key_expr} AS k, {name_expr} AS nm,
                COUNT(*),
                COALESCE(SUM(total_tokens), 0),
                SUM(CASE WHEN status <> 'success' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'rate_limited' THEN 1 ELSE 0 END),
                CAST(AVG(CASE WHEN status = 'success' THEN first_token_ms END) AS INTEGER),
                COALESCE(SUM(cost_micro), 0)
         FROM usage_record WHERE created_at >= ?1
         GROUP BY k ORDER BY 4 DESC, 3 DESC LIMIT 50"
    );
    let mut stmt = conn.prepare(&sql)?;
    let groups = stmt
        .query_map(params![since], |r| {
            let key: String = r.get(0)?;
            let name: String = r.get(1)?;
            let slot_id = if dimension == "key" { key.parse::<i64>().ok() } else { None };
            Ok(UsageGroup {
                key,
                name,
                slot_id,
                cooling: false,
                requests: r.get(2)?,
                tokens: r.get(3)?,
                failed: r.get::<_, Option<i64>>(4)?.unwrap_or(0),
                rate_limited: r.get::<_, Option<i64>>(5)?.unwrap_or(0),
                avg_first_token_ms: r.get(6)?,
                cost_micro: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(UsageSummary { totals, groups })
}

/// 汇总(全局连接)。dimension: model | provider | key | agent。
pub fn summary(dimension: &str, days: u32) -> Result<UsageSummary, String> {
    let conn = db::conn().lock().unwrap_or_else(|e| e.into_inner());
    summary_on(&conn, dimension, days).map_err(|e| e.to_string())
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

fn entries_on(conn: &Connection, limit: u32) -> rusqlite::Result<Vec<UsageEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider_id, model_id, slot_id, slot_label, key_masked, status, error_kind,
                input_tokens, output_tokens, total_tokens, reasoning_tokens,
                first_token_ms, total_ms, created_at,
                agent_id, agent_name, cost_micro, cost_source
         FROM usage_record ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit.clamp(1, 500) as i64], |r| {
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
                first_token_ms: r.get(12)?,
                total_ms: r.get(13)?,
                created_at: r.get(14)?,
                agent_id: r.get(15)?,
                agent_name: r.get(16)?,
                cost_micro: r.get(17)?,
                cost_source: r.get(18)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 最近明细(全局连接)。
pub fn entries(limit: u32) -> Result<Vec<UsageEntry>, String> {
    let conn = db::conn().lock().unwrap_or_else(|e| e.into_inner());
    entries_on(&conn, limit).map_err(|e| e.to_string())
}

/// Key 健康度:某 provider 各槽位成功 / 限流 / 失败计数。
pub fn key_health_on(
    conn: &Connection,
    provider_id: &str,
    days: u32,
) -> rusqlite::Result<Vec<UsageGroup>> {
    let since = since_secs(days);
    let mut stmt = conn.prepare(
        "SELECT slot_id, COALESCE(NULLIF(slot_label,''), '槽位 ' || slot_id),
                COUNT(*),
                COALESCE(SUM(total_tokens), 0),
                SUM(CASE WHEN status <> 'success' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'rate_limited' THEN 1 ELSE 0 END),
                MAX(created_at),
                COALESCE(SUM(cost_micro), 0)
         FROM usage_record WHERE provider_id = ?1 AND created_at >= ?2
         GROUP BY slot_id ORDER BY slot_id ASC",
    )?;
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
                cost_micro: r.get(7)?,
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

/// Key 掩码:保留首 4 与末 4 字符,中间以 **** 代替(短 Key 全掩)。
pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 10 {
        return "****".into();
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}****{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        conn
    }

    fn rec(status: &str, slot: u32, tokens: i64, first_ms: Option<i64>) -> UsageRecord {
        UsageRecord {
            request_id: format!("r{slot}-{status}"),
            provider_id: "openai".into(),
            provider_name: Some("OpenAI".into()),
            model_id: "gpt-4o".into(),
            slot_id: slot,
            slot_label: Some(format!("key-{slot}")),
            key_masked: Some("sk-a****1234".into()),
            status: status.into(),
            total_tokens: Some(tokens as u32),
            first_token_ms: first_ms,
            ..Default::default()
        }
    }

    #[test]
    fn insert_and_aggregate_by_model() {
        let conn = mem();
        insert_on(&conn, &rec("success", 0, 100, Some(200))).unwrap();
        insert_on(&conn, &rec("success", 1, 300, Some(400))).unwrap();
        insert_on(&conn, &rec("rate_limited", 2, 0, None)).unwrap();

        let sum = summary_on(&conn, "model", 7).unwrap();
        assert_eq!(sum.totals.requests, 3);
        assert_eq!(sum.totals.tokens, 400);
        assert_eq!(sum.totals.failed, 1);
        assert_eq!(sum.totals.rate_limited, 1);
        assert_eq!(sum.totals.avg_first_token_ms, Some(300));
        assert_eq!(sum.groups.len(), 1);
        assert_eq!(sum.groups[0].key, "gpt-4o");
        assert_eq!(sum.groups[0].tokens, 400);
    }

    #[test]
    fn aggregate_by_provider_and_key() {
        let conn = mem();
        insert_on(&conn, &rec("success", 0, 100, Some(200))).unwrap();
        insert_on(&conn, &rec("success", 1, 300, Some(400))).unwrap();

        let by_p = summary_on(&conn, "provider", 7).unwrap();
        assert_eq!(by_p.groups.len(), 1);
        assert_eq!(by_p.groups[0].key, "openai");
        assert_eq!(by_p.groups[0].name, "OpenAI");

        let by_k = summary_on(&conn, "key", 7).unwrap();
        assert_eq!(by_k.groups.len(), 2);
        // 槽位 1 token 更多 → 排在首位
        assert_eq!(by_k.groups[0].slot_id, Some(1));
        assert_eq!(by_k.groups[1].name, "key-0");
    }

    #[test]
    fn key_health_counts_per_slot() {
        let conn = mem();
        insert_on(&conn, &rec("success", 0, 10, Some(1))).unwrap();
        insert_on(&conn, &rec("rate_limited", 0, 0, None)).unwrap();
        insert_on(&conn, &rec("failed", 1, 0, None)).unwrap();

        let h = key_health_on(&conn, "openai", 7).unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].slot_id, Some(0));
        assert_eq!(h[0].requests, 2);
        assert_eq!(h[0].rate_limited, 1);
        assert_eq!(h[1].failed, 1);
    }

    #[test]
    fn entries_are_newest_first() {
        let conn = mem();
        insert_on(&conn, &rec("success", 0, 1, None)).unwrap();
        insert_on(&conn, &rec("success", 1, 2, None)).unwrap();
        let e = entries_on(&conn, 10).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].slot_id, 1);
    }

    #[test]
    fn mask_hides_middle() {
        assert_eq!(mask_key("sk-abcdefghijklmn"), "sk-a****klmn");
        assert_eq!(mask_key("short"), "****");
    }

    /// Phase 2:按助手归因 + 成本聚合。
    #[test]
    fn aggregate_by_agent_and_cost() {
        let conn = mem();
        let mut a = rec("success", 0, 100, Some(200));
        a.agent_id = Some("chat".into());
        a.agent_name = Some("通用助手".into());
        a.cost_micro = Some(1200);
        let mut b = rec("success", 1, 300, Some(400));
        b.agent_id = Some("chat".into());
        b.agent_name = Some("通用助手".into());
        b.cost_micro = Some(800);
        // 未绑定助手的请求归入 'none'
        insert_on(&conn, &a).unwrap();
        insert_on(&conn, &b).unwrap();
        insert_on(&conn, &rec("rate_limited", 2, 0, None)).unwrap();

        let by_agent = summary_on(&conn, "agent", 7).unwrap();
        assert_eq!(by_agent.groups.len(), 2);
        let chat = by_agent.groups.iter().find(|g| g.key == "chat").unwrap();
        assert_eq!(chat.name, "通用助手");
        assert_eq!(chat.requests, 2);
        assert_eq!(chat.cost_micro, 2000);
        let none = by_agent.groups.iter().find(|g| g.key == "none").unwrap();
        assert_eq!(none.name, "未绑定助手");

        let by_model = summary_on(&conn, "model", 7).unwrap();
        assert_eq!(by_model.totals.cost_micro, 2000);
    }

    /// Phase 2:热力图分桶(dow × hour)。
    #[test]
    fn hourly_buckets_created_records() {
        use chrono::{Datelike, TimeZone, Timelike};
        let conn = mem();
        insert_on(&conn, &rec("success", 0, 100, Some(10))).unwrap();
        let buckets = hourly_on(&conn, 7).unwrap();
        // 刚写入的记录必然落在某一格(共 1 次请求 / 100 tokens)
        let total_req: i64 = buckets.iter().map(|b| b.requests).sum();
        let total_tok: i64 = buckets.iter().map(|b| b.tokens).sum();
        assert_eq!(total_req, 1);
        assert_eq!(total_tok, 100);
        let now = chrono::Local.timestamp_opt(now_secs(), 0).single().unwrap();
        let expect = buckets
            .iter()
            .find(|b| b.dow == now.weekday().num_days_from_sunday() as i64 && b.hour == now.hour() as i64);
        assert!(expect.is_some(), "应落在当前小时对应的格子");
    }
}
