//! MCP 相关 Tauri commands —— Phase 5:配置 + 运行时 + HITL 审批 + 内置时间。

use crate::mcp::{
    self, ApprovalPolicy, ApprovalResult, ApprovalStore, McpConfig, McpRuntime, McpServerConfig,
    McpToolCall,
};
use crate::paths::data_dir as data_dir_fn;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

/// 全局 MCP 配置 state。
pub struct McpState(pub Mutex<McpConfig>);

// ============ 配置管理(Phase 4) ============

#[tauri::command]
pub fn mcp_load_config(state: State<'_, McpState>) -> Result<McpConfig, String> {
    let dir = data_dir_fn();
    let cfg = mcp::load_or_init(&dir);
    if let Ok(mut g) = state.0.lock() {
        *g = cfg.clone();
    }
    Ok(cfg)
}

#[tauri::command]
pub async fn mcp_save_config(
    cfg: McpConfig,
    state: State<'_, McpState>,
    runtime: State<'_, McpRuntime>,
) -> Result<(), String> {
    // 记录保存前「应运行」的 server 集合,用于禁用即停
    let prev_running: std::collections::HashSet<String> = state
        .0
        .lock()
        .map_err(|e| format!("lock 失败: {e}"))?
        .servers
        .iter()
        .filter(|s| s.enabled)
        .map(|s| s.name.clone())
        .collect();
    let dir = data_dir_fn();
    mcp::save(&dir, &cfg)?;
    let next_running: std::collections::HashSet<String> = cfg
        .servers
        .iter()
        .filter(|s| s.enabled)
        .map(|s| s.name.clone())
        .collect();
    if let Ok(mut g) = state.0.lock() {
        *g = cfg;
    }
    // 禁用即停:原本启用、现在被禁用或删除的 server,立刻停掉子进程
    // (此前仅改配置,进程与数据文件句柄继续持有,用户感知为"关了但没关")
    for name in prev_running.difference(&next_running) {
        runtime.stop_server(name).await;
    }
    Ok(())
}

/// 清除「长期记忆」MCP server 的全局记忆数据(data/mcp-memory.jsonl)。
/// 先停止所有 server-memory 实例释放文件句柄,再删除文件;返回被删除的字节数(0 = 本为空)。
#[tauri::command]
pub async fn mcp_clear_memory(
    state: State<'_, McpState>,
    runtime: State<'_, McpRuntime>,
) -> Result<u64, String> {
    let names: Vec<String> = state
        .0
        .lock()
        .map_err(|e| format!("lock 失败: {e}"))?
        .servers
        .iter()
        .filter(|s| s.args.iter().any(|a| a.contains("server-memory")))
        .map(|s| s.name.clone())
        .collect();
    for n in &names {
        runtime.stop_server(n).await;
    }
    let path = data_dir_fn().join("mcp-memory.jsonl");
    if !path.exists() {
        return Ok(0);
    }
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    std::fs::remove_file(&path).map_err(|e| format!("删除记忆文件失败: {e}"))?;
    mcp::audit(
        &data_dir_fn(),
        "memory_clear",
        "长期记忆",
        "-",
        "user_clear",
    );
    Ok(size)
}

// ============ 审批模式(「解放双手」) ============

/// 读取审批策略(手动 / 自动 / 完全访问 / 自定义)。文件缺失 → 手动审批。
#[tauri::command]
pub fn mcp_get_approval_policy() -> Result<ApprovalPolicy, String> {
    Ok(mcp::approval::load_policy(&data_dir_fn()))
}

/// 保存审批策略 —— 立即对下一次工具调用生效(dispatcher 每次读文件)。
#[tauri::command]
pub fn mcp_set_approval_policy(policy: ApprovalPolicy) -> Result<(), String> {
    mcp::approval::save_policy(&data_dir_fn(), &policy)
}

