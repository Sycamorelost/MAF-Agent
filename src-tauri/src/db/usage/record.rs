//! usage_record 写入(append-only)与 Key 掩码。
//!
//! 对应 DEVPLAN-api-rotation-usage.md §4.1。
//!   - 每次实际发出的请求落一行,成功 / 失败 / 限流都记;
//!   - slot_id / slot_label / key_masked 为 Key 归因主线;
//!   - 只存 Key 掩码,绝不明文。

use rusqlite::{Connection, params};

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
    /// 缓存命中输入 token(V17;厂商未返回为 None = 未知)
    pub cached_tokens: Option<u32>,
    pub first_token_ms: Option<i64>,
    pub total_ms: Option<i64>,
    /// 会话绑定的 Agent 模板 id(按助手归因;未绑定为 None)
    pub agent_id: Option<String>,
    /// Agent 展示名(便于直接分组展示)
    pub agent_name: Option<String>,
    /// 成本(纳元,1e-9 元,V17);None = 无定价信息
    pub cost_nano: Option<i64>,
    /// 计价快照 JSON(落库当时的单价,保证历史成本可复算)
    pub pricing_snapshot: Option<String>,
    /// 计价来源:model_card | custom | unpriced | none
    pub cost_source: Option<String>,
}

pub(crate) fn now_secs() -> i64 {
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
            cached_tokens, first_token_ms, total_ms, created_at,
            agent_id, agent_name, cost_nano, pricing_snapshot, cost_source
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)",
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
            rec.cached_tokens.map(|v| v as i64),
            rec.first_token_ms,
            rec.total_ms,
            now_secs(),
            rec.agent_id,
            rec.agent_name,
            rec.cost_nano,
            rec.pricing_snapshot,
            rec.cost_source,
        ],
    )?;
    Ok(())
}

/// 写入并同时返回(带错误吞掉,统计失败不应影响对话主流程)。
pub fn record(rec: &UsageRecord) {
    if let Err(e) = insert(rec) {
        log::warn!("usage_record 落库失败: {e}");
    }
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

    #[test]
    fn mask_hides_middle() {
        assert_eq!(mask_key("sk-abcdefghijklmn"), "sk-a****klmn");
        assert_eq!(mask_key("short"), "****");
    }
}
