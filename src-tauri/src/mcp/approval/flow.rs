//! 待审批请求的 oneshot 通道与会话级「始终允许」缓存。

use super::rules::{assess_risk, reason_for_risk};
use super::{ApprovalRequest, ApprovalResult};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};
use uuid::Uuid;

/// 全局审批 store —— tauri::State 注入。内部字段 Arc 包装实现 Clone,
/// 方便跨 task 传递(chat_stream command → spawn 的后台任务)。
#[derive(Default, Clone)]
pub struct ApprovalStore {
    /// 待处理的 oneshot sender
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<ApprovalResult>>>>,
    /// "始终允许"缓存:key = format!("{server}::{tool}"),会话级,重启清空
    session_allowed: Arc<Mutex<HashSet<String>>>,
}

impl ApprovalStore {
    /// 检查是否已在"始终允许"集合里(会话级,应用退出清空)。
    pub async fn is_session_allowed(&self, server: &str, tool: &str) -> bool {
        let key = format!("{server}::{tool}");
        self.session_allowed.lock().await.contains(&key)
    }

    /// 把 {server, tool} 标记为会话内始终允许。
    pub async fn mark_session_allowed(&self, server: &str, tool: &str) {
        let key = format!("{server}::{tool}");
        self.session_allowed.lock().await.insert(key);
    }

    /// 会话结束时清空 —— 下一个会话干净开始。
    pub async fn clear_session(&self) {
        self.session_allowed.lock().await.clear();
        // 同时清 pending —— 会话退出时所有 pending 审批不再有意义
        self.pending.lock().await.clear();
    }

    /// 注册一个待审批请求 —— 返回 approval_id + oneshot receiver 给调用方。
    pub async fn register(
        &self,
        server: &str,
        tool: &str,
        args: serde_json::Value,
    ) -> (String, ApprovalRequest, oneshot::Receiver<ApprovalResult>) {
        let approval_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();

        // 粗略风险判定 —— Phase 6 白名单会做得更细
        let risk = assess_risk(server, tool, &args);
        let reason = reason_for_risk(server, &risk, tool);

        let req = ApprovalRequest {
            approval_id: approval_id.clone(),
            server_name: server.to_string(),
            tool_name: tool.to_string(),
            arguments: args,
            risk,
            reason,
        };

        self.pending.lock().await.insert(approval_id.clone(), tx);
        (approval_id, req, rx)
    }

    /// 前端返回决策 —— 从 HashMap 取出 sender.send(result)。
    pub async fn resolve(&self, approval_id: &str, result: ApprovalResult) -> bool {
        let mut pending = self.pending.lock().await;
        if let Some(tx) = pending.remove(approval_id) {
            // AllowAlways → 写入 session_allowed
            if matches!(result, ApprovalResult::AllowAlways) {
                let key_part: Vec<&str> = approval_id.split("::").collect();
                if key_part.len() >= 2 {
                    // approval_id 本身不带 server/tool,得从别的地方拿 —— 改架构:register 时存一份
                    // 简化处理:resolve 只发结果,session_allowed 由前端再单独调 set_session_allowed
                }
            }
            let _ = tx.send(result); // receiver 可能已 drop(超时),静默忽略
            true
        } else {
            false // 可能已超时
        }
    }

    /// 超时清理 —— ChatStream 启动时 spawn 一个 tokio task,每秒扫一遍 HashMap,>TIMEOUT_SECS 的丢弃。
    #[allow(dead_code)] // 预留给 Phase 5e ChatStream spawn task
    pub async fn cleanup_expired(&self) {
        // 简化:超时由 ChatStream 的 select! 处理,这里只是清 HashMap 里已 resolve 但 receiver 已 drop 的
        // oneshot sender 会被自动 drop,不需要显式清理
    }
}
