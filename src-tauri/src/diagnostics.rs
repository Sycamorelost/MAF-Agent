//! 运行期「静默失败」计数(P2)。
//!
//! 事件发送(`emit`)、会话事件落库、MCP 审计写入都属于旁路操作:失败不该打断主流程,
//! 但也不该完全静默。统一经本模块计数 + 打 warn 日志,设置页「关于」在计数非 0 时
//! 展示一行提示,便于用户反馈问题。不新增任何用户可调项。

use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use tauri::Emitter;

/// 失败类别:向前端发送事件。
pub const KIND_EMIT: &str = "emit";
/// 失败类别:会话事件落库(`db::events`)。
pub const KIND_EVENT_LOG: &str = "event_log";
/// 失败类别:MCP 审计日志写入(`mcp::audit`)。
pub const KIND_AUDIT: &str = "audit";

static EMIT_FAILURES: AtomicU64 = AtomicU64::new(0);
static EVENT_LOG_FAILURES: AtomicU64 = AtomicU64::new(0);
static AUDIT_FAILURES: AtomicU64 = AtomicU64::new(0);

/// 失败计数快照(供 IPC 读取)。
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct FailureCounts {
    pub emit: u64,
    pub event_log: u64,
    pub audit: u64,
}

fn counter(kind: &str) -> Option<&'static AtomicU64> {
    match kind {
        KIND_EMIT => Some(&EMIT_FAILURES),
        KIND_EVENT_LOG => Some(&EVENT_LOG_FAILURES),
        KIND_AUDIT => Some(&AUDIT_FAILURES),
        _ => None,
    }
}

/// 记录一次可恢复失败:打 warn 日志并累加计数。
pub fn record_failure(kind: &str, detail: &str) {
    match counter(kind) {
        Some(c) => {
            let n = c.fetch_add(1, Ordering::Relaxed) + 1;
            log::warn!("{kind} failed(累计 {n} 次): {detail}");
        }
        None => log::warn!("{kind} failed: {detail}"),
    }
}

/// 读取当前失败计数。
pub fn failure_counts() -> FailureCounts {
    FailureCounts {
        emit: EMIT_FAILURES.load(Ordering::Relaxed),
        event_log: EVENT_LOG_FAILURES.load(Ordering::Relaxed),
        audit: AUDIT_FAILURES.load(Ordering::Relaxed),
    }
}

/// 广播事件;失败计数并告警,不影响调用方流程。
pub fn safe_emit<T: Serialize + Clone>(app: &tauri::AppHandle, event: &str, payload: T) {
    if let Err(e) = app.emit(event, payload) {
        record_failure(KIND_EMIT, &format!("{event}: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_counts_accumulate() {
        let before = failure_counts().audit;
        record_failure(KIND_AUDIT, "test");
        assert_eq!(failure_counts().audit, before + 1);
        // 未知类别只告警,不计入任何计数器
        record_failure("unknown", "test");
        assert_eq!(failure_counts().audit, before + 1);
    }
}
