//! Phase 5e: ChatStream → MCP 工具调用调度器。
//!
//! 职责:
//! 1. 接收模型返回的 tool_calls 列表 + 本轮累积的 assistant 文本
//! 2. 按工具名反查所属 server,逐个走 HITL 审批通道(ApprovalStore)
//! 3. 调用 McpRuntime 执行工具
//! 4. 产出 assistant(带 tool_calls)+ N 条 tool(带 tool_call_id)消息,供下一轮请求模型
//!
//! 不持有状态:所有依赖通过参数传入(McpRuntime/ApprovalStore/data_dir)。

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use tauri::Emitter;
use tauri::ipc::Channel;

use crate::provider::{ChatMessage, StreamChunk, ToolCall};

use super::approval::{APPROVAL_TIMEOUT_SECS, ApprovalResult, ApprovalStore};
use super::capability::CapabilityStore;
use super::runtime::{McpRuntime, McpToolCall};

/// 单轮工具调用结果 —— 供 ChatStream 循环决定是否继续下一轮。
#[derive(Debug)]
pub struct ToolDispatchOutcome {
    /// 追加到 req.messages 的新消息(assistant + N 条 tool)
    pub new_messages: Vec<ChatMessage>,
    /// 是否全部工具执行成功
    pub all_ok: bool,
    /// 是否被用户拒绝过(仍有 tool 消息回填,模型可据此改写答案)
    pub denied: bool,
}

