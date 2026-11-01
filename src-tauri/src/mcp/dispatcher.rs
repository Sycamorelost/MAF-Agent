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
        // arguments 是 JSON 字符串;空串兜底为 {}。非空但不是合法 JSON 时
        // 通常是模型单次输出被截断(大段文件内容),此时绝不能按空参数执行,
        // 否则只会得到一个误导性的「缺少 path 参数」。
        let raw_args = tc.function.arguments.trim();
        let args: Value = match serde_json::from_str::<Value>(raw_args) {
            Ok(v) if v.is_object() => v,
            Ok(_) => json!({}),
            Err(_) if raw_args.is_empty() => json!({}),
            Err(e) => {
                all_ok = false;
                let msg = format!(
                    "工具 `{tool_name}` 的参数不是合法 JSON({e});通常是单次输出被截断。\
                     请缩小单次参数,写大文件改用 fs_write_begin → fs_write_chunk(多次)→ fs_write_finish 分批写入后重试。"
                );
                let _ = channel.send(StreamChunk::ToolCallResult {
                    tool_call_id: tc.id.clone(),
                    tool_name: tool_name.clone(),
                    server_name: "builtin".into(),
                    content: msg.clone(),
                    ok: false,
                    arguments: None,
                    elapsed_ms: None,
                    log_id: None,
                });
                tool_messages.push(tool_message(&tc.id, msg));
                continue;
            }
        };

        // 0. 内置工具(Phase 6):时间 / 白名单 shell —— 不走 MCP,但 shell 仍需审批
        if is_builtin_tool(&tool_name) {
            // D3:写入/执行类一律审批;只读工具若指向工作区外本机文件同样要问一次
            let write_like = builtin_needs_approval(&tool_name);
            let forced = !write_like && reads_outside_workspace(&tool_name, &args, session_id);
            let need_approval = write_like || forced;
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
                    forced,
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
            // P8:shell 放开白名单需要「完全信任」且持有有效的临时授权(全局或本会话),
            // 仅有档位声明而授权已过期时按工作区档执行(白名单校验照常生效)。
            let trust_shell = crate::mcp::approval::effective_mode(
                &crate::mcp::approval::load_policy(&data_dir),
                &data_dir,
                session_id,
            ) == crate::mcp::ApprovalMode::Full;
            let started = std::time::Instant::now();
            let (content, ok) =
                execute_builtin(&tool_name, &exec_args, trust_shell, session_id, sub_llm).await;
            let elapsed_ms = Some(started.elapsed().as_millis() as u64);
            let arguments = args_summary(&exec_args);
            // P4:统一记录审计日志(全文),并把行 id 回传前端供「查看完整输出」
            let log_id = record_tool_log(session_id, &tool_name, &exec_args, &content);
            // 任务清单变更 → 通知前端刷新对应会话的 todo 面板
            if ok
                && tool_name == "todo_write"
                && let Some(sid) = session_id
            {
                crate::diagnostics::safe_emit(app, &format!("session-todos-{sid}"), ());
            }
            let _ = channel.send(StreamChunk::ToolCallResult {
                tool_call_id: tc.id.clone(),
                tool_name: tool_name.clone(),
                server_name: "builtin".into(),
                content: content.clone(),
                ok,
                arguments,
                elapsed_ms,
                log_id,
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
                    log_id: None,
                });
                tool_messages.push(tool_message(&tc.id, msg));
                continue;
            }
        };

        // 2. 审批(会话内已放行的工具直接跳过弹窗);AllowEdited 用用户改后的参数
        let exec_args = match ask_approval(
            app,
            approval,
            capability,
            &data_dir,
            &server,
            &tool_name,
            args,
            session_id,
            false,
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
                    log_id: None,
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
                log_id: None,
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

        // 5. 结果推前端(UI 展示工具卡片);P4:外部工具同样记录全文并回传 log id
        let log_id = record_tool_log(session_id, &tool_name, &exec_args, &content);
        let _ = channel.send(StreamChunk::ToolCallResult {
            tool_call_id: tc.id.clone(),
            tool_name: tool_name.clone(),
            server_name: server,
            content: content.clone(),
            ok: result.ok,
            arguments,
            elapsed_ms,
            log_id,
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
///
/// `forced=true` 表示必须人工确认、跳过一切自动放行(风险/白名单/能力令牌/会话常允许):
///   - D3 工作区外读取(低风险但需确认)
///   - P1 高危 shell 命令(完全信任档下也不交给模型自行确认)
///
/// 「完全信任」档位本身仍会拦截 `forced` 调用 —— 高危动作始终需要人类点击。
#[allow(clippy::too_many_arguments)]
async fn ask_approval(
    app: &tauri::AppHandle,
    approval: &ApprovalStore,
    capability: &CapabilityStore,
    data_dir: &std::path::Path,
    server: &str,
    tool: &str,
    args: Value,
    session_id: Option<&str>,
    forced: bool,
) -> Option<Value> {
    // P1:高危 shell 命令(格式化/删根/改注册表等)强制人工确认。
    // `confirm` 是模型可写的工具参数,不作为人类确认依据,在此统一忽略。
    let danger = dangerous_shell_pattern(server, tool, &args);
    let forced = forced || danger.is_some();
    if !forced
        && approval.is_session_allowed(server, tool).await
    {
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
    // P8:声明为「完全信任」但临时授权缺失/过期/会话不匹配时,实际按「工作区」档执行,
    // 能力令牌消费条件也随之自动收缩,避免残留令牌在失效后继续免审批。
    policy.mode = super::approval::effective_mode(&policy, data_dir, session_id);
    let risk = super::approval::assess_risk(server, tool, &args);
    // 能力令牌:已并入「完全信任」总开关 —— 仅完全信任模式下才消费令牌,
    // 避免 full 期间签发的残留令牌在切回逐次/工作区档后继续免审批。
    // forced(工作区外读取 / 高危 shell)不消费令牌:令牌不能被用来静默放行。
    if !forced
        && policy.mode == super::approval::ApprovalMode::Full
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
    if !forced && !policy.requires_approval(server, tool, risk, &args) {
        crate::mcp::audit(
            data_dir,
            "approval_skip",
            server,
            tool,
            &format!("mode={:?} risk={:?}", policy.mode, risk),
        );
        return Some(args);
    }
    let (_id, mut req, rx) = approval.register(server, tool, args.clone()).await;
    if let Some(pattern) = &danger {
        // P1:高危 shell 必须醒目提示命中的危险片段,由用户决定是否执行
        req.risk = super::approval::ApprovalRisk::High;
        req.reason = format!(
            "检测到高风险命令片段 `{pattern}`(可能导致数据丢失或系统损坏),请确认无误后再放行"
        );
    } else if forced {
        // 工作区外读取:低风险工具也要把风险标成 High 并说明原因,前端弹窗才会醒目
        req.risk = super::approval::ApprovalRisk::High;
        req.reason = format!(
            "工具 `{tool}` 要读取工作区外的本机文件,请确认路径无误后再放行"
        );
    }
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
    builtin_needs_approval, capability_scope, dangerous_shell_pattern, execute_builtin,
    is_builtin_tool, reads_outside_workspace, record_tool_log,
};
