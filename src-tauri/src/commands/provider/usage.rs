//! 用量统计命令。

use crate::db::usage::{self, UsageEntry, UsageGroup, UsageHour, UsageSummary};

use super::pricing::{price_tokens_nano, pricing_for};

// ---------- 用量统计 ----------

/// 按维度聚合用量(dimension: model | provider | key | agent;days: 统计窗口天数)。
#[tauri::command]
pub async fn usage_summary(dimension: String, days: u32) -> Result<UsageSummary, String> {
    let dim = match dimension.as_str() {
        "provider" | "key" | "agent" => dimension.as_str(),
        _ => "model",
    };
    usage::summary(dim, days)
}

/// 最近请求明细(倒序,最多 500 条)。
/// 时间窗口与维度下钻全部下沉 SQL:明细必须与上方图表同口径,
/// 不允许"图表近 7 天、明细全历史"的错位。
#[tauri::command]
pub async fn usage_entries(
    limit: u32,
    days: u32,
    dimension: Option<String>,
    key: Option<String>,
) -> Result<Vec<UsageEntry>, String> {
    let dim = dimension.as_deref().map(|d| match d {
        "provider" | "key" | "agent" => d,
        _ => "model",
    });
    usage::entries(limit, days, dim, key.as_deref())
}

/// 某 Provider 各槽位的健康度(成功 / 限流 / 失败 / token)。
#[tauri::command]
pub async fn usage_key_health(provider_id: String, days: u32) -> Result<Vec<UsageGroup>, String> {
    crate::memory::validate_id(&provider_id)?;
    usage::key_health(&provider_id, days)
}

/// 热力图数据:近 days 天内按「星期 × 小时」分桶的请求数 / Token 数。
#[tauri::command]
pub async fn usage_hourly(days: u32) -> Result<Vec<UsageHour>, String> {
    usage::hourly(days)
}

/// 复算报告的一行(按 provider × model 聚合,只读对比)。
#[derive(Debug, serde::Serialize)]
pub struct UsageRecalcRow {
    pub provider_id: String,
    pub model_id: String,
    pub requests: i64,
    /// 当时落库的成本(纳元)
    pub stored_nano: i64,
    /// 按当前定价试算的成本(纳元);None = 当前无定价
    pub current_nano: Option<i64>,
    /// 差异(试算 - 落库);None = 无法试算
    pub delta_nano: Option<i64>,
    /// 历史未计价请求数
    pub unpriced_requests: i64,
}

/// 复算报告。
#[derive(Debug, serde::Serialize)]
pub struct UsageRecalcReport {
    pub days: u32,
    pub rows: Vec<UsageRecalcRow>,
    pub stored_total_nano: i64,
    pub current_total_nano: i64,
}

/// 按当前定价试算历史成本(干跑,只报告差异,不写库)。
/// 用于调价 / 修正汇率后评估历史偏差,对账入口在设置页「用量统计」。
#[tauri::command]
pub async fn usage_recalc(days: u32) -> Result<UsageRecalcReport, String> {
    let mixes = usage::token_mix(days)?;
    let mut rows = Vec::with_capacity(mixes.len());
    let mut stored_total = 0i64;
    let mut current_total = 0i64;
    for m in mixes {
        let current = pricing_for(&m.provider_id, &m.model_id)
            .and_then(|p| price_tokens_nano(&p, m.input_tokens, m.output_tokens, m.cached_tokens));
        stored_total += m.stored_nano;
        current_total += current.unwrap_or(0);
        rows.push(UsageRecalcRow {
            provider_id: m.provider_id,
            model_id: m.model_id,
            requests: m.requests,
            stored_nano: m.stored_nano,
            current_nano: current,
            delta_nano: current.map(|c| c - m.stored_nano),
            unpriced_requests: m.unpriced_requests,
        });
    }
    // 差异最大的排前面,便于一眼看到偏差来源
    rows.sort_by(|a, b| {
        let da = a.delta_nano.map(|d| d.abs()).unwrap_or(-1);
        let db = b.delta_nano.map(|d| d.abs()).unwrap_or(-1);
        db.cmp(&da)
    });
    Ok(UsageRecalcReport {
        days,
        rows,
        stored_total_nano: stored_total,
        current_total_nano: current_total,
    })
}
