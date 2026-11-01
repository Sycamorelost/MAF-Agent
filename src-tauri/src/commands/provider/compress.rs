//! 上下文自动压缩(harness):超预算时用 LLM 摘要替代最早轮次,保住早期需求与结论。
//! 非破坏性:只改本次请求的内存消息,不动 DB 历史;LLM 失败返回 Err,由调用方回落硬裁剪(prune_context)。

use crate::provider::{ChatMessage, SubLlm};

use super::context_budget::ContextBudget;

/// 压缩后尾部保留的预算占整体预算的比例(≈最近 10k tokens / 26k 字符)。
/// 必须明显小于 1:否则「保留尾部」会把全部消息都留下,head 为空 →
/// 永远返回 Ok(0),LLM 摘要压缩形同虚设,早期对话被 prune_context 直接丢弃。
const KEEP_TAIL_RATIO: f64 = 0.55;
/// 旧工具结果的保留长度:工具输出通常是大头,先剪这一刀往往就不必再调 LLM 摘要。
const TOOL_RESULT_KEEP_CHARS: usize = 2_000;
/// 摘要输出 token 上限。
const SUMMARY_MAX_TOKENS: u32 = 1200;
/// 单条消息进摘要素材的截断长度(防超大工具结果灌进摘要请求)。
const HEAD_MSG_CHARS: usize = 1500;
/// 摘要子调用上限:压缩发生在「工具轮次之后、下一轮请求之前」,
/// 这段是用户完全看不到任何流式输出的静默期;官方流式客户端总超时 300s,
/// 放任其跑满会让界面看起来像卡死,因此收紧到 60s,超时直接回落硬裁剪。
const COMPRESS_TIMEOUT_SECS: u64 = 60;

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

/// 压缩入口。返回被摘要替代的早期消息条数;仅做了「旧工具结果截断」时返回 Ok(0)
/// (消息已就地变短,调用方无需提示)。无压缩素材返回 Ok(0)。
pub async fn compress_context(
    messages: &mut Vec<ChatMessage>,
    llm: &SubLlm,
    budget: &ContextBudget,
) -> Result<usize, String> {
    let groups = split_groups(messages);

    // 从尾部往前累积,直到装满保留预算(按当前预算单位计权)
    let keep_tail = (budget.limit() as f64 * KEEP_TAIL_RATIO) as usize;
    let mut keep_from = groups.len();
    let mut weight = 0usize;
    while keep_from > 0 {
        let g = &groups[keep_from - 1];
        let gw: usize = g.iter().map(|m| budget.weight(m)).sum();
        if weight + gw > keep_tail {
            break;
        }
        weight += gw;
        keep_from -= 1;
    }

    // 第一刀:旧轮次的工具输出通常占大头,先按字符截断;剪完若已回到预算内,
    // 就不必再花一次 LLM 摘要(省时省钱,也避免摘要丢失细节)。
    let head_len: usize = groups[..keep_from].iter().map(|g| g.len()).sum();
    shrink_tool_results(&mut messages[..head_len]);
    if !budget.over(messages) {
        return Ok(0);
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
    let collected = tokio::time::timeout(
        std::time::Duration::from_secs(COMPRESS_TIMEOUT_SECS),
        llm.collect(prompt, SUMMARY_MAX_TOKENS),
    )
    .await
    .map_err(|_| format!("压缩摘要超时({COMPRESS_TIMEOUT_SECS}s),回落硬裁剪"))?;
    let summary = collected?;
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

/// 截断早期工具结果(保留开头,足以看出这次调用做了什么),返回被截断的条数。
/// 只动生成本次请求的内存消息,不影响 DB 里的完整输出。
fn shrink_tool_results(slice: &mut [ChatMessage]) -> usize {
    let mut shaved = 0usize;
    for m in slice.iter_mut() {
        if m.role != "tool" {
            continue;
        }
        let chars = m.content.chars().count();
        if chars <= TOOL_RESULT_KEEP_CHARS {
            continue;
        }
        let head: String = m.content.chars().take(TOOL_RESULT_KEEP_CHARS).collect();
        m.content = format!(
            "{head}\n\n[早期工具结果已截断:原文 {chars} 字符,仅保留前 {TOOL_RESULT_KEEP_CHARS} 字符]"
        );
        shaved += 1;
    }
    shaved
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

    #[test]
    fn shrink_truncates_only_long_tool_results() {
        let long = "x".repeat(TOOL_RESULT_KEEP_CHARS + 500);
        let mut msgs = vec![
            msg("user", "u"),
            msg("assistant", "a"),
            msg("tool", &long),
            msg("tool", "short"),
        ];
        assert_eq!(shrink_tool_results(&mut msgs), 1);
        assert!(msgs[2].content.contains("早期工具结果已截断"));
        assert_eq!(msgs[3].content, "short", "短结果不应被改动");
        assert!(msgs[0].content == "u", "非 tool 消息不受影响");
    }
}
