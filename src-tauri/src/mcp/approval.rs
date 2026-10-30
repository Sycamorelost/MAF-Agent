//! HITL 审批通道(Cline 43k⭐同款)。
//!
//! 架构:
//!   ChatStream 检测到 tool_call → emit Tauri event + 保存 oneshot::Sender 到 HashMap
//!   前端 ApprovalDialog 收到 event → 用户决策 → 调 mcp_resolve_approval(id, decision)
//!   resolve_approval 从 HashMap 取出 sender.send(result) → ChatStream 恢复
//!
//! key 设计:
//!   - approval_id 每次生成(UUID),防止审批重放
//!   - HashMap key 是 approval_id,value 是 oneshot sender
//!   - 默认 5 分钟超时(TIMEOUT_SECS),超时自动拒绝并写审计
//!   - "始终允许"缓存存在 McpRuntime 的 allowed_set 里,会话结束清空

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};
use uuid::Uuid;

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
    /// 逐次审批:每个工具调用都弹窗(最保守)
    #[serde(alias = "manual")]
    Ask,
    /// 工作区(默认,D11):只读与工作区内写自动放行;
    /// shell / GUI / 删除 / 越界写入仍审批。旧自定义白名单作为本档附加放行继续生效。
    #[default]
    #[serde(alias = "auto", alias = "custom")]
    Workspace,
    /// 完全不弹窗,直接执行。此模式下:shell 走 run_unrestricted(无白名单/路径/元字符校验),
    /// 写前备份仍生效;GUI 自动化与能力令牌等高级功能仅在此模式下可用(总开关)。
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
    pub fn requires_approval(
        &self,
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
                let key = format!("{server}::{tool}");
                if self.allow.iter().any(|p| p.trim() == key || p.trim() == tool) {
                    return false;
                }
                !workspace_write_auto_allowed(server, tool, args)
            }
        }
    }
}

/// 工作区档的写操作免审批判定(D11):仅限内置文本/文档/表格写工具,
/// 且 path 解析后落在工作区内;删除 / 备份恢复 / zip 解包 / shell 一律仍需审批。
fn workspace_write_auto_allowed(server: &str, tool: &str, args: &serde_json::Value) -> bool {
    if server != "builtin" || !matches!(tool, "fs_write" | "fs_export_docx" | "fs_export_xlsx") {
        return false;
    }
    let Some(path) = args.get("path").and_then(|v| v.as_str()) else {
        return false;
    };
    let Ok(root) = crate::fs::paths::workspace_root() else {
        return false;
    };
    in_workspace_at(&root, path)
}

/// 路径是否落在指定工作区内(相对路径按工作区解析,绝对路径直接判定;解析失败保守按不在)。
fn in_workspace_at(root: &std::path::Path, path: &str) -> bool {
    let Ok(abs) = crate::fs::paths::resolve(root, path, false) else {
        return false;
    };
    crate::fs::paths::rel_of(root, &abs).is_ok()
}

/// 策略文件路径。
pub fn policy_path(data_dir: &Path) -> PathBuf {
    data_dir.join("mcp-approval.json")
}

/// 读策略(文件缺失/损坏 → 工作区档,D11 默认)。
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
    std::fs::write(policy_path(data_dir), json).map_err(|e| format!("写入失败: {e}"))
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

/// 全局审批 store —— tauri::State 注入。内部字段 Arc 包装实现 Clone,
/// 方便跨 task 传递(chat_stream command → spawn 的后台任务)。
#[derive(Default, Clone)]
pub struct ApprovalStore {
    /// 待处理的 oneshot sender
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<ApprovalResult>>>>,
    /// "始终允许"缓存:key = format!("{server}::{tool}"),会话级,重启清空
    session_allowed: Arc<Mutex<HashSet<String>>>,
}

/// 300s 审批超时 —— 超时后自动 Deny 并从 HashMap 移除。
pub const APPROVAL_TIMEOUT_SECS: u64 = 300;

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
        let reason = reason_for_risk(&risk, tool);

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

/// 风险判定 —— 按工具名动词 + 敏感参数推断(不看 server 名,
/// 用户自定义的 server 名("项目文件")也要能判准)。自动审批模式据此决定是否放行。
pub(crate) fn assess_risk(_server: &str, tool: &str, args: &serde_json::Value) -> ApprovalRisk {
    // 1. 敏感路径参数直接高风险
    if let Some(obj) = args.as_object()
        && let Some(path) = obj.get("path").and_then(|v| v.as_str())
    {
        let p = path.to_ascii_lowercase();
        if p.contains(".env")
            || p.contains("credentials")
            || p.contains("secrets")
            || p.contains("id_rsa")
        {
            return ApprovalRisk::High;
        }
    }
    let t = tool.to_ascii_lowercase();
    // 2. 破坏性/执行类动词 → 高风险
    const DESTRUCTIVE: [&str; 7] = [
        "delete",
        "write",
        "edit",
        "patch",
        "move",
        "shell_exec",
        "execute",
    ];
    if DESTRUCTIVE.iter().any(|k| t.contains(k)) {
        return ApprovalRisk::High;
    }
    // 3. 纯思考类(无副作用)
    if t == "sequentialthinking" {
        return ApprovalRisk::Low;
    }
    // 4. 只读动词 → 低风险(自动审批直接放行)
    const READONLY: [&str; 9] = [
        "read", "list", "search", "get", "open", "query", "retrieve", "tree", "grep",
    ];
    if READONLY.iter().any(|k| t.starts_with(k) || t.contains(k)) {
        return ApprovalRisk::Low;
    }
    // 5. 其余(create/add/copy… 有副作用但非破坏性)→ 中风险
    ApprovalRisk::Medium
}

