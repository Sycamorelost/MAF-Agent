//! 能力令牌(Capability Token)—— 用户预授权的短期凭据。
//!
//! 设计要点:
//! 1. 仅存内存,应用重启即失效,不落盘避免凭据长期化;
//! 2. 令牌限定「作用域」:文件只读 / 文件写入 / 命令执行 / 网络(MCP)/ GUI 自动化;
//! 3. 有效期上限 8 小时,可再叠加「最大使用次数」;
//! 4. 命中令牌只跳过 HITL 弹窗,shell 白名单、目录白名单、GUI 进程白名单仍然生效。

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 令牌作用域 —— 与工具类别一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// 只读文件 / 时间查询
    FsRead,
    /// 写入、删除、导出、解包
    FsWrite,
    /// 命令执行
    Exec,
    /// 外部 MCP 只读查询
    Net,
    /// GUI 自动化(点击 / 输入 / 聚焦)
    Ui,
}

impl Scope {
    /// 前端可枚举的全部作用域(保留给设置页与后续命令行工具使用)
    #[allow(dead_code)]
    pub const ALL: [Scope; 5] = [
        Scope::FsRead,
        Scope::FsWrite,
        Scope::Exec,
        Scope::Net,
        Scope::Ui,
    ];

    /// 中文标签(前端展示与审计共用)。
    pub fn label(self) -> &'static str {
        match self {
            Scope::FsRead => "文件只读",
            Scope::FsWrite => "文件写入",
            Scope::Exec => "命令执行",
            Scope::Net => "MCP 查询",
            Scope::Ui => "GUI 自动化",
        }
    }
}

/// 默认有效期 30 分钟。
pub const DEFAULT_TTL_SECS: i64 = 30 * 60;
/// 有效期上限 8 小时。
pub const MAX_TTL_SECS: i64 = 8 * 3600;
/// 有效期下限 1 分钟。
pub const MIN_TTL_SECS: i64 = 60;

/// 一个已签发的令牌。
#[derive(Debug, Clone, Serialize)]
pub struct Capability {
    pub id: String,
    pub label: String,
    pub scopes: Vec<Scope>,
    pub issued_at: i64,
    pub expires_at: i64,
    /// None = 不限次数
    pub max_uses: Option<u32>,
    pub uses: u32,
}

impl Capability {
    pub fn is_active(&self, now: i64) -> bool {
        self.expires_at > now && self.max_uses.is_none_or(|m| self.uses < m)
    }

    /// 剩余可用次数(None = 不限)。
    pub fn remaining(&self) -> Option<u32> {
        self.max_uses.map(|m| m.saturating_sub(self.uses))
    }

    fn covers(&self, scope: Scope, now: i64) -> bool {
        self.is_active(now) && self.scopes.contains(&scope)
    }
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// 全局令牌仓库(内存态,Clone 便于跨 task 传递)。
#[derive(Default, Clone)]
pub struct CapabilityStore {
    inner: Arc<Mutex<Vec<Capability>>>,
}

impl CapabilityStore {
    /// 签发令牌。ttl_secs 为 None 时用默认值;越界自动收敛到 [1min, 8h]。
    pub fn issue(
        &self,
        label: &str,
        scopes: Vec<Scope>,
        ttl_secs: Option<i64>,
        max_uses: Option<u32>,
    ) -> Result<Capability, String> {
        let mut scopes = scopes;
        scopes.sort_by_key(|s| *s as u8);
        scopes.dedup();
        if scopes.is_empty() {
            return Err("至少选择一个作用域".into());
        }
        let ttl = ttl_secs
            .unwrap_or(DEFAULT_TTL_SECS)
            .clamp(MIN_TTL_SECS, MAX_TTL_SECS);
        let now = now_secs();
        let cap = Capability {
            id: Uuid::new_v4().to_string(),
            label: label.trim().to_string(),
            scopes,
            issued_at: now,
            expires_at: now + ttl,
            // 0 次视为不限次数,避免签出「一次都用不了」的令牌
            max_uses: max_uses.filter(|m| *m > 0),
            uses: 0,
        };
        self.lock().push(cap.clone());
        Ok(cap)
    }

