//! 上下文预算裁剪 + 跨轮压缩应用(200 轮工具调用时防止上下文无限膨胀)。
//!
//! P0 变更:
//!   - 去掉隐藏的「40 条」硬上限 —— 会话 `max_context_messages=0` 真正表示「全部」,
//!     只按模型预算裁剪;非 0 时按用户显式设置保留最近 N 条;
//!   - 新增跨轮摘要的应用:按持久化水位线丢弃旧消息,把摘要作为 system 前置。

use crate::provider::ChatMessage;

use super::super::context_budget::ContextBudget;

/// 跨轮压缩摘要的消息标记(用于识别/合并旧摘要,避免叠加多份)。
pub(crate) const SUMMARY_MARKER: &str = "【前情摘要】";

/// 是否是「早期对话摘要」system 消息。
pub(crate) fn is_summary_message(m: &ChatMessage) -> bool {
    m.role == "system" && m.content.starts_with(SUMMARY_MARKER)
}

/// 是否超预算(触发 LLM 压缩;压缩后仍超限由 prune_context 硬裁剪兜底)。
pub(crate) fn context_over_budget(messages: &[ChatMessage], budget: &ContextBudget) -> bool {
    budget.over(messages)
}

/// 应用已落库的跨轮摘要:丢弃水位线及之前的历史消息,在 system 之后插入摘要。
///
/// 只丢弃 `id > 0` 的历史消息;本轮新消息 / 工具回合(id=0)始终保留。
pub(crate) fn apply_stored_compaction(
    messages: &mut Vec<ChatMessage>,
    through_msg_id: i64,
    summary: &str,
) {
    let summary = summary.trim();
    if through_msg_id <= 0 || summary.is_empty() {
        return;
    }
    messages.retain(|m| m.id == 0 || m.id > through_msg_id);
    let at = messages
        .iter()
        .position(|m| m.role != "system")
        .unwrap_or(messages.len());
    messages.insert(
        at,
        ChatMessage {
            role: "system".into(),
            content: format!("{SUMMARY_MARKER}以下为更早对话的压缩摘要,细节已省略:\n{summary}"),
            ..Default::default()
        },
    );
}

/// 会话级条数窗口:只保留最近 `max` 条非 system 消息(0 = 不限制)。
///
/// 这是用户在会话设置里显式选择的语义(0 = 全部),不再叠加任何隐藏硬上限。
pub(crate) fn apply_session_window(messages: &mut Vec<ChatMessage>, max: usize) {
    if max == 0 {
        return;
    }
    let non_system = messages.iter().filter(|m| m.role != "system").count();
    if non_system <= max {
        return;
    }
    let mut keep: Vec<ChatMessage> = Vec::new();
    for m in messages.iter().rev() {
        if m.role == "system" {
            continue;
        }
        if keep.len() >= max {
            break;
        }
        keep.push(m.clone());
    }
    keep.reverse();
    // 丢掉开头没有对应 assistant 的 tool 结果(否则协议 400)
    while keep.first().map(|m| m.role == "tool").unwrap_or(false) {
        keep.remove(0);
    }
    messages.retain(|m| m.role == "system");
    messages.extend(keep);
}

/// 裁剪过长的工具调用上下文:
///   - 永远保留 system;
///   - 其余按预算保留最近若干条(不再有条数硬上限);
///   - 裁剪后若开头是孤立的 tool 消息(其 assistant 已被裁掉),一并丢弃,
///     否则 OpenAI function-calling 协议不合法(400)。
pub(crate) fn prune_context(messages: &mut Vec<ChatMessage>, budget: &ContextBudget) {
    let total: usize = messages.iter().map(|m| budget.weight(m)).sum();
    if total <= budget.limit() {
        return;
    }
    let system: Vec<ChatMessage> = messages
        .iter()
        .filter(|m| m.role == "system")
        .cloned()
        .collect();
    let mut tail: Vec<ChatMessage> = messages
        .iter()
        .filter(|m| m.role != "system")
        .cloned()
        .collect();

    let mut weight: usize =
        system.iter().chain(tail.iter()).map(|m| budget.weight(m)).sum();
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