fn reason_for_risk(risk: &ApprovalRisk, tool: &str) -> String {
    match risk {
        ApprovalRisk::High => format!("工具 `{tool}` 涉及高风险操作,需要您的确认"),
        ApprovalRisk::Medium => format!("工具 `{tool}` 涉及文件/命令操作,建议检查参数后放行"),
        ApprovalRisk::Low => format!("工具 `{tool}` 为只读查询类操作"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn risk_distinguishes_readonly_from_write() {
        assert_eq!(
            assess_risk("x", "read_text_file", &json!({})),
            ApprovalRisk::Low
        );
        assert_eq!(
            assess_risk("x", "search_nodes", &json!({})),
            ApprovalRisk::Low
        );
        assert_eq!(
            assess_risk("x", "sequentialthinking", &json!({})),
            ApprovalRisk::Low
        );
        assert_eq!(
            assess_risk("x", "write_file", &json!({})),
            ApprovalRisk::High
        );
        assert_eq!(
            assess_risk("x", "delete_entities", &json!({})),
            ApprovalRisk::High
        );
        assert_eq!(
            assess_risk("x", "create_entities", &json!({})),
            ApprovalRisk::Medium
        );
        // 敏感路径:即便只读工具也按高风险处理
        assert_eq!(
            assess_risk("x", "read_text_file", &json!({ "path": "/a/.env" })),
            ApprovalRisk::High
        );
    }

    #[test]
    fn policy_modes_gate_approval() {
        let ask = ApprovalPolicy {
            mode: ApprovalMode::Ask,
            allow: vec![],
        };
        assert!(ask.requires_approval("s", "read_text_file", ApprovalRisk::Low, &json!({})));

        let ws = ApprovalPolicy {
            mode: ApprovalMode::Workspace,
            allow: vec![],
        };
        // 只读免打扰
        assert!(!ws.requires_approval("s", "read_text_file", ApprovalRisk::Low, &json!({})));
        // shell / 删除 / 外部写 一律审批(workspace_write_auto_allowed 对非白名单工具直接 false,
        // 不触碰文件系统,可在测试中安全调用)
        assert!(
            ws.requires_approval("builtin", "shell_exec", ApprovalRisk::High, &json!({"command":"ls"}))
        );
        assert!(
            ws.requires_approval("builtin", "fs_delete", ApprovalRisk::High, &json!({"path":"a.txt"}))
        );
        assert!(
            ws.requires_approval("mcp", "write_file", ApprovalRisk::High, &json!({"path":"a.txt"}))
        );

        let full = ApprovalPolicy {
            mode: ApprovalMode::Full,
            allow: vec![],
        };
        assert!(!full.requires_approval("s", "shell_exec", ApprovalRisk::High, &json!({})));

        // 旧自定义白名单 → workspace 档附加放行
        let ws_allow = ApprovalPolicy {
            mode: ApprovalMode::Workspace,
            allow: vec!["项目文件::read_text_file".into(), "search_nodes".into()],
        };
        assert!(
            !ws_allow.requires_approval("项目文件", "read_text_file", ApprovalRisk::Low, &json!({}))
        );
        assert!(!ws_allow.requires_approval("长期记忆", "search_nodes", ApprovalRisk::Low, &json!({})));
        assert!(
            ws_allow.requires_approval("长期记忆", "create_entities", ApprovalRisk::Medium, &json!({}))
        );
    }

    #[test]
    fn workspace_write_boundary_check() {
        // 纯函数判定:相对路径落在工作区内,绝对越界路径不在
        let root = std::env::temp_dir().join(format!("maf-ws-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        assert!(in_workspace_at(&root, "out/a.txt"));
        assert!(in_workspace_at(&root, "sub/../a.txt"));
        let outside = std::env::temp_dir().join(format!("maf-out-{}", uuid::Uuid::new_v4()));
        assert!(!in_workspace_at(&root, outside.to_str().unwrap()));
        // 越过工作区的相对路径(.. 跳不出去时才算在内;跳出根目录即判不在)
        assert!(!in_workspace_at(&root, "../../../../etc/passwd"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn policy_json_roundtrip_and_legacy_alias() {
        let p: ApprovalPolicy =
            serde_json::from_str(r#"{"mode":"full","allow":["a::b"]}"#).unwrap();
        assert_eq!(p.mode, ApprovalMode::Full);
        assert_eq!(p.allow, vec!["a::b".to_string()]);
        // 旧值别名迁移:manual→ask,auto/custom→workspace;full 保持 full(绝不降级)
        let m: ApprovalPolicy = serde_json::from_str(r#"{"mode":"manual"}"#).unwrap();
        assert_eq!(m.mode, ApprovalMode::Ask);
        let a: ApprovalPolicy = serde_json::from_str(r#"{"mode":"auto"}"#).unwrap();
        assert_eq!(a.mode, ApprovalMode::Workspace);
        let c: ApprovalPolicy = serde_json::from_str(r#"{"mode":"custom","allow":["x"]}"#).unwrap();
        assert_eq!(c.mode, ApprovalMode::Workspace);
        assert_eq!(c.allow, vec!["x".to_string()]);
        // 缺失/损坏 → 工作区档(D11 默认),不缺省直放也不全弹窗
        let d: ApprovalPolicy = serde_json::from_str("{}").unwrap();
        assert_eq!(d.mode, ApprovalMode::Workspace);
    }
}
