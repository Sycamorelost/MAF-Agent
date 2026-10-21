//! usage_record 查询与聚合(API 流转 · 用量统计)。
//!
//! 口径(与设置页说明一致):
//!   total_tokens = input + output(reasoning 已含在 output 内,单独展示不重复相加)
//!   cost         = (input - cached) × miss_price + cached × hit_price + output × output_price
//!   currency     = CNY;USD 来源按快照内 fx_rate 换算并标注
//!   coverage     = 有定价的成功请求 / 成功请求
//!
//! 成本一律用纳元(1e-9 元)聚合;旧记录只有微元 cost_micro 时按 ×1000 兜底,
//! 保证历史数字不跳变、聚合不丢精度。
//!
//! 拆分:`totals` 负责总览聚合,`entries` 负责明细与 Key 健康度,`hourly` 负责热力图,
//! `tokens` 负责复算输入;本文件保留共享类型与聚合入口(summary)。

mod entries;
mod hourly;
#[cfg(test)]
mod tests;
mod tokens;
mod totals;

pub use entries::{entries, key_health, UsageEntry};
pub use hourly::{hourly, UsageHour};
pub use tokens::token_mix;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::db;

use super::record::now_secs;

/// 有效成本表达式:新记录走 cost_nano,旧记录回退 cost_micro × 1000。
const COST: &str = "COALESCE(cost_nano, cost_micro * 1000)";

/// 聚合总览。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageTotals {
    pub requests: i64,
    pub tokens: i64,
    pub failed: i64,
    pub rate_limited: i64,
    pub avg_first_token_ms: Option<i64>,
    /// 总成本(纳元,1e-9 元);未计价请求不计入
    pub cost_nano: i64,
    /// 成功请求数(coverage 分母)
    pub successful_requests: i64,
    /// 成功且有定价的请求数
    pub costed_requests: i64,
    /// 成功但未计价的请求数
    pub uncosted_requests: i64,
    /// 计价覆盖率 0..1(无成功请求时为 0)
    pub cost_coverage: f64,
}

/// 一个分组(按模型 / provider / Key / 助手)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageGroup {
    /// 分组键(model_id / provider_id / slot 号 / agent_id;其他桶为 `__other__`)
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
    /// 该分组成本(纳元)
    pub cost_nano: i64,
    /// 该分组成功但未计价的请求数
    pub uncosted_requests: i64,
}

/// 归因汇总(总览 + 分组)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageSummary {
    pub totals: UsageTotals,
    pub groups: Vec<UsageGroup>,
}

/// 时间下界(秒):now - days*86400。
pub(crate) fn since_secs(days: u32) -> i64 {
    now_secs() - (days.max(1) as i64) * 86_400
}

/// 按维度聚合(给定连接)。
/// dimension: "model" | "provider" | "key" | "agent"。
///
/// 分组取前 50;若超过 50 个分类,追加「其他」桶(总数减前 50 之和),
/// 保证 Σ分组 = 总览,避免用户看到"分组加不起来"。
pub fn summary_on(
    conn: &Connection,
    dimension: &str,
    days: u32,
) -> rusqlite::Result<UsageSummary> {
    let since = since_secs(days);
    let totals = totals::totals_on(conn, since)?;

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
                COALESCE(SUM({COST}), 0),
                SUM(CASE WHEN status = 'success' AND {COST} IS NULL THEN 1 ELSE 0 END)
         FROM usage_record WHERE created_at >= ?1
         GROUP BY k ORDER BY 4 DESC, 3 DESC LIMIT 51"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut groups = stmt
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
                cost_nano: r.get(7)?,
                uncosted_requests: r.get::<_, Option<i64>>(8)?.unwrap_or(0),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if groups.len() > 50 {
        groups.truncate(50);
        let sum = |f: fn(&UsageGroup) -> i64| groups.iter().map(f).sum::<i64>();
        groups.push(UsageGroup {
            key: "__other__".into(),
            name: "其他".into(),
            slot_id: None,
            cooling: false,
            requests: totals.requests - sum(|g| g.requests),
            tokens: totals.tokens - sum(|g| g.tokens),
            failed: totals.failed - sum(|g| g.failed),
            rate_limited: totals.rate_limited - sum(|g| g.rate_limited),
            avg_first_token_ms: None,
            cost_nano: totals.cost_nano - sum(|g| g.cost_nano),
            uncosted_requests: totals.uncosted_requests - sum(|g| g.uncosted_requests),
        });
    }

    Ok(UsageSummary { totals, groups })
}

/// 汇总(全局连接)。dimension: model | provider | key | agent。
pub fn summary(dimension: &str, days: u32) -> Result<UsageSummary, String> {
    let conn = db::conn().lock().unwrap_or_else(|e| e.into_inner());
    summary_on(&conn, dimension, days).map_err(|e| e.to_string())
}