#[tauri::command]
pub fn mcp_add_server(server: McpServerConfig, state: State<'_, McpState>) -> Result<(), String> {
    if server.name.is_empty() {
        return Err("name 不能为空".into());
    }
    // 安全:stdio 拒绝 shell 包装器作为启动命令;http 校验端点协议与请求头
    if server.is_http() {
        crate::mcp::safety::validate_server_url(&server.url)?;
        crate::mcp::safety::validate_headers(&server.headers)?;
    } else {
        if server.command.is_empty() {
            return Err("command 不能为空".into());
        }
        crate::mcp::safety::validate_server_command(&server.command)?;
    }
    let mut cfg = state
        .0
        .lock()
        .map_err(|e| format!("lock 失败: {e}"))?
        .clone();
    if cfg.servers.iter().any(|s| s.name == server.name) {
        return Err(format!("server `{}` 已存在", server.name));
    }
    cfg.servers.push(server);
    let dir = data_dir_fn();
    mcp::save(&dir, &cfg)?;
    *state.0.lock().unwrap() = cfg;
    Ok(())
}

#[tauri::command]
pub fn mcp_remove_server(name: String, state: State<'_, McpState>) -> Result<(), String> {
    let mut cfg = state
        .0
        .lock()
        .map_err(|e| format!("lock 失败: {e}"))?
        .clone();
    let before = cfg.servers.len();
    cfg.servers.retain(|s| s.name != name);
    if cfg.servers.len() == before {
        return Err(format!("server `{name}` 不存在"));
    }
    let dir = data_dir_fn();
    mcp::save(&dir, &cfg)?;
    *state.0.lock().unwrap() = cfg;
    Ok(())
}

