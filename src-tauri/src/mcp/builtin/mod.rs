//! 内置工具路由:能力作用域映射、白名单判定与执行。
//!
//! 审批仍由 dispatcher 负责,本模块只做「是不是内置工具 / 要不要审批 / 作用域是什么 /
//! 怎么执行」。执行按职责拆到子模块:
//!   - `shell`:shell_exec(复用 `mcp::shell` 内核)
//!   - `fs_tools`:文件 / 导出 / 备份 / 提取 / zip
//!   - `docs`:任务清单与会话文档
//!   - `sys_web`:系统只读快照与 web_fetch
//!   - `delegate`:子智能体委派
//!
//! 工具名单与 `get_now` 实现来自中立模块(`mcp::tool_spec` / `mcp::time_tool`),
//! 本模块不再反向依赖 `commands`。

use serde_json::Value;

use super::approval::ApprovalRisk;
use super::capability::Scope;

mod delegate;
mod docs;
mod fs_tools;
#[cfg(test)]
mod matrix_tests;
#[cfg(test)]
mod roundtrip_tests;
mod shell;
mod sys_web;

/// 工具 → 能力作用域的映射。外部 MCP 工具按风险粗分:只读走 Net,其余走 Exec。
///
/// 内置工具的作用域从 schema 定义的「只读/写入」分组派生(见
/// `mcp::tool_spec`),仅 `shell_exec`(执行)、`delegate` / `web_fetch`
/// (网络)与 `ui_*`(GUI 自动化)单独标注,避免名单重复维护。
pub(crate) fn capability_scope(server: &str, tool: &str, risk: ApprovalRisk) -> Scope {
    if server == "builtin" {
        return match tool {
            "shell_exec" => Scope::Exec,
            "delegate" | "web_fetch" => Scope::Net,
            t if t.starts_with("ui_") => Scope::Ui,
            t if crate::mcp::tool_spec::WRITE_TOOL_NAMES.contains(t) => Scope::FsWrite,
            _ => Scope::FsRead,
        };
    }
    if risk == ApprovalRisk::Low {
        Scope::Net
    } else {
        Scope::Exec
    }
}

/// 内置工具白名单。工具名从 schema 定义单源派生(见 `ALL_BUILTIN_TOOL_NAMES`),
/// GUI 工具由 `gui::tools` 单独注册。
pub(crate) fn is_builtin_tool(name: &str) -> bool {
    crate::mcp::tool_spec::ALL_BUILTIN_TOOL_NAMES.contains(name)
        || crate::gui::tools::is_gui_tool(name)
}

/// 内置工具是否需要人工审批。写入/执行类工具(来自 `WRITE_TOOL_NAMES`)一律审批;
/// 只读工具默认免打扰,但 D3 起「指向工作区外本机文件」的只读工具也需审批
/// (见 `reads_outside_workspace`)。GUI 工具不在此列(由「完全信任」总开关在执行期直接拒绝)。
pub(crate) fn builtin_needs_approval(name: &str) -> bool {
    crate::mcp::tool_spec::WRITE_TOOL_NAMES.contains(name)
}

/// D3:只读工具的目标路径是否在工作区外。
///
/// 只处理按 path 读取本机文件的工具;`doc_*`、`sys_*`、`get_now`、`web_fetch`
/// 不触碰本机文件(web_fetch 自带 SSRF 拦截),维持免审批。
pub(crate) fn reads_outside_workspace(name: &str, args: &Value, session_id: Option<&str>) -> bool {
    const PATH_READ_TOOLS: [&str; 5] = [
        "fs_read",
        "fs_list",
        "fs_grep",
        "file_extract",
        "zip_list",
    ];
    if !PATH_READ_TOOLS.contains(&name) {
        return false;
    }
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    !crate::fs::paths::within_workspace(session_id, path)
}

