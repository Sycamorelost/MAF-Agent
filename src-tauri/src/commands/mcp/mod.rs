//! MCP 相关 Tauri commands —— Phase 5:配置 + 运行时 + HITL 审批。
//!
//! 分层:
//! - 本文件:配置管理(加载/保存/增删 server)与审批策略读写;
//! - `runtime_cmd.rs`:启动/停止、列工具、带审批的工具调用、白名单 shell;
//! - `tools.rs`:按会话收集下发给模型的工具定义。

mod runtime_cmd;
mod tools;

pub use runtime_cmd::*;
pub use tools::*;

use crate::mcp::{self, ApprovalPolicy, McpConfig, McpServerConfig};
use crate::paths::data_dir as data_dir_fn;
use std::sync::Mutex;
use tauri::State;

/// 全局 MCP 配置 state。
pub struct McpState(pub Mutex<McpConfig>);

/// 启动初始化状态(0=无/未开始,1=初始化中,2=完成);前端据此显示「正在初始化组件…」。
#[tauri::command]
pub async fn get_bootstrap_state() -> Result<u8, String> {
    Ok(mcp::bootstrap_state())
}

// ============ 配置管理(Phase 4) ============

#[tauri::command]
pub async fn mcp_load_config(state: State<'_, McpState>) -> Result<McpConfig, String> {
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
    runtime: State<'_, mcp::McpRuntime>,
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

// ============ 审批模式(「解放双手」) ============

/// 读取审批策略(手动 / 自动 / 完全访问 / 自定义)。文件缺失 → 手动审批。
#[tauri::command]
pub async fn mcp_get_approval_policy() -> Result<ApprovalPolicy, String> {
    Ok(mcp::approval::load_policy(&data_dir_fn()))
}

/// 保存审批策略 —— 立即对下一次工具调用生效(dispatcher 每次读文件)。
#[tauri::command]
pub async fn mcp_set_approval_policy(policy: ApprovalPolicy) -> Result<(), String> {
    mcp::approval::save_policy(&data_dir_fn(), &policy)
}

/// P4:按 id 读取工具调用记录全文(前端「查看完整输出」按需加载,避免历史消息膨胀)。
#[tauri::command]
pub async fn mcp_get_tool_log(id: i64) -> Result<Option<crate::db::tool_log::ToolCallLog>, String> {
    Ok(crate::db::tool_log::get(id))
}

#[tauri::command]
pub async fn mcp_add_server(
    server: McpServerConfig,
    state: State<'_, McpState>,
) -> Result<(), String> {
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
pub async fn mcp_remove_server(name: String, state: State<'_, McpState>) -> Result<(), String> {
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

// ============ 内置时间工具(Phase 6) ============
// 实现已上提到 `mcp::time_tool`(执行层不再依赖命令层),`get_now` 工具直接调用该模块;
// 这里不再保留 IPC 包装(前端从未调用,注册表也未登记)。

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
}
