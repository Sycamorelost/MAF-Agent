//! 上下文裁剪的单元测试。

use crate::commands::provider::context_budget::ContextBudget;
use crate::provider::ChatMessage;

use super::context::{
    SUMMARY_MARKER, apply_session_window, apply_stored_compaction, context_over_budget,
    prune_context,
};

fn msg(role: &str, content: &str) -> ChatMessage {
    ChatMessage {
        role: role.into(),
        content: content.into(),
        ..Default::default()
    }
}

fn msg_id(id: i64, role: &str, content: &str) -> ChatMessage {
    ChatMessage {
        id,
        ..msg(role, content)
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

/// P0:不再有隐藏的 40 条上限 —— 未超预算的 240 条消息原样保留。
#[test]
fn no_hidden_message_count_cap() {
    let mut m = vec![msg("system", "s")];
    for i in 0..240 {
        m.push(msg("user", &format!("u{i}")));
    }
    let budget = ContextBudget::unknown();
    assert!(!context_over_budget(&m, &budget));
    prune_context(&mut m, &budget);
    assert_eq!(m.len(), 241);
}

#[test]
fn prune_trims_by_budget_and_drops_orphan_tools() {
    let mut m = vec![msg("system", "s"), msg("user", "u0")];
    let big = "x".repeat(2_000);
    for i in 0..120 {
        let mut a = msg("assistant", "");
        a.tool_calls = Some(Vec::new());
        m.push(a);
        m.push(msg("tool", &format!("{big}{i}")));
    }
    let budget = ContextBudget::unknown();
    prune_context(&mut m, &budget);
    assert!(m.len() > 2, "至少保留最近若干轮");
    assert_eq!(m[0].role, "system");
    assert_ne!(m[1].role, "tool", "裁剪后开头不应是孤立 tool 消息");
    assert_eq!(
        m.last().unwrap().content,
        format!("{big}119"),
        "必须保留最新一轮结果"
    );
}

/// P0:跨轮摘要按水位线丢弃旧历史,并把摘要放在 system 之后。
#[test]
fn stored_compaction_drops_covered_history() {
    let mut m = vec![
        msg("system", "sys"),
        msg_id(1, "user", "很旧的问题"),
        msg_id(2, "assistant", "很旧的回答"),
        msg_id(3, "user", "新问题"),
        msg_id(4, "assistant", "新回答"),
    ];
    apply_stored_compaction(&mut m, 2, "旧轮次的结论");
    let contents: Vec<&str> = m.iter().map(|x| x.content.as_str()).collect();
    assert_eq!(contents[0], "sys");
    assert!(contents[1].starts_with(SUMMARY_MARKER));
    assert!(contents[1].contains("旧轮次的结论"));
    assert_eq!(contents[2], "新问题");
    assert_eq!(contents[3], "新回答");
}

/// 本轮新消息(id=0)与 system 永远不被水位线丢弃。
#[test]
fn stored_compaction_keeps_id_zero_messages() {
    let mut m = vec![
        msg("system", "sys"),
        msg_id(1, "user", "旧"),
        msg_id(0, "user", "本轮输入"),
    ];
    apply_stored_compaction(&mut m, 10, "摘要");
    assert!(m.iter().any(|x| x.content == "本轮输入"));
    assert!(!m.iter().any(|x| x.content == "旧"));
}

/// 0 = 全部:显式关闭条数窗口时一条都不丢。
#[test]
fn session_window_zero_means_all() {
    let mut m = vec![msg("system", "s")];
    for i in 0..100 {
        m.push(msg_id(i + 1, "user", "u"));
    }
    apply_session_window(&mut m, 0);
    assert_eq!(m.len(), 101);
}

/// 非 0 = 保留最近 N 条非 system 消息,system 全保留。
#[test]
fn session_window_keeps_latest_non_system() {
    let mut m = vec![msg("system", "s")];
    for i in 0..10 {
        m.push(msg_id(i + 1, "user", &format!("u{i}")));
    }
    apply_session_window(&mut m, 3);
    assert_eq!(m.len(), 4);
    assert_eq!(m[0].content, "s");
    assert_eq!(m[1].content, "u7");
    assert_eq!(m[3].content, "u9");
}
