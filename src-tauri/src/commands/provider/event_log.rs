//! D7:对话链路上的会话事件埋点(请求指纹 / 项目约定注入 / 上下文压缩)。
//!
//! 只做「可解释」的最小版:把运行时对请求的内存态改写记进 `session_events`,
//! 不改消息表、不改渲染,也不参与任何决策。存储与裁剪由 `db::events` 负责。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::db::events;
use crate::provider::ChatMessage;

use super::context_budget::{ContextBudget, messages_tokens};

/// 消息列表指纹:role / content / 工具调用参数 / 图片数量参与哈希。
/// 用于事后比对本轮「模型实际看到了什么」是否与此前某轮一致(压缩与注入可见地改变它)。
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
pub(crate) fn record_request(
    session_id: Option<&str>,
    provider_id: &str,
    model: &str,
    messages: &[ChatMessage],
    tools: usize,
) {
    let payload = serde_json::json!({
        "provider": provider_id,
        "model": model,
        "messages": messages.len(),
        "tools": tools,
        "est_tokens": messages_tokens(messages),
        "fingerprint": format!("{:016x}", fingerprint(messages)),
    });
    events::append(session_id, "request", &payload.to_string());
}

/// 记录项目约定注入(哪个文件、注入多少字符)。
pub(crate) fn record_rules(session_id: Option<&str>, source: &str, chars: usize) {
    let payload = serde_json::json!({ "source": source, "chars": chars });
    events::append(session_id, "rules", &payload.to_string());
}

/// 记录一次上下文压缩前后的条数与预算口径,便于核对「为什么被压缩」。
pub(crate) fn record_compress(
    session_id: Option<&str>,
    compressed: usize,
    before: usize,
    after: usize,
    budget: &ContextBudget,
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
        assert_ne!(fingerprint(&a), fingerprint(&b), "正文变化必须改变指纹");
        let mut c = a.clone();
        c[0].role = "user".into();
        assert_ne!(fingerprint(&a), fingerprint(&c), "角色变化必须改变指纹");
    }

    #[test]
    fn fingerprint_covers_tool_arguments() {
        use crate::provider::{ToolCall, ToolCallFunction};
        let call = |args: &str| ChatMessage {
            role: "assistant".into(),
            content: String::new(),
            tool_calls: Some(vec![ToolCall {
                id: "c1".into(),
                r#type: "function".into(),
                function: ToolCallFunction {
                    name: "fs_read".into(),
                    arguments: args.into(),
                },
            }]),
            ..Default::default()
        };
        assert_ne!(fingerprint(&[call("{\"path\":\"a\"}")]), fingerprint(&[call("{\"path\":\"b\"}")]));
    }
}
