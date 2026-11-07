//! C3:指数退避重连 + 连续失败自动禁用。
//!
//! 原先是 runtime.rs 的内嵌逻辑,抽出来后 runtime.rs 回到 700 行以内。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::Mutex;

/// 单 server 启动/探活失败的最大重试次数;耗尽后标记 disabled_by_failure,
/// 后续 start_server 直接拒绝,直到用户手动启用或应用重启。
pub const MAX_FAILURE_RETRIES: u32 = 10;
/// 指数退避起始等待时间(ms);失败次数累加时等待 = base * 2^min(count,20) ms,上限 RETRY_CAP_MS。
pub const RETRY_BASE_MS: u64 = 500;
/// 指数退避上限(ms)。
pub const RETRY_CAP_MS: u64 = 30_000;

/// C3 失败追踪状态包 —— 由 McpRuntime 持有两个 Arc<Mutex<...>> 槽。
///
/// 独立出来而非直接在 McpRuntime 里存,是因为 start_from_config 的全生命周期都用得到,
/// 抽成模块后方便独立测试与未来扩展(比如按 server 存每次失败原因)。
#[derive(Default, Clone)]
pub struct FailureTracker {
    /// 单 server 连续失败次数。
    pub failure_counts: Arc<Mutex<HashMap<String, u32>>>,
    /// 连续失败次数耗尽后,标记 disabled_by_failure 的 server 名集合。
    pub disabled_by_failure: Arc<Mutex<HashSet<String>>>,
}

impl FailureTracker {
    /// 查询 server 是否已被标记 disabled_by_failure。
    pub async fn is_disabled(&self, name: &str) -> bool {
        self.disabled_by_failure.lock().await.contains(name)
    }

    /// 启动成功时调用 —— 重置失败计数与 disabled 标记。
    pub async fn clear(&self, name: &str) {
        self.failure_counts.lock().await.remove(name);
        self.disabled_by_failure.lock().await.remove(name);
    }

    /// 累加失败计数;耗尽 MAX_FAILURE_RETRIES 后标记 disabled_by_failure。
    /// 返回是否已禁用(调用方可以据此返回明确的错误信息给前端)。
    pub async fn increment(&self, name: &str) -> bool {
        let mut counts = self.failure_counts.lock().await;
        let count = counts.entry(name.to_string()).or_insert(0);
        *count += 1;
        let c = *count;
        if c >= MAX_FAILURE_RETRIES {
            drop(counts);
            self.disabled_by_failure.lock().await.insert(name.to_string());
            log::warn!(
                "[MCP] server `{name}` 连续失败 {c} 次,自动禁用;等待下一次退避 {}ms",
                backoff_ms(c)
            );
            true
        } else {
            false
        }
    }
}

/// 指数退避等待时间(ms),输入失败次数,返回等待毫秒(含上限)。
pub fn backoff_ms(count: u32) -> u64 {
    let exp = count.min(20); // 2^20 ≈ 100 万,上限 30s 足够
    let ms = RETRY_BASE_MS.saturating_mul(1u64 << exp);
    ms.min(RETRY_CAP_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_monotonic_increases_with_cap() {
        let prev = backoff_ms(0);
        assert_eq!(prev, RETRY_BASE_MS);
        for c in 1..20u32 {
            let cur = backoff_ms(c);
            assert!(cur >= prev, "退避必须递增");
        }
        let capped = backoff_ms(25); // >20 的也应 cap
        assert!(capped <= RETRY_CAP_MS);
    }

    #[tokio::test]
    async fn tracker_disables_after_max_retries() {
        let t = FailureTracker::default();
        for _ in 0..(MAX_FAILURE_RETRIES - 1) {
            let disabled = t.increment("srv").await;
            assert!(!disabled);
        }
        let disabled = t.increment("srv").await;
        assert!(disabled);
        assert!(t.is_disabled("srv").await);

        // clear 后恢复
        t.clear("srv").await;
        assert!(!t.is_disabled("srv").await);
        assert!(t.failure_counts.lock().await.is_empty());
    }
}
