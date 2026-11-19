//! 生成轮次的轻量判定辅助。

use std::time::{Duration, Instant};

/// 整体生成剩余预算(总时长扣掉已用)。
pub(super) fn remaining_budget(started: &Instant, total: Duration) -> Duration {
    total.saturating_sub(started.elapsed())
}

/// 任务清单同步提醒的触发条件:会话存在未完成任务(in_progress/pending)。
/// 读库失败按「不需要」处理,绝不因面板状态影响主流。
pub(super) fn todos_sync_needed(session_id: &Option<String>) -> bool {
    let Some(sid) = session_id.as_deref() else {
        return false;
    };
    let Ok(conn) = crate::db::conn().lock() else {
        return false;
    };
    crate::db::todos::list(&conn, sid)
        .map(|items| {
            items
                .iter()
                .any(|t| matches!(t.status.as_str(), "in_progress" | "pending"))
        })
        .unwrap_or(false)
}

/// 收尾答复是否在等待用户决策:尾部出现疑问标点即视为阻塞于用户输入,
/// 此时任务清单未更新是正确状态,不做「先同步清单」补轮(否则会空转刷屏)。
pub(super) fn awaiting_user_decision(text: &str) -> bool {
    let tail: String = text.chars().rev().take(240).collect();
    tail.contains('？') || tail.contains('?')
}