/// P1:完全信任档下仍需人工确认的高危 shell 命令;返回命中的危险片段。
///
/// 模型传入的 `confirm` 参数一律忽略 —— 工具参数是模型可写的内容,不是人类确认。
/// 是否弹窗由 `dispatcher::ask_approval` 统一处理(forced 审批)。
pub(crate) fn dangerous_shell_pattern(server: &str, tool: &str, args: &Value) -> Option<String> {
    if server != "builtin" || tool != "shell_exec" {
        return None;
    }
    let cmd = args
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if cmd.is_empty() {
        return None;
    }
    let (argv, _) = crate::mcp::shell::parse_shell_args(args.get("args"));
    crate::mcp::shell::dangerous_pattern(cmd, &argv).map(|p| p.trim().to_string())
}

/// 执行内置工具。返回 (给模型的内容, 是否成功)。
///
/// `trust_shell`(完全信任 MCP,审批策略 = 完全访问)为 true 时 shell_exec 跳过白名单校验,
/// 直接执行任意命令 —— 信任判定由调用方(dispatcher / IPC 层)完成,此函数保持纯执行。
/// 高危命令的人工确认已在审批层完成(见 `dangerous_shell_pattern`)。
/// `session_id` 供 todo_*/doc_*/web_fetch 定位会话上下文;`sub_llm` 供 delegate 委派子智能体。
pub(crate) async fn execute_builtin(
    name: &str,
    args: &Value,
    trust_shell: bool,
    session_id: Option<&str>,
    sub_llm: Option<&crate::provider::SubLlm>,
) -> (String, bool) {
    // GUI 自动化:UIA / SendInput 都是阻塞调用,隔离到阻塞线程池,避免卡住 tokio worker
    if crate::gui::tools::is_gui_tool(name) {
        let tool = name.to_string();
        let a = args.clone();
        let sid = session_id.map(|s| s.to_string());
        return match tokio::task::spawn_blocking(move || {
            crate::gui::tools::execute(&tool, &a, sid.as_deref())
        })
        .await
        {
            Ok(r) => r,
            Err(e) => (format!("GUI 工具执行线程异常:{e}"), false),
        };
    }

    if fs_tools::handles(name) {
        return fs_tools::exec(name, args, session_id);
    }
    if docs::handles(name) {
        return docs::exec(name, args, session_id);
    }
    if sys_web::handles(name) {
        return sys_web::exec(name, args, session_id).await;
    }

    match name {
        "get_now" => {
            let now = crate::mcp::time_tool::now();
            (
                format!(
                    "当前时间:{} {} (UTC{}) 日期:{}",
                    now.date, now.time, now.tz, now.weekday
                ),
                true,
            )
        }
        "delegate" => delegate::exec(args, sub_llm).await,
        "present" => crate::deliverables::present(session_id, args),
        "fs_to_pdf" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            match crate::fs::convert::convert_in_workspace(session_id, path) {
                Ok(p) => (format!("已生成 PDF:{}", p.display()), true),
                Err(e) => (format!("转换 PDF 失败:{e}"), false),
            }
        }
        "shell_exec" => shell::exec(args, trust_shell, session_id).await,
        _ => ("未知内置工具".into(), false),
    }
}

/// 审计日志写入助手:从 args 里提取 path/cmd 作为 target_path,截断 args_preview 到 500 字符。
/// 由 dispatcher 统一调用(内置与外部 MCP 工具共用),返回日志行 id。
pub(crate) fn record_tool_log(
    session_id: Option<&str>,
    tool_name: &str,
    args: &Value,
    content: &str,
) -> Option<i64> {
    let sid = session_id?; // 无会话上下文时不写
    let project_id = crate::db::sessions::get(sid)
        .ok()
        .flatten()
        .and_then(|s| s.project_id);
    let target_path = args
        .get("path")
        .and_then(|v| v.as_str())
        .or_else(|| args.get("dest").and_then(|v| v.as_str()))
        .or_else(|| args.get("command").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let args_preview = serde_json::to_string(args).ok();
    crate::db::tool_log::record(
        sid,
        project_id.as_deref(),
        tool_name,
        target_path.as_deref(),
        args_preview.as_deref(),
        content,
    )
}

// 测试见 `matrix_tests`(注册/审批矩阵、D3、P1 高危 shell)与 `roundtrip_tests`(分段写入回环)。
