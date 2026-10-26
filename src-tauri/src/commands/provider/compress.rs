//! 上下文自动压缩(harness):超预算时用 LLM 摘要替代最早轮次,保住早期需求与结论。
//! 非破坏性:只改本次请求的内存消息,不动 DB 历史;LLM 失败返回 Err,由调用方回落硬裁剪(prune_context)。

use crate::provider::{ChatMessage, SubLlm};

/// 压缩后尾部保留的字符预算(≈最近 30k tokens;需小于 chat.rs 的 MAX_CONTEXT_CHARS 留出摘要空间)。
const KEEP_TAIL_CHARS: usize = 120_000;
/// 摘要输出 token 上限。
const SUMMARY_MAX_TOKENS: u32 = 1200;
/// 单条消息进摘要素材的截断长度(防超大工具结果灌进摘要请求)。
const HEAD_MSG_CHARS: usize = 1500;

/// 把消息切成不可分割组:assistant(带 tool_calls)与其后连续的 tool 消息同组,其余各自成组。
/// 压缩只能按组丢弃,否则 OpenAI function-calling 协议出现孤儿 tool 消息 → 400。
pub fn split_groups(messages: &[ChatMessage]) -> Vec<Vec<ChatMessage>> {
    let mut groups: Vec<Vec<ChatMessage>> = Vec::new();
    for m in messages {
        let can_join = m.role == "tool"
            && groups
                .last()
                .map(|g| g.iter().any(|x| x.tool_calls.is_some()))
                .unwrap_or(false);
        if can_join {
            if let Some(g) = groups.last_mut() {
                g.push(m.clone());
            }
            continue;
        }
        groups.push(vec![m.clone()]);
    }
    groups
}

/// 压缩入口。返回被压缩掉的早期消息条数;无压缩素材返回 Ok(0)。
pub async fn compress_context(
    messages: &mut Vec<ChatMessage>,
    llm: &SubLlm,
) -> Result<usize, String> {
    let groups = split_groups(messages);

    // 从尾部往前累积,直到装满保留预算
    let mut keep_from = groups.len();
    let mut chars = 0usize;
    while keep_from > 0 {
        let g = &groups[keep_from - 1];
        let gc: usize = g.iter().map(|m| m.content.len()).sum();
        if chars + gc > KEEP_TAIL_CHARS {
            break;
        }
        chars += gc;
        keep_from -= 1;
    }

    let head = &groups[..keep_from];
    let head_msgs: usize = head.iter().map(|g| g.len()).sum();
    let head_has_dialog = head
        .iter()
        .flatten()
        .any(|m| m.role == "user" || m.role == "assistant");
    if head_msgs == 0 || !head_has_dialog {
        return Ok(0); // 没有可压缩的对话内容(可能整条尾巴就已超预算,交给硬裁剪)
    }

    // 摘要素材:逐条截断后拼接 role + 内容
    let mut material = String::new();
    for m in head.iter().flatten() {
        let c: String = m.content.chars().take(HEAD_MSG_CHARS).collect();
        material.push_str(&format!("[{}] {}\n\n", m.role, c));
    }
    let prompt = vec![
        ChatMessage {
            role: "system".into(),
            content: "你是对话压缩器。把给定的早期对话压缩成一段中文摘要,必须保留:用户的需求与目标、\
                      关键决策与结论、已执行的重要操作及其结果、尚未完成的事项。丢弃寒暄与重复内容。\
                      直接输出摘要,不要解释。"
                .into(),
            ..Default::default()
        },
        ChatMessage {
            role: "user".into(),
            content: material,
            ..Default::default()
        },
    ];
    let summary = llm.collect(prompt, SUMMARY_MAX_TOKENS).await?;
    let summary = summary.trim();
    if summary.is_empty() {
        return Err("压缩摘要为空".into());
    }

    // 重组:system 全部置顶(与 prune_context 语义一致)→ 摘要 → 保留尾部(滤掉 system 防重复)
    let mut rebuilt: Vec<ChatMessage> = messages
        .iter()
        .filter(|m| m.role == "system")
        .cloned()
        .collect();
    rebuilt.push(ChatMessage {
        role: "system".into(),
        content: format!(
            "【前情摘要】以下为早期 {head_msgs} 条对话的压缩摘要,细节已省略:\n{summary}"
        ),
        ..Default::default()
    });
    for g in &groups[keep_from..] {
        rebuilt.extend(g.iter().filter(|m| m.role != "system").cloned());
    }
    *messages = rebuilt;
    Ok(head_msgs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ToolCall;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.into(),
            content: content.into(),
            ..Default::default()
        }
    }

    fn tool_call_msg() -> ChatMessage {
        ChatMessage {
            role: "assistant".into(),
            content: String::new(),
            tool_calls: Some(vec![ToolCall {
                id: "c1".into(),
                r#type: "function".into(),
                function: crate::provider::ToolCallFunction {
                    name: "fs_read".into(),
                    arguments: "{}".into(),
                },
            }]),
            ..Default::default()
        }
    }

    #[test]
    fn groups_keep_tool_call_chain_together() {
        let msgs = vec![
            msg("system", "s"),
            msg("user", "u1"),
            tool_call_msg(),
            ChatMessage {
                role: "tool".into(),
                content: "r1".into(),
                tool_call_id: Some("c1".into()),
                ..Default::default()
            },
            msg("assistant", "a1"),
        ];
        let groups = split_groups(&msgs);
        assert_eq!(groups.len(), 4);
        assert_eq!(
            groups[2].len(),
            2,
            "tool_calls 组应含 assistant + tool 两条"
        );
        assert_eq!(groups[2][0].tool_calls.as_ref().unwrap()[0].id, "c1");
    }

    #[test]
    fn orphan_tool_message_forms_own_group() {
        let msgs = vec![msg("user", "u"), msg("tool", "orphan")];
        let groups = split_groups(&msgs);
        assert_eq!(groups.len(), 2, "孤儿 tool 不并入前组");
    }
}
