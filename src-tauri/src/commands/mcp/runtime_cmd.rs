//! MCP 运行时相关命令:启动/停止 server、列工具、带审批的工具调用、白名单 shell。

use super::McpState;
use crate::mcp::{
    self, ApprovalResult, ApprovalStore, McpRuntime, McpServerConfig, McpToolCall,
};
use crate::paths::data_dir as data_dir_fn;
use tauri::{AppHandle, Emitter, State};

/// 启动一个 MCP server —— 参数一律取自已保存配置(stdio 或 http 由 transport 决定)。
#[tauri::command]
pub async fn mcp_start_server(
    name: String,
    state: State<'_, McpState>,
    runtime: State<'_, McpRuntime>,
) -> Result<(), String> {
    let cfg = state
        .0
        .lock()
        .map_err(|e| format!("lock 失败: {e}"))?
        .servers
        .iter()
        .find(|s| s.name == name)
        .cloned()
        .ok_or_else(|| format!("server `{name}` 不存在"))?;
    runtime.start_from_config(&cfg, &data_dir_fn(), true).await
}

/// 一键启动所有 enabled 的 MCP server。返回 (成功数, 失败数, 错误信息列表)。
#[tauri::command]
pub async fn mcp_start_all_enabled(
    state: State<'_, McpState>,
    runtime: State<'_, McpRuntime>,
) -> Result<(usize, usize, Vec<String>), String> {
    let servers: Vec<McpServerConfig> = state
        .0
        .lock()
        .map_err(|e| format!("lock 失败: {e}"))?
        .servers
        .iter()
        .filter(|s| s.enabled)
        .cloned()
        .collect();
    let dir = data_dir_fn();
    let mut ok = 0;
    let mut fail = 0;
    let mut errors = Vec::new();
    for server in servers {
        match runtime.start_from_config(&server, &dir, true).await {
            Ok(()) => ok += 1,
            Err(e) => {
                fail += 1;
                errors.push(format!("{}: {e}", server.name));
            }
        }
    }
    Ok((ok, fail, errors))
}

/// 列出指定 server 的工具(需已 start)。
#[tauri::command]
pub async fn mcp_list_tools(
    name: String,
    runtime: State<'_, McpRuntime>,
) -> Result<Vec<mcp::McpToolInfo>, String> {
    runtime.list_tools(&name, false).await
}

