//! HITL 审批通道(Cline 43k GitHub stars 同款)。
//!
//! 架构:
//!   ChatStream 检测到 tool_call → emit Tauri event + 保存 oneshot::Sender 到 HashMap
//!   前端 ApprovalDialog 收到 event → 用户决策 → 调 mcp_resolve_approval(id, decision)
//!   resolve_approval 从 HashMap 取出 sender.send(result) → ChatStream 恢复
//!
//! 分层:
//! - 本文件:审批数据结构、策略持久化与「是否需要审批」判定;
//! - `rules.rs`:风险判定 / 敏感路径 / 工作区边界等纯函数;
//! - `flow.rs`:待审批请求的 oneshot 通道(`ApprovalStore`)。
//!
//! key 设计:
//!   - approval_id 每次生成(UUID),防止审批重放
//!   - HashMap key 是 approval_id,value 是 oneshot sender
//!   - 默认 5 分钟超时(TIMEOUT_SECS),超时自动拒绝并写审计
//!   - "始终允许"缓存存在 McpRuntime 的 allowed_set 里,会话结束清空

mod flow;
mod rules;

pub use flow::ApprovalStore;
pub(crate) use rules::assess_risk;

use rules::{sensitive_path, workspace_write_auto_allowed_in};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 审批请求(emit 给前端的完整 payload)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub approval_id: String,
    pub server_name: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    /// 风险标签:高/中/低 —— 按工具白名单或敏感参数自动判定
    pub risk: ApprovalRisk,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalRisk {
    Low,
    Medium,
    High,
}

/// 审批模式 —— 决定模型调工具时是否弹窗征求同意(D11 三档收敛)。
///
/// 旧四档映射(serde alias 自动迁移,旧 mcp-approval.json 无需手改):
///   manual(逐次)→ ask;auto(只读自动)→ workspace;custom(白名单)→ workspace(白名单保留);
///   full(完全信任)→ full,绝不降级(val.md 风险表)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    /// 逐次审批(默认):每个工具调用都弹窗(最保守)
    #[default]
    #[serde(alias = "manual")]
    Ask,
    /// 工作区:只读与工作区内写自动放行;
    /// shell / GUI / 删除 / 越界写入仍审批。旧自定义白名单作为本档附加放行继续生效。
    #[serde(alias = "auto", alias = "custom")]
    Workspace,
    /// 完全信任:不再弹窗,shell 直接执行(写前备份仍生效);GUI 自动化仅在此模式下可用。
    Full,
}

/// 审批策略 —— 持久化在 data/mcp-approval.json。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApprovalPolicy {
    #[serde(default)]
    pub mode: ApprovalMode,
    /// 自定义模式:自动放行的工具,支持 `server::tool` 或只写 `tool`
    #[serde(default)]
    pub allow: Vec<String>,
}

impl ApprovalPolicy {
    /// 是否需要弹窗审批(false = 后台直接放行)。
    ///
    /// 无会话上下文时按全局默认工作区判定;能拿到 session_id 的调用点必须改用
    /// [`ApprovalPolicy::requires_approval_in`],否则项目绑定后「项目内写」仍会误弹审批。
    #[allow(dead_code)]
    pub fn requires_approval(
        &self,
        server: &str,
        tool: &str,
        risk: ApprovalRisk,
        args: &serde_json::Value,
    ) -> bool {
        self.requires_approval_in(None, server, tool, risk, args)
    }

    /// 会话感知版:工作区档下,「项目内写」按会话绑定的项目目录判定。
    pub fn requires_approval_in(
        &self,
        session_id: Option<&str>,
        server: &str,
        tool: &str,
        risk: ApprovalRisk,
        args: &serde_json::Value,
    ) -> bool {
        match self.mode {
            ApprovalMode::Ask => true,
            ApprovalMode::Full => false,
            // 「工作区」:只读(低风险)不打扰;白名单附加放行;
            // 内置 fs 写工具且目标在工作区内自动放行,其余(shell/删除/越界写/外部写)仍审批
            ApprovalMode::Workspace => {
                if risk == ApprovalRisk::Low {
                    return false;
                }
                // P7:凭据类文件名即便落在工作区内也强制审批 —— 自动放行最贵的误操作
                // 就是把 .env / 私钥写坏或覆盖;白名单也不能豁免这一条。
                if sensitive_path(args) {
                    return true;
                }
                let key = format!("{server}::{tool}");
                if self
                    .allow
                    .iter()
                    .any(|p| p.trim() == key || p.trim() == tool)
                {
                    return false;
                }
                !workspace_write_auto_allowed_in(session_id, server, tool, args)
            }
        }
    }
}

/// 策略文件路径。
pub fn policy_path(data_dir: &Path) -> PathBuf {
    data_dir.join("mcp-approval.json")
}

/// 读策略(文件缺失/损坏 → 逐次审批档,默认)。
pub fn load_policy(data_dir: &Path) -> ApprovalPolicy {
    std::fs::read_to_string(policy_path(data_dir))
        .ok()
        // 容忍手改文件带入的 UTF-8 BOM:serde_json 遇 BOM 直接解析失败,
        // 会静默回落到默认档(表现为「完全信任不生效」),此处先剥掉再解析
        .and_then(|s| serde_json::from_str::<ApprovalPolicy>(s.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default()
}

/// 写策略(前端设置页保存)。
pub fn save_policy(data_dir: &Path, policy: &ApprovalPolicy) -> Result<(), String> {
    let json = serde_json::to_string_pretty(policy).map_err(|e| format!("序列化失败: {e}"))?;
    std::fs::write(policy_path(data_dir), json).map_err(|e| format!("写入失败: {e}"))?;
    Ok(())
}

/// 实际生效的审批档:审批档是全局永久设置(写入 mcp-approval.json,所有会话与环境一致,
/// 重启后仍保持)。会话级覆盖由调用方在传入前替换 `policy.mode`,此处直接返回该档位。
pub fn effective_mode(
    policy: &ApprovalPolicy,
    _data_dir: &Path,
    _session_id: Option<&str>,
) -> ApprovalMode {
    policy.mode
}

/// 审批结果(前端回传给后端的决策)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum ApprovalResult {
    /// 允许本次调用
    Allow,
    /// 允许本次会话中该 tool 不再问
    AllowAlways,
    /// 允许本次调用,但要先修改参数
    AllowEdited { arguments: serde_json::Value },
    /// 拒绝
    Deny,
}

/// 300s 审批超时 —— 超时后自动 Deny 并从 HashMap 移除。
pub const APPROVAL_TIMEOUT_SECS: u64 = 300;

#[cfg(test)]
mod tests;
