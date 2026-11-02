//! 上下文裁剪的单元测试。

use crate::commands::provider::context_budget::ContextBudget;
use crate::provider::ChatMessage;

use super::context::{MAX_CONTEXT_MESSAGES, context_over_budget, prune_context};

fn msg(role: &str, content: &str) -> ChatMessage {
    ChatMessage {
        role: role.into(),
        content: content.into(),
        ..Default::default()
    }
}

#[test]
fn prune_keeps_short_context_untouched() {
    let mut m = vec![msg("system", "s"), msg("user", "u")];
    prune_context(&mut m, &ContextBudget::unknown());
    assert_eq!(m.len(), 2);
}

/// P6:预算按 Unicode 字符计 —— 大量中文不应因 UTF-8 字节放大而被过早裁掉。
#[test]
fn budget_counts_chars_not_bytes_for_chinese() {
    let zh = "中".repeat(4_000); // 4k 字符 / 12k 字节
    let mut m = vec![msg("system", "s")];
    for _ in 0..10 {
        m.push(msg("user", &zh));
    }
    // 40k 字符(120k 字节)在 48k 字符预算内 → 不裁剪
    let budget = ContextBudget::unknown();
    assert!(!context_over_budget(&m, &budget));
    prune_context(&mut m, &budget);
    assert_eq!(m.len(), 11);
}

#[test]
fn prune_trims_oldest_rounds_and_orphan_tools() {
    let mut m = vec![msg("system", "s"), msg("user", "u0")];
    for i in 0..120 {
        let mut a = msg("assistant", "");
        a.tool_calls = Some(Vec::new());
        m.push(a);
        m.push(msg("tool", &format!("r{i}")));
    }
    let budget = ContextBudget::unknown();
    prune_context(&mut m, &budget);
    assert!(m.len() <= MAX_CONTEXT_MESSAGES);
    assert_eq!(m[0].role, "system");
    assert_ne!(m[1].role, "tool", "裁剪后开头不应是孤立 tool 消息");
    assert_eq!(m.last().unwrap().content, "r119", "必须保留最新一轮结果");
}