/// 执行一轮 tool_calls:审批 → 反查 server → 调用 → 重组消息。
// 参数为审批链各独立依赖(app/令牌/审批/运行时/数据目录/通道),均为必需上下文
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_tool_calls(
    app: &tauri::AppHandle,
    tool_calls: Vec<ToolCall>,
    assistant_content: String,
    runtime: &McpRuntime,
    approval: &ApprovalStore,
    capability: &CapabilityStore,
    data_dir: PathBuf,
    channel: &Channel<StreamChunk>,
    session_id: Option<&str>,
    sub_llm: Option<&crate::provider::SubLlm>,
) -> ToolDispatchOutcome {
    // assistant 消息(带 tool_calls)必须留在历史里,否则模型下一轮无法关联 tool 结果
    let assistant_msg = ChatMessage {
        role: "assistant".into(),
        content: assistant_content,
        tool_calls: Some(tool_calls.clone()),
        ..Default::default()
    };

    let mut tool_messages: Vec<ChatMessage> = Vec::with_capacity(tool_calls.len());
    let mut all_ok = true;
    let mut denied = false;

    for tc in &tool_calls {
        let tool_name = tc.function.name.clone();
        // arguments 是 JSON 字符串;空串/半截 JSON 兜底为 {}
        let args: Value =
            serde_json::from_str(&tc.function.arguments).unwrap_or_else(|_| json!({}));

        // 0. 内置工具(Phase 6):时间 / 白名单 shell —— 不走 MCP,但 shell 仍需审批
        if is_builtin_tool(&tool_name) {
            let need_approval = builtin_needs_approval(&tool_name);
            let exec_args = if need_approval {
                match ask_approval(
                    app,
                    approval,
                    capability,
                    &data_dir,
                    "builtin",
                    &tool_name,
                    args.clone(),
                    session_id,
                )
                .await
                {
                    Some(a) => a,
                    None => {
                        all_ok = false;
                        denied = true;
                        tool_messages.push(tool_message(
                            &tc.id,
                            format!("用户拒绝执行内置命令 `{tool_name}`。"),
                        ));
                        continue;
                    }
                }
            } else {
                args.clone()
            };
            // 审批通过后才真正执行(顺序不可颠倒:shell_exec 有副作用);
            // shell_exec 的放开白名单只看全局「完全信任」,会话级 full 覆盖不解锁 shell/GUI
            // (避免单会话静默提权系统级能力;会话覆盖只管「问不问」)。
            let trust_shell =
                crate::mcp::approval::load_policy(&data_dir).mode == crate::mcp::ApprovalMode::Full;
            let started = std::time::Instant::now();
            let (content, ok) =
                execute_builtin(&tool_name, &exec_args, trust_shell, session_id, sub_llm).await;
            let elapsed_ms = Some(started.elapsed().as_millis() as u64);
            let arguments = args_summary(&exec_args);
            // 任务清单变更 → 通知前端刷新对应会话的 todo 面板
            if ok
                && tool_name == "todo_write"
                && let Some(sid) = session_id
            {
                let _ = app.emit(&format!("session-todos-{sid}"), ());
            }
            let _ = channel.send(StreamChunk::ToolCallResult {
                tool_call_id: tc.id.clone(),
                tool_name: tool_name.clone(),
                server_name: "builtin".into(),
                content: content.clone(),
                ok,
                arguments,
                elapsed_ms,
            });
            if !ok {
                all_ok = false;
            }
            tool_messages.push(tool_message(&tc.id, content));
            continue;
        }

        // 1. 反查 server
        let server = match runtime.find_server_for_tool(&tool_name).await {
            Some(s) => s,
            None => {
                all_ok = false;
                let msg = format!("未找到提供工具 `{tool_name}` 的已启用 MCP server,调用已跳过。");
                let _ = channel.send(StreamChunk::ToolCallResult {
                    tool_call_id: tc.id.clone(),
                    tool_name: tool_name.clone(),
                    server_name: "(unknown)".into(),
                    content: msg.clone(),
                    ok: false,
                    arguments: None,
                    elapsed_ms: None,
                });
                tool_messages.push(tool_message(&tc.id, msg));
                continue;
            }
        };

        // 2. 审批(会话内已放行的工具直接跳过弹窗);AllowEdited 用用户改后的参数
        let exec_args = match ask_approval(
            app, approval, capability, &data_dir, &server, &tool_name, args, session_id,
        )
        .await
        {
            Some(a) => a,
            None => {
                all_ok = false;
                denied = true;
                let msg = format!(
                    "用户拒绝执行工具 `{tool_name}`(call_id={})。请改用其他方式回答,不要重复调用。",
                    tc.id
                );
                let _ = channel.send(StreamChunk::ToolCallResult {
                    tool_call_id: tc.id.clone(),
                    tool_name: tool_name.clone(),
                    server_name: server.clone(),
                    content: msg.clone(),
                    ok: false,
                    arguments: None,
                    elapsed_ms: None,
                });
                tool_messages.push(tool_message(&tc.id, msg));
                continue;
            }
        };

        // 3.5 安全预检(M-2):让 allowed_dirs 真正生效;
        // 预检与启动参数同源;未配置目录时预检不限制位置(不做文件夹授权)
        let cfg = crate::mcp::load_or_init(&data_dir);
        if let Some(sc) = cfg.servers.iter().find(|s| s.name == server)
            && let Err(e) = crate::mcp::safety::validate_tool_paths(
                &exec_args,
                &crate::mcp::config::effective_allowed_dirs(sc),
            )
        {
            all_ok = false;
            let msg = format!("工具调用被安全策略拒绝:{e}");
            let _ = channel.send(StreamChunk::ToolCallResult {
                tool_call_id: tc.id.clone(),
                tool_name: tool_name.clone(),
                server_name: server.clone(),
                content: msg.clone(),
                ok: false,
                arguments: None,
                elapsed_ms: None,
            });
            tool_messages.push(tool_message(&tc.id, msg));
            continue;
        }

        // 4. 执行
        let started = std::time::Instant::now();
        let result = runtime
            .call_tool(
                McpToolCall {
                    server_name: server.clone(),
                    tool_name: tool_name.clone(),
                    arguments: exec_args.clone(),
                },
                data_dir.clone(),
            )
            .await;
        let elapsed_ms = Some(started.elapsed().as_millis() as u64);
        let arguments = args_summary(&exec_args);

        let content = if result.ok {
            result.content
        } else {
            format!(
                "工具执行失败:{}",
                result.error.unwrap_or_else(|| "未知错误".into())
            )
        };
        if !result.ok {
            all_ok = false;
        }

        // 5. 结果推前端(UI 展示工具卡片)
        let _ = channel.send(StreamChunk::ToolCallResult {
            tool_call_id: tc.id.clone(),
            tool_name: tool_name.clone(),
            server_name: server,
            content: content.clone(),
            ok: result.ok,
            arguments,
            elapsed_ms,
        });

        // 6. 回填 tool 消息
        tool_messages.push(tool_message(&tc.id, content));
    }

    let mut new_messages = vec![assistant_msg];
    new_messages.extend(tool_messages);
    ToolDispatchOutcome {
        new_messages,
        all_ok,
        denied,
    }
}

