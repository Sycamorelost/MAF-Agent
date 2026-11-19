//! D7/P7:对话链路上的会话事件埋点 + 三层 token 估算校准。
//!
//! record_request 现在走完整三层:
//!   L1 官方 BPE → L2 历史校准 → L3 Segmented Cache 预估
//! breakdown 包含 messages / tools / calibration / cache_saved / total,
//! 前端可直接展示「token 都花在哪了」。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::db::events;
use crate::provider::{ChatMessage, ToolDef};

use super::context_budget::{EstTokensBreakdown, est_tokens_full};

/// 消息列表指纹:role / content / 工具调用参数 / 图片数量参与哈希。
pub(crate) fn fingerprint(messages: &[ChatMessage]) -> u64 {
    let mut h = DefaultHasher::new();
    for m in messages {
        m.role.hash(&mut h);
        m.content.hash(&mut h);
        m.images.len().hash(&mut h);
        if let Some(calls) = &m.tool_calls {
            for c in calls {
                c.id.hash(&mut h);
                c.function.name.hash(&mut h);
                c.function.arguments.hash(&mut h);
            }
        }
    }
    h.finish()
}

/// 记录一次请求的规模与指纹(每轮对话一条,不按工具轮次重复)。
///
/// P7:现在算完整三层(L1×L2−L3),payload 包含完整 breakdown JSON,
/// 前端可以直接展示 token 去向,事后也能核对「为什么偏差这么大」。
///
/// - session_id:用于 Layer 2 DB 查询 + 日志关联
/// - tools:本轮工具定义(可空,None → 0 tokens)
/// - prev_messages:上轮 messages(用于 Layer 3 cache 预估,None → 首轮)
pub(crate) fn record_request(
    session_id: Option<&str>,
    provider_id: &str,
    model: &str,
    messages: &[ChatMessage],
    tools: Option<&[ToolDef]>,
    prev_messages: Option<&[ChatMessage]>,
) {
    let tools_slice = tools.unwrap_or_default();
    let brk: EstTokensBreakdown = est_tokens_full(
        messages,
        tools_slice,
        Some(model),
        session_id,
        prev_messages,
    );

    let payload = serde_json::json!({
        "provider": provider_id,
        "model": model,
        "messages": messages.len(),
        "tools": tools_slice.len(),
        "est_tokens": brk.total,
        "breakdown": {
            "messages": brk.messages,
            "tools": brk.tools,
            "l1_raw": brk.l1_raw,
            "calibration": brk.calibration,
            "l2_calibrated": brk.l2_calibrated,
            "cache_saved": brk.cache_saved,
        },
        "fingerprint": format!("{:016x}", fingerprint(messages)),
    });
    events::append(session_id, "request", &payload.to_string());
}

pub(crate) fn record_rules(session_id: Option<&str>, source: &str, chars: usize) {
    let payload = serde_json::json!({ "source": source, "chars": chars });
    events::append(session_id, "rules", &payload.to_string());
}

pub(crate) fn record_compress(
    session_id: Option<&str>,
    compressed: usize,
    before: usize,
    after: usize,
    budget: &super::context_budget::ContextBudget,
) {
    let (unit, limit) = budget.describe();
    let payload = serde_json::json!({
        "compressed": compressed,
        "before": before,
        "after": after,
        "unit": unit,
        "limit": limit,
    });
    events::append(session_id, "compress", &payload.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.into(),
            content: content.into(),
            ..Default::default()
        }
    }

    #[test]
    fn fingerprint_is_stable_and_content_sensitive() {
        let a = vec![msg("system", "固定提示"), msg("user", "你好")];
        assert_eq!(fingerprint(&a), fingerprint(&a.clone()));
        let mut b = a.clone();
        b[1].content.push('呀');
        assert_ne!(fingerprint(&a), fingerprint(&b));
    }

    #[test]
    fn record_request_tools_spec_adds_to_est_tokens() {
        let msgs = vec![msg("system", "You are helpful."), msg("user", "hi")];
        let tools = vec![ToolDef {
            r#type: "function".into(),
            function: crate::provider::ToolFunctionDef {
                name: "fs_read".into(),
                description: Some("Read a file.".into()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {"path": {"type": "string"}}
                })),
            },
        }];
        let brk_no = est_tokens_full(&msgs, &[], Some("deepseek-flash"), None, None);
        let brk_yes = est_tokens_full(&msgs, &tools, Some("deepseek-flash"), None, None);
        assert_eq!(brk_no.tools, 0);
        assert!(brk_yes.tools > 0);
        assert!(brk_yes.total > brk_no.total);
    }
}