// ============ 运行时(Phase 5) ============

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
    runtime.start_from_config(&cfg, &data_dir_fn()).await
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
        match runtime.start_from_config(&server, &dir).await {
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
    runtime.list_tools(&name).await
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

    // 能力令牌:与 ChatStream 主链路同一顺序(会话允许 → 令牌 → 弹窗);
    // 令牌已并入「完全信任」总开关,非完全信任模式不消费(回落弹窗)
    let risk = mcp::approval::assess_risk(&call.server_name, &call.tool_name, &call.arguments);
    let scope = mcp::builtin::capability_scope(&call.server_name, &call.tool_name, risk);
    if mcp::approval::load_policy(&dir).mode == mcp::ApprovalMode::Full
        && let Some(cap) = capability.consume(scope)
    {
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

// ============ 内置时间工具(Phase 6) ============

#[derive(Serialize, Deserialize)]
pub struct NowResult {
    pub iso: String,
    pub unix_ms: i64,
    pub tz: String,
    pub date: String,
    pub time: String,
    pub weekday: String,
}

#[tauri::command]
pub fn get_now(_tz: Option<String>) -> Result<NowResult, String> {
    use chrono::Local;
    let now = Local::now();
    let weekday_names = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
    let wd_idx = now.format("%u").to_string().parse::<usize>().unwrap_or(1);
    Ok(NowResult {
        iso: now.to_rfc3339(),
        unix_ms: now.timestamp_millis(),
        tz: now.format("%:z").to_string(),
        date: now.format("%Y-%m-%d").to_string(),
        time: now.format("%H:%M:%S").to_string(),
        weekday: weekday_names
            .get(wd_idx - 1)
            .copied()
            .unwrap_or("")
            .to_string(),
    })
}

// ============ Phase 5e:工具定义下发(前端随 chat_stream 传给模型) ============

/// 汇总内置工具与已启动 MCP server 的工具,转成 OpenAI function-calling 定义。
/// R4/D2:只读内置工具默认下发(长文档 doc_search 不再依赖工具总开关);
/// 写入/执行类工具与 MCP server 工具仍需打开总开关。
#[tauri::command]
pub async fn mcp_collect_tools(
    runtime: State<'_, McpRuntime>,
    state: State<'_, McpState>,
) -> Result<Vec<crate::provider::ToolDef>, String> {
    let dir = data_dir_fn();
    let (tools_enabled, builtin_enabled) = state
        .0
        .lock()
        .map(|c| (c.tools_enabled, c.builtin_tools))
        .unwrap_or((false, false));
    let full_trust = mcp::approval::load_policy(&dir).mode == mcp::ApprovalMode::Full;

    let mut out: Vec<crate::provider::ToolDef> = Vec::new();
    // 只读内置工具(get_now / todo / sys / fs 读 / file_extract / doc_search / doc_read …)
    // 默认可用:它们不改变磁盘与系统状态,且是长文档分析的前置能力。
    if builtin_enabled {
        out.extend(super::mcp_builtin_tools::readonly_tool_defs());
    }
    // 完全信任模式:GUI 自动化工具(ui_*)也下发
    if full_trust {
        out.extend(crate::gui::tools::tool_defs());
    }
    // 总开关关闭:不再下发 MCP server 工具与写入类内置工具
    if !tools_enabled {
        return Ok(out);
    }
    // 只收集「已启用」配置里的 server,防止用户停用后仍被调用
    let servers: Vec<McpServerConfig> = state
        .0
        .lock()
        .map(|c| {
            c.servers
                .iter()
                .filter(|s| {
                    if !s.enabled {
                        return false;
                    }
                    // 安全:配置被手工改成 shell 包装器或非法 URL 时,同样拒绝启动
                    let safe = if s.is_http() {
                        crate::mcp::safety::validate_server_url(&s.url)
                            .and_then(|_| crate::mcp::safety::validate_headers(&s.headers))
                    } else {
                        crate::mcp::safety::validate_server_command(&s.command)
                    };
                    match safe {
                        Ok(()) => true,
                        Err(e) => {
                            log::warn!("跳过不安全的 MCP server `{}`: {e}", s.name);
                            false
                        }
                    }
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    for server in servers {
        let name = server.name.clone();
        // 懒启动:应用启动时已统一拉起过所有 enabled server,
        // 这里只在 handle 不存在或已 stale(进程死了)时才启动/重启一次;
        // 幂等保护在 start_server 内部:健康运行的 server 不会被 kill+respawn。
        let tools = match runtime.list_tools(&name).await {
            Ok(t) => t,
            Err(_) => {
                let was_running = runtime.is_running(&name).await;
                match runtime.start_from_config(&server, &dir).await {
                    Ok(()) => match runtime.list_tools(&name).await {
                        Ok(t) => t,
                        Err(e) => {
                            log::warn!("重启后仍 list_tools 失败 `{name}`: {e}");
                            continue;
                        }
                    },
                    Err(e) => {
                        log::warn!(
                            "{} MCP server `{name}` 失败: {e}",
                            if was_running {
                                "stale 重启"
                            } else {
                                "懒启动"
                            }
                        );
                        continue;
                    }
                }
            }
        };
        for t in tools {
            let params = if t.input_schema.is_null() {
                serde_json::json!({ "type": "object", "properties": {} })
            } else {
                t.input_schema
            };
            out.push(crate::provider::ToolDef {
                r#type: "function".into(),
                function: crate::provider::ToolFunctionDef {
                    name: t.name,
                    description: if t.description.is_empty() {
                        None
                    } else {
                        Some(t.description)
                    },
                    parameters: Some(params),
                },
            });
        }
    }
    // 写入/执行类内置工具(shell / fs 写 / 导出 / 解包):总开关打开时才下发
    if builtin_enabled {
        out.extend(super::mcp_builtin_tools::write_tool_defs());
    }
    Ok(out)
}

// ============ Phase 6:白名单 Shell 命令(执行内核见 mcp::shell) ============

/// 执行白名单命令;审批策略为「完全访问」时放开为任意命令(用户明示完全信任 MCP)。
/// 非信任模式下的校验规则(解释器拒绝 / 元字符 / 只读白名单)与超时控制全部在 `mcp::shell`。
///
/// D8 安全收口:完全信任模式下,命中 DANGEROUS_PATTERNS 的命令(格式化/删根/注册表等)
/// 仍需二次确认。`confirm_danger=true` 表示用户已确认;否则返回带 `DANGEROUS_CONFIRM`
/// 前缀的错误,前端据此弹确认框后带 true 重调。
#[tauri::command]
pub async fn shell_whitelist_exec(
    cmd: String,
    args: Vec<String>,
    cwd: Option<String>,
    confirm_danger: Option<bool>,
) -> Result<String, String> {
    let data_dir = crate::paths::data_dir();
    let cwd = cwd.as_deref();
    let full = crate::mcp::approval::load_policy(&data_dir).mode == crate::mcp::ApprovalMode::Full;
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
        crate::mcp::shell::run_unrestricted(&cmd, &args, cwd).await
    } else {
        crate::mcp::shell::run(&cmd, &args, cwd).await
    }
    .map(|o| o.text)
}

// ============ ApprovalStore 辅助方法 ============
// (定义在 approval.rs,这里通过 State 访问)

#[cfg(test)]
mod tests {
    /// 回归:Windows 内置命令走 `cmd /c`,若不拦元字符,`["&","calc"]` 可绕过白名单。
    #[test]
    fn shell_rejects_cmd_metachars_in_args() {
        use std::path::Path;
        let err = crate::mcp::shell::validate(
            "dir",
            &["&".to_string(), "calc".to_string()],
            Path::new("."),
        )
        .expect_err("应以元字符拒绝");
        assert!(err.contains("元字符"), "实际错误:{err}");

        let err = crate::mcp::shell::validate("dir", &["a > b.txt".to_string()], Path::new("."))
            .expect_err("应以元字符拒绝");
        assert!(err.contains("元字符"), "实际错误:{err}");
    }

    /// 回归:命令名本身也不得携带元字符。
    #[test]
    fn shell_rejects_metachars_in_command_name() {
        use std::path::Path;
        assert!(crate::mcp::shell::validate("dir&calc", &[], Path::new(".")).is_err());
        assert!(crate::mcp::shell::validate("echo%PATH%", &[], Path::new(".")).is_err());
    }

    /// 白名单外的命令与解释器一律拒绝(含 git 写子命令)。
    #[test]
    fn shell_rejects_non_whitelisted_and_interpreters() {
        use std::path::Path;
        let v = |cmd: &str, args: &[&str]| {
            let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            crate::mcp::shell::validate(cmd, &argv, Path::new("."))
        };
        assert!(v("rm", &[]).is_err());
        assert!(v("git", &["push"]).is_err());
        assert!(v("git", &["status"]).is_ok());
        // H-1 回归:解释器无论参数如何都必须被拒绝
        assert!(v("python", &["-c", "print(1)"]).is_err());
        assert!(v("node", &["-e", "1"]).is_err());
        assert!(v("powershell", &["-Command", "whoami"]).is_err());
    }

    /// 回归:前端下发 snake_case 决策,后端必须能反序列化(曾用 lowercase → 全部失败)。
    #[test]
    fn approval_result_accepts_snake_case_decisions() {
        use crate::mcp::approval::ApprovalResult;
        let cases = [
            (r#"{"decision":"allow"}"#, "allow"),
            (r#"{"decision":"allow_always"}"#, "allow_always"),
            (r#"{"decision":"deny"}"#, "deny"),
        ];
        for (raw, label) in cases {
            let v: ApprovalResult =
                serde_json::from_str(raw).unwrap_or_else(|e| panic!("{label} 反序列化失败:{e}"));
            match (label, v) {
                ("allow", ApprovalResult::Allow) => {}
                ("allow_always", ApprovalResult::AllowAlways) => {}
                ("deny", ApprovalResult::Deny) => {}
                (_, other) => panic!("{label} 解析成了错误变体:{other:?}"),
            }
        }
        let edited: ApprovalResult =
            serde_json::from_str(r#"{"decision":"allow_edited","arguments":{"command":"ls"}}"#)
                .expect("应能反序列化 allow_edited");
        match edited {
            ApprovalResult::AllowEdited { arguments } => {
                assert_eq!(arguments["command"], "ls");
            }
            other => panic!("类型错误:{other:?}"),
        }
    }

    /// 内置工具定义与 dispatcher 的 builtin 判定必须同名,否则模型调不到。
    #[test]
    fn builtin_tool_names_match_dispatcher_whitelist() {
        let tools = crate::commands::mcp_builtin_tools::builtin_tool_defs();
        assert!(!tools.is_empty());
        for t in tools {
            assert!(
                crate::mcp::dispatcher::is_builtin_tool(&t.function.name),
                "{} 未在 dispatcher 注册",
                t.function.name
            );
            assert_eq!(t.r#type, "function");
        }
    }
}