/// 带 HITL 审批的工具调用 —— ChatStream 调用链:
///   1. 检查 session_allowed(会话内已标记的"始终允许") → 跳过审批
///   2. 检查能力令牌(命中作用域即放行,与 dispatcher 主链路一致)
///   3. register → emit Tauri event → await oneshot
///   4. 收到 Allow → runtime.call_tool;收到 Deny → 返回 error
#[tauri::command]
pub async fn mcp_call_tool_with_approval(
    call: McpToolCall,
    app: AppHandle,
    runtime: State<'_, McpRuntime>,
    approvals: State<'_, ApprovalStore>,
    capability: State<'_, mcp::CapabilityStore>,
) -> Result<mcp::McpToolResult, String> {
    // Phase 5e:ChatStream 真正走这里;Phase 5d 前端可以直接调测试
    let dir = data_dir_fn();

    // 快速路径:会话内已允许 → 直接调
    if approvals
        .is_session_allowed(&call.server_name, &call.tool_name)
        .await
    {
        return Ok(runtime.call_tool(call, dir).await);
    }

    // 完全信任:跳过能力令牌和审批弹窗,直接调用(只保留审计)
    let policy = mcp::approval::load_policy(&dir);
    if policy.mode == mcp::ApprovalMode::Full {
        mcp::audit(
            &dir,
            "full_trust_call",
            &call.server_name,
            &call.tool_name,
            "mode=full;跳过审批直接执行",
        );
        return Ok(runtime.call_tool(call, dir).await);
    }

    // 能力令牌(仅非完全信任模式消费;Full 模式已在上方提前返回)
    let risk = mcp::approval::assess_risk(&call.server_name, &call.tool_name, &call.arguments);
    let scope = mcp::builtin::capability_scope(&call.server_name, &call.tool_name, risk);
    if let Some(cap) = capability.consume(scope) {
        mcp::audit(
            &dir,
            "capability_use",
            &call.server_name,
            &call.tool_name,
            &format!(
                "cap={} label={} scope={} remaining={:?}",
                cap.id,
                cap.label,
                scope.label(),
                cap.remaining()
            ),
        );
        return Ok(runtime.call_tool(call, dir).await);
    }

    // register + emit event
    let (approval_id, req, rx) = approvals
        .register(&call.server_name, &call.tool_name, call.arguments.clone())
        .await;

    if let Err(e) = app.emit("mcp-approval-request", &req) {
        // emit 失败不阻塞 —— 继续走,等前端
        log::warn!("[MCP] emit approval event 失败: {e}");
    }

    // 等待前端决策,或 5 分钟超时 → Deny
    use tokio::time::{Duration, timeout};
    let decision = match timeout(Duration::from_secs(mcp::APPROVAL_TIMEOUT_SECS), rx).await {
        Ok(Ok(d)) => d,
        Ok(Err(_)) => return Err(format!("审批 channel 异常(server=`{}`)", call.server_name)),
        Err(_) => {
            // 超时:前端没响应,自动 Deny
            let _ = approvals.resolve(&approval_id, ApprovalResult::Deny).await;
            mcp::audit(
                &dir,
                "approval_timeout",
                &call.server_name,
                &call.tool_name,
                "timeout",
            );
            return Err("审批超时:前端 5 分钟内未响应,已自动拒绝".into());
        }
    };

    match decision {
        ApprovalResult::Deny => {
            mcp::audit(
                &dir,
                "approval_deny",
                &call.server_name,
                &call.tool_name,
                "user_deny",
            );
            Err(format!(
                "用户拒绝了 `{}::{}`",
                call.server_name, call.tool_name
            ))
        }
        ApprovalResult::AllowAlways => {
            approvals
                .mark_session_allowed(&call.server_name, &call.tool_name)
                .await;
            mcp::audit(
                &dir,
                "approval_allow_always",
                &call.server_name,
                &call.tool_name,
                "",
            );
            Ok(runtime.call_tool(call, dir).await)
        }
        ApprovalResult::Allow => {
            mcp::audit(
                &dir,
                "approval_allow",
                &call.server_name,
                &call.tool_name,
                "",
            );
            Ok(runtime.call_tool(call, dir).await)
        }
        ApprovalResult::AllowEdited { arguments } => {
            mcp::audit(
                &dir,
                "approval_allow_edited",
                &call.server_name,
                &call.tool_name,
                "",
            );
            let mut edited_call = call;
            edited_call.arguments = arguments;
            Ok(runtime.call_tool(edited_call, dir).await)
        }
    }
}

/// 前端回传审批决策 —— McpApprovalDialog 点 [允许/拒绝] 后调。
#[tauri::command]
pub async fn mcp_resolve_approval(
    approval_id: String,
    result: ApprovalResult,
    approvals: State<'_, ApprovalStore>,
) -> Result<bool, String> {
    let ok = approvals.resolve(&approval_id, result).await;
    Ok(ok)
}

/// 会话结束时清空 session_allowed —— Phase 5e ChatStream 结束回调调。
#[tauri::command]
pub async fn mcp_clear_session_allowed(approvals: State<'_, ApprovalStore>) -> Result<(), String> {
    approvals.clear_session().await;
    Ok(())
}

// ============ Phase 6:白名单 Shell 命令(执行内核见 mcp::shell) ============

#[tauri::command]
pub async fn shell_whitelist_exec(
    cmd: String,
    args: Vec<String>,
    cwd: Option<String>,
    confirm_danger: Option<bool>,
) -> Result<String, String> {
    let data_dir = crate::paths::data_dir();
    let cwd = cwd.as_deref();
    let policy = crate::mcp::approval::load_policy(&data_dir);
    let full = crate::mcp::approval::effective_mode(&policy, &data_dir, None)
        == crate::mcp::ApprovalMode::Full;
    if full {
        if let Some(pattern) = crate::mcp::shell::dangerous_pattern(&cmd, &args)
            && !confirm_danger.unwrap_or(false)
        {
            return Err(format!(
                "DANGEROUS_CONFIRM:检测到高风险命令 `{}`(格式化/删数据/改系统等不可逆操作),\
请确认后再执行。",
                pattern.trim()
            ));
        }
        // 该命令由设置页直接触发,无会话上下文 → 走全局默认工作区根
        crate::mcp::shell::run_unrestricted(&cmd, &args, cwd, None).await
    } else {
        crate::mcp::shell::run(&cmd, &args, cwd, None).await
    }
    .map(|o| o.text)
}