    /// 列出仍有效的令牌(顺带剔除已过期的)。
    pub fn list_active(&self) -> Vec<Capability> {
        let now = now_secs();
        let mut guard = self.lock();
        guard.retain(|c| c.expires_at > now);
        guard.clone()
    }

    /// 撤销令牌;返回是否真的删掉了。
    pub fn revoke(&self, id: &str) -> bool {
        let mut guard = self.lock();
        let before = guard.len();
        guard.retain(|c| c.id != id);
        guard.len() != before
    }

    /// 消费一次作用域授权:命中则 uses+1 并返回该令牌的副本。
    ///
    /// 优先使用「剩余次数更少」的令牌,先把快过期的额度用掉。
    pub fn consume(&self, scope: Scope) -> Option<Capability> {
        let now = now_secs();
        let mut guard = self.lock();
        guard.retain(|c| c.expires_at > now);
        let idx = guard
            .iter()
            .enumerate()
            .filter(|(_, c)| c.covers(scope, now))
            .min_by_key(|(_, c)| (c.expires_at, c.remaining().unwrap_or(u32::MAX)))
            .map(|(i, _)| i)?;
        guard[idx].uses += 1;
        Some(guard[idx].clone())
    }

    /// 清空全部令牌(关闭 GUI 自动化 / 退出会话时调用)。
    pub fn clear(&self) {
        self.lock().clear();
    }

    /// 当前有效令牌数(状态栏用)
    #[allow(dead_code)]
    pub fn active_count(&self) -> usize {
        self.list_active().len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Capability>> {
        // 内存仓库极简,锁中毒时直接恢复内部数据继续用,避免整个能力体系瘫痪
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_requires_scope_and_clamps_ttl() {
        let store = CapabilityStore::default();
        assert!(store.issue("空", vec![], None, None).is_err());

        let cap = store
            .issue(
                "测试",
                vec![Scope::FsRead, Scope::FsRead],
                Some(999_999),
                None,
            )
            .unwrap();
        assert_eq!(cap.scopes, vec![Scope::FsRead]);
        assert_eq!(cap.expires_at - cap.issued_at, MAX_TTL_SECS);

        // 0 次 → 视为不限次数
        assert_eq!(
            store
                .issue("a", vec![Scope::Exec], None, Some(0))
                .unwrap()
                .max_uses,
            None
        );
    }

    #[test]
    fn consume_respects_scope_and_max_uses() {
        let store = CapabilityStore::default();
        let cap = store.issue("a", vec![Scope::Exec], None, Some(2)).unwrap();

        assert!(
            store.consume(Scope::FsRead).is_none(),
            "作用域不匹配不应消费"
        );
        assert!(store.consume(Scope::Exec).is_some());
        assert!(store.consume(Scope::Exec).is_some());
        assert!(store.consume(Scope::Exec).is_none(), "次数耗尽后应拒绝");

        let listed = store.list_active();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, cap.id);
        assert_eq!(listed[0].remaining(), Some(0));
    }

    #[test]
    fn expired_and_revoked_tokens_are_dropped() {
        let store = CapabilityStore::default();
        let cap = store.issue("a", vec![Scope::Ui], None, None).unwrap();
        // 直接改过期时间模拟自然过期(同模块可访问私有字段)
        store.lock()[0].expires_at = now_secs() - 1;

        assert!(store.list_active().is_empty());
        assert!(store.consume(Scope::Ui).is_none());

        let cap2 = store.issue("b", vec![Scope::Ui], None, None).unwrap();
        assert_ne!(cap.id, cap2.id);
        assert!(store.revoke(&cap2.id));
        assert!(!store.revoke(&cap2.id));
        assert_eq!(store.active_count(), 0);
    }
}
