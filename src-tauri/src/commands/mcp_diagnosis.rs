//! C2 mcp_doctor:MCP server 诊断命令。
//!
//! 对每个 enabled server 做健康检查,返回结构化诊断信息:
//! - name / transport / enabled / is_preset
//! - is_running(runtime 是否有 handle)
//! - peek_alive 结果(进程是否真能响应 tools/list)
//! - disabled_by_failure 状态(C3 连续失败自动禁用标记)
//! - command 二进制可执行检查(Windows 按 PATH + 扩展名解析;Unix 按 PATH 搜索)
//!
//! 前端设置页「MCP 管理」可按需调用此命令,把诊断结果显示在 server 卡片上。

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::mcp::config::McpServerConfig;
use crate::mcp::runtime::McpRuntime;
use crate::mcp::util::check_command_resolvable;

/// McpState 在 commands/mcp.rs 定义,通过 super::mcp:: 引用。
use super::mcp::McpState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpDoctorResult {
    pub name: String,
    pub enabled: bool,
    pub is_preset: bool,
    pub transport: String,
    // 值固定为 `stdio` / `http`(与前端判断一致,不要用 Debug 输出)
    pub is_running: bool,
    pub disabled_by_failure: bool,
    pub peek_ok: bool,
    pub peek_error: Option<String>,
    pub command_resolved: bool,
    pub command_path: Option<String>,
    /// P0b:实际启动方式 —— `npx`(默认)/ `native`(opt-in 且命中 manifest)/ `http`
    pub backend: String,
}

#[tauri::command]
pub async fn mcp_doctor(
    state: State<'_, McpState>,
    runtime: State<'_, McpRuntime>,
) -> Result<Vec<McpDoctorResult>, String> {
    let servers: Vec<McpServerConfig> = state
        .0
        .lock()
        .map_err(|e| format!("lock 失败: {e}"))?
        .servers
        .clone();

    let mut out = Vec::with_capacity(servers.len());
    for server in servers {
        let is_running = runtime.is_running(&server.name).await;
        let disabled_by_failure = runtime.failure.is_disabled(&server.name).await;

        // peek_alive:只有 stdio 已启动的 server 才探活;http 跳过(单独测)
        let (peek_ok, peek_error) = if !server.is_http() && is_running {
            match runtime.peek_alive(&server.name, server.is_preset).await {
                Ok(()) => (true, None),
                Err(e) => (false, Some(e)),
            }
        } else {
            (false, None)
        };

        // command 二进制可执行检查(仅 stdio 有效)
        let (command_resolved, command_path) = if !server.is_http() {
            check_command_resolvable(&server.command)
        } else {
            (true, None) // http 不传 command
        };

        out.push(McpDoctorResult {
            backend: crate::mcp::native::describe_backend(
                &server.name,
                &server.command,
                server.is_preset,
                server.is_http(),
            )
            .to_string(),
            name: server.name,
            enabled: server.enabled,
            is_preset: server.is_preset,
            transport: match server.transport {
                crate::mcp::config::Transport::Stdio => "stdio".to_string(),
                crate::mcp::config::Transport::Http => "http".to_string(),
            },
            is_running,
            disabled_by_failure,
            peek_ok,
            peek_error,
            command_resolved,
            command_path,
        });
    }
    Ok(out)
}
