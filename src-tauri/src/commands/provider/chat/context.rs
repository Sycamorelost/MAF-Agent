//! 上下文预算裁剪(200 轮工具调用时防止上下文无限膨胀)。

use crate::provider::ChatMessage;

use super::super::context_budget::ContextBudget;

/// 单次请求最多携带的消息条数(200 轮工具调用时防止上下文无限膨胀)。
pub(crate) const MAX_CONTEXT_MESSAGES: usize = 40;

/// 是否超预算(触发 LLM 压缩;压缩后仍超限由 prune_context 硬裁剪兜底)。
pub(crate) fn context_over_budget(messages: &[ChatMessage], budget: &ContextBudget) -> bool {
    messages.len() > MAX_CONTEXT_MESSAGES || budget.over(messages)
}

/// 裁剪过长的工具调用上下文:
///   - 永远保留 system;
///   - 其余只保留最近 N 条 / 预算内(D5:预算按模型上下文窗口折算为 token);
///   - 裁剪后若开头是孤立的 tool 消息(其 assistant 已被裁掉),一并丢弃,
///     否则 OpenAI function-calling 协议不合法(400)。
pub(crate) fn prune_context(messages: &mut Vec<ChatMessage>, budget: &ContextBudget) {
    let total: usize = messages.iter().map(|m| budget.weight(m)).sum();
    if messages.len() <= MAX_CONTEXT_MESSAGES && total <= budget.limit() {
        return;
    }
    let system: Vec<ChatMessage> = messages
        .iter()
        .filter(|m| m.role == "system")
        .cloned()
        .collect();
    let keep = MAX_CONTEXT_MESSAGES.saturating_sub(system.len()).max(1);
    let mut tail: Vec<ChatMessage> = messages
        .iter()
        .filter(|m| m.role != "system")
        .rev()
        .take(keep)
        .cloned()
        .collect();
    tail.reverse();

    let mut weight: usize = system.iter().chain(tail.iter()).map(|m| budget.weight(m)).sum();
    while weight > budget.limit() && tail.len() > 1 {
        weight = weight.saturating_sub(budget.weight(&tail[0]));
        tail.remove(0);
    }
    // 丢掉开头没有对应 assistant 的 tool 结果
    while tail.first().map(|m| m.role == "tool").unwrap_or(false) {
        tail.remove(0);
    }

    messages.clear();
    messages.extend(system);
    messages.extend(tail);
}