/// 工具流水(P6):实际执行参数的紧凑摘要,截断到 500 字符,避免撑爆 maf-tool 块。
fn args_summary(args: &Value) -> Option<String> {
    let s = serde_json::to_string(args).unwrap_or_default();
    if s.is_empty() || s == "{}" || s == "null" {
        return None;
    }
    Some(s.chars().take(500).collect())
}

fn tool_message(tool_call_id: &str, content: String) -> ChatMessage {
    ChatMessage {
        role: "tool".into(),
        content,
        tool_call_id: Some(tool_call_id.to_string()),
        ..Default::default()
    }
}

/// 走一次 HITL 审批。返回 Some(实际执行参数) 表示放行,None 表示拒绝/超时/通道不可用。
/// 以下四种情况直接放行(「解放双手」):
///   1. 会话内已点过「始终允许」
///   2. 命中有效的能力令牌(且作用域覆盖该工具)
///   3. 审批档为「完全信任」(全局或会话级覆盖,D11)
///   4. 「工作区」档下判定为只读 / 工作区内写,或命中附加白名单
async fn ask_approval(
    app: &tauri::AppHandle,
    approval: &ApprovalStore,
    capability: &CapabilityStore,
    data_dir: &std::path::Path,
    server: &str,
    tool: &str,
    args: Value,
    session_id: Option<&str>,
) -> Option<Value> {
    if approval.is_session_allowed(server, tool).await {
        return Some(args);
    }
    let mut policy = super::approval::load_policy(data_dir);
    // 会话级权限档覆盖(D11/V25):非 inherit 时替换全局档位;
    // allow 白名单沿用全局配置(它只在 workspace 档生效,与档位正交)。
    if let Some(sid) = session_id
        && let Some(mode) = crate::db::sessions::permission_mode_override(sid)
    {
        policy.mode = match mode.as_str() {
            "ask" => super::approval::ApprovalMode::Ask,
            "full" => super::approval::ApprovalMode::Full,
            _ => super::approval::ApprovalMode::Workspace,
        };
    }
    let risk = super::approval::assess_risk(server, tool, &args);
    // 能力令牌:已并入「完全信任」总开关 —— 仅完全信任模式下才消费令牌,
    // 避免 full 期间签发的残留令牌在切回逐次/工作区档后继续免审批
    if policy.mode == super::approval::ApprovalMode::Full
        && let Some(cap) = capability.consume(capability_scope(server, tool, risk))
    {
        crate::mcp::audit(
            data_dir,
            "capability_use",
            server,
            tool,
            &format!(
                "cap={} label={} scope={} remaining={:?}",
                cap.id,
                cap.label,
                capability_scope(server, tool, risk).label(),
                cap.remaining()
            ),
        );
        return Some(args);
    }
    if !policy.requires_approval(server, tool, risk, &args) {
        crate::mcp::audit(
            data_dir,
            "approval_skip",
            server,
            tool,
            &format!("mode={:?} risk={:?}", policy.mode, risk),
        );
        return Some(args);
    }
    let (_id, req, rx) = approval.register(server, tool, args.clone()).await;
    // 通知前端弹审批框;emit 失败(窗口已关)按拒绝处理,避免无审批执行
    if app.emit("mcp-approval-request", &req).is_err() {
        return None;
    }
    match tokio::time::timeout(Duration::from_secs(APPROVAL_TIMEOUT_SECS), rx).await {
        Ok(Ok(ApprovalResult::Allow)) => Some(args),
        // 用户改过参数 → 必须用改后的参数执行,否则改参数毫无意义
        Ok(Ok(ApprovalResult::AllowEdited { arguments })) => Some(arguments),
        Ok(Ok(ApprovalResult::AllowAlways)) => {
            approval.mark_session_allowed(server, tool).await;
            Some(args)
        }
        Ok(Ok(ApprovalResult::Deny)) | Ok(Err(_)) | Err(_) => None,
    }
}

// 内置工具路由(作用域映射 / 白名单 / 执行)已拆到 builtin.rs,此处仅做转发
pub(crate) use super::builtin::{
    builtin_needs_approval, capability_scope, execute_builtin, is_builtin_tool,
};
