//! 内置工具路由:能力作用域映射、白名单判定与执行。
//!
//! 从 dispatcher 拆出,避免单文件过长;审批仍由 dispatcher 负责,本模块只做
//! 「是不是内置工具 / 要不要审批 / 作用域是什么 / 怎么执行」。

use serde_json::Value;

use super::approval::ApprovalRisk;
use super::capability::Scope;

/// 工具 → 能力作用域的映射。外部 MCP 工具按风险粗分:只读走 Net,其余走 Exec。
pub(crate) fn capability_scope(server: &str, tool: &str, risk: ApprovalRisk) -> Scope {
    if server == "builtin" {
        return match tool {
            "fs_list" | "fs_read" | "file_extract" | "zip_list" => Scope::FsRead,
            "fs_write" | "fs_delete" | "fs_restore" | "fs_export_docx" | "fs_export_xlsx"
            | "zip_extract" => Scope::FsWrite,
            "shell_exec" => Scope::Exec,
            "todo_read" => Scope::FsRead,
            "todo_write" => Scope::FsWrite,
            "delegate" => Scope::Net,
            // G5 系统状态快照:只读,归 FsRead
            t if t.starts_with("sys_") => Scope::FsRead,
            t if t.starts_with("ui_") => Scope::Ui,
            _ => Scope::FsRead,
        };
    }
    if risk == ApprovalRisk::Low {
        Scope::Net
    } else {
        Scope::Exec
    }
}

/// 内置工具白名单(名称必须与 mcp_collect_tools 下发的定义一致)。
pub(crate) fn is_builtin_tool(name: &str) -> bool {
    matches!(
        name,
        "get_now"
            | "shell_exec"
            | "fs_list"
            | "fs_read"
            | "fs_write"
            | "fs_delete"
            | "fs_restore"
            | "fs_export_docx"
            | "fs_export_xlsx"
            | "file_extract"
            | "zip_list"
            | "zip_extract"
            | "todo_write"
            | "todo_read"
            | "delegate"
            | "sys_overview"
            | "sys_processes"
            | "sys_disks"
            | "sys_networks"
    ) || crate::gui::tools::is_gui_tool(name)
}

/// 内置工具是否需要人工审批(有副作用的一律审批;只读不打扰)。
/// GUI 工具不再单独走审批:已并入「完全信任」总开关(见 gui::tools::run),
/// 非完全信任模式在执行期直接拒绝,弹窗没有意义。
pub(crate) fn builtin_needs_approval(name: &str) -> bool {
    matches!(
        name,
        "shell_exec"
            | "fs_write"
            | "fs_delete"
            | "fs_restore"
            | "fs_export_docx"
            | "fs_export_xlsx"
            | "zip_extract"
    )
}

/// 执行内置工具。返回 (给模型的内容, 是否成功)。
/// shell_exec 复用 `mcp::shell` 的校验与执行内核,不新开执行路径。
/// `trust_shell`(完全信任 MCP,审批策略 = 完全访问)为 true 时 shell_exec 跳过白名单校验,
/// 直接执行任意命令 —— 信任判定由调用方(dispatcher / IPC 层)完成,此函数保持纯执行。
/// `session_id` 供 todo_* 工具定位会话任务清单;`sub_llm` 供 delegate 委派子智能体。
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
        return match tokio::task::spawn_blocking(move || crate::gui::tools::execute(&tool, &a))
            .await
        {
            Ok(r) => r,
            Err(e) => (format!("GUI 工具执行线程异常:{e}"), false),
        };
    }
    match name {
        "get_now" => match crate::commands::mcp::get_now(None) {
            Ok(now) => (
                format!(
                    "当前时间:{} {} (UTC{}) 日期:{}",
                    now.date, now.time, now.tz, now.weekday
                ),
                true,
            ),
            Err(e) => (format!("获取时间失败:{e}"), false),
        },
        "todo_write" => todo_write_exec(args, session_id),
        "todo_read" => todo_read_exec(session_id),
        "delegate" => delegate_exec(args, sub_llm).await,
        // G5 系统状态快照:只读免审批;CPU 采样含约 200ms 间隔,隔离到阻塞线程池
        "sys_overview" | "sys_processes" | "sys_disks" | "sys_networks" => {
            let tool = name.to_string();
            let limit = args
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            let r = tokio::task::spawn_blocking(move || {
                let v = match tool.as_str() {
                    "sys_overview" => crate::sys::overview(),
                    "sys_processes" => crate::sys::processes(limit),
                    "sys_disks" => crate::sys::disks(),
                    _ => crate::sys::networks(),
                };
                crate::sys::audit(&tool);
                v
            })
            .await;
            match r {
                Ok(v) => (serde_json::to_string(&v).unwrap_or_default(), true),
                Err(e) => (format!("系统快照线程异常:{e}"), false),
            }
        }
        "shell_exec" => {
            let cmd = args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if cmd.is_empty() {
                return (
                    "缺少 command 参数\n[提示] command 填程序名(如 git、ls、where),参数放进 args 数组"
                        .into(),
                    false,
                );
            }
            // args 容错:字符串/数字不再静默丢弃(见 shell::parse_shell_args)
            let (argv, warn) = crate::mcp::shell::parse_shell_args(args.get("args"));
            let cwd = args
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            // 信任判定在 dispatcher / IPC 层完成;非信任路径走白名单校验,信任路径直接执行
            let r = if trust_shell {
                crate::mcp::shell::run_unrestricted(cmd, &argv, cwd).await
            } else {
                crate::mcp::shell::run(cmd, &argv, cwd).await
            };
            match r {
                Ok(out) => {
                    let ok = out.success();
                    let mut text = out.text;
                    if let Some(w) = warn {
                        text.push_str(&format!("\n[warn] {w}"));
                    }
                    (text, ok)
                }
                Err(e) => (format!("命令被拒绝或执行失败:{e}"), false),
            }
        }
        "fs_list" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            match crate::fs::text::list(path) {
                Ok(entries) => (serde_json::to_string(&entries).unwrap_or_default(), true),
                Err(e) => (format!("列目录失败:{e}"), false),
            }
        }
        "fs_read" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            let max = args
                .get("max_chars")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            match crate::fs::text::read_text(path, max) {
                Ok(text) => (text, true),
                Err(e) => (format!("读取失败:{e}"), false),
            }
        }
        "fs_write" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            match crate::fs::text::write_text(path, content) {
                Ok(r) => (serde_json::to_string(&r).unwrap_or_default(), true),
                Err(e) => (format!("写入失败:{e}"), false),
            }
        }
        "fs_delete" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            match crate::fs::text::delete(path) {
                Ok(r) => (serde_json::to_string(&r).unwrap_or_default(), true),
                Err(e) => (format!("删除失败:{e}"), false),
            }
        }
        "fs_export_docx" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            match crate::fs::text::export_docx(path, title, body) {
                Ok(r) => (serde_json::to_string(&r).unwrap_or_default(), true),
                Err(e) => (format!("导出 docx 失败:{e}"), false),
            }
        }
        "fs_export_xlsx" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            let sheets: Vec<crate::fs::text::SheetInput> = match args.get("sheets") {
                Some(v) => match serde_json::from_value(v.clone()) {
                    Ok(s) => s,
                    Err(e) => return (format!("sheets 参数格式错误:{e}"), false),
                },
                None => Vec::new(),
            };
            match crate::fs::text::export_xlsx(path, &sheets) {
                Ok(r) => (serde_json::to_string(&r).unwrap_or_default(), true),
                Err(e) => (format!("导出 xlsx 失败:{e}"), false),
            }
        }
        "fs_restore" => {
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if id.is_empty() {
                return ("缺少 id 参数".into(), false);
            }
            let root = match crate::fs::text::root() {
                Ok(r) => r,
                Err(e) => return (format!("工作区不可用:{e}"), false),
            };
            match crate::fs::backup::restore(&root, id) {
                Ok(rel) => (format!("已恢复 `{rel}`"), true),
                Err(e) => (format!("恢复失败:{e}"), false),
            }
        }
        "file_extract" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            match crate::fs::formats::extract(path) {
                Ok(text) => (text, true),
                Err(e) => (format!("提取失败:{e}"), false),
            }
        }
        "zip_list" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            match crate::fs::formats::zip_list(path) {
                Ok(entries) => (serde_json::to_string(&entries).unwrap_or_default(), true),
                Err(e) => (format!("列压缩包失败:{e}"), false),
            }
        }
        "zip_extract" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let dest = args.get("dest").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() || dest.is_empty() {
                return ("缺少 path / dest 参数".into(), false);
            }
            match crate::fs::formats::zip_extract(path, dest) {
                Ok(n) => (format!("解包完成,共 {n} 个文件"), true),
                Err(e) => (format!("解包失败:{e}"), false),
            }
        }
        _ => ("未知内置工具".into(), false),
    }
}

/// todo_write 入参(模型提交完整列表;id 可省略,缺省按序编号)。
#[derive(Debug, serde::Deserialize)]
struct TodoItemIn {
    id: Option<String>,
    content: String,
    status: Option<String>,
    priority: Option<String>,
}

const TODO_STATUSES: [&str; 4] = ["pending", "in_progress", "completed", "cancelled"];
const TODO_PRIORITIES: [&str; 3] = ["high", "medium", "low"];
/// 单会话任务上限,防模型刷屏式写入拖垮 UI。
const TODO_MAX_ITEMS: usize = 50;

/// todo_write:校验 + 归一化 + 整表替换落库。无副作用外的审批(内部任务状态,不动用户数据)。
fn todo_write_exec(args: &Value, session_id: Option<&str>) -> (String, bool) {
    let Some(sid) = session_id else {
        return ("任务清单需要会话上下文,当前不可用".into(), false);
    };
    let raw: Vec<TodoItemIn> = args
        .get("items")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    if raw.is_empty() {
        return ("items 不能为空:请提交完整任务列表".into(), false);
    }
    let mut items = Vec::with_capacity(raw.len().min(TODO_MAX_ITEMS));
    for (i, r) in raw.into_iter().take(TODO_MAX_ITEMS).enumerate() {
        let content = r.content.trim();
        if content.is_empty() {
            continue; // 空条目静默跳过,不打断整批
        }
        let content: String = content.chars().take(500).collect();
        let status = r
            .status
            .filter(|s| TODO_STATUSES.contains(&s.as_str()))
            .unwrap_or_else(|| "pending".into());
        let priority = r
            .priority
            .filter(|p| TODO_PRIORITIES.contains(&p.as_str()))
            .unwrap_or_else(|| "medium".into());
        items.push(crate::db::todos::TodoItem {
            id: r
                .id
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| format!("t{}", i + 1)),
            content,
            status,
            priority,
        });
    }
    if items.is_empty() {
        return ("items 里没有有效任务(内容均为空)".into(), false);
    }
    let counts = |s: &str| items.iter().filter(|t| t.status == s).count();
    let result = match crate::db::conn().lock() {
        Ok(conn) => crate::db::todos::replace(&conn, sid, &items),
        Err(e) => Err(format!("db lock 失败: {e}")),
    };
    match result {
        Ok(()) => (
            format!(
                "任务清单已更新:共 {} 项(进行中 {},待办 {},已完成 {},已取消 {})",
                items.len(),
                counts("in_progress"),
                counts("pending"),
                counts("completed"),
                counts("cancelled")
            ),
            true,
        ),
        Err(e) => (format!("任务清单写入失败:{e}"), false),
    }
}

/// todo_read:读出当前会话清单,给模型核对状态。
fn todo_read_exec(session_id: Option<&str>) -> (String, bool) {
    let Some(sid) = session_id else {
        return ("任务清单需要会话上下文,当前不可用".into(), false);
    };
    let items = match crate::db::conn().lock() {
        Ok(conn) => match crate::db::todos::list(&conn, sid) {
            Ok(v) => v,
            Err(e) => return (format!("任务清单读取失败:{e}"), false),
        },
        Err(e) => return (format!("db lock 失败: {e}"), false),
    };
    if items.is_empty() {
        return ("任务清单为空".into(), true);
    }
    let mut out = String::from("当前任务清单:\n");
    for t in &items {
        out.push_str(&format!("- [{}] {} ({})\n", t.status, t.content, t.id));
    }
    (out, true)
}

/// delegate:把独立子任务委派给专项智能体。子调用不带工具,天然防递归。
async fn delegate_exec(args: &Value, sub_llm: Option<&crate::provider::SubLlm>) -> (String, bool) {
    let query = args
        .get("agent")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let task = args
        .get("task")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if task.is_empty() {
        return ("缺少 task 参数".into(), false);
    }
    let Some(llm) = sub_llm else {
        return ("委派通道不可用(当前 provider 未配置 API Key)".into(), false);
    };
    // 先按 agent_id 精确找,再按名称找(用户自定义与内置同名时以 find 结果为准)
    let agent = crate::agents::find(query).or_else(|| {
        crate::agents::list_all()
            .into_iter()
            .find(|a| a.name == query)
    });
    let Some(agent) = agent else {
        return (
            format!("未找到智能体 `{query}`。可用 delegate 的 agent 参数填智能体名称或 id。"),
            false,
        );
    };
    let sys = crate::agents::render_prompt(&agent.system_prompt, &llm.model);
    let msgs = vec![
        crate::provider::ChatMessage {
            role: "system".into(),
            content: sys,
            ..Default::default()
        },
        crate::provider::ChatMessage {
            role: "user".into(),
            content: task.chars().take(20_000).collect(),
            ..Default::default()
        },
    ];
    match llm.collect(msgs, 4096).await {
        Ok(text) => {
            let text = text.trim();
            if text.is_empty() {
                return (format!("子智能体「{}」返回了空内容", agent.name), false);
            }
            let truncated: String = text.chars().take(32_000).collect();
            (
                format!("【子智能体「{}」的执行结果】\n{}", agent.name, truncated),
                true,
            )
        }
        Err(e) => (format!("子智能体「{}」执行失败:{e}", agent.name), false),
    }
}

#[cfg(test)]
mod tests {
    use super::{builtin_needs_approval, execute_builtin, is_builtin_tool};
    use serde_json::json;

    #[test]
    fn builtin_names_are_recognized() {
        assert!(is_builtin_tool("get_now"));
        assert!(is_builtin_tool("shell_exec"));
        assert!(is_builtin_tool("fs_write"));
        assert!(is_builtin_tool("fs_export_xlsx"));
        assert!(is_builtin_tool("file_extract"));
        assert!(!is_builtin_tool("read_file"));
    }

    #[test]
    fn gui_tools_join_the_builtin_matrix() {
        assert!(is_builtin_tool("ui_list_windows"));
        assert!(is_builtin_tool("ui_type_text"));
        // GUI 已并入完全信任总开关,执行期统一拒绝/放行,不再单独走审批
        assert!(!builtin_needs_approval("ui_invoke"));
        assert!(!builtin_needs_approval("ui_list_windows"));
    }

    #[test]
    fn builtin_approval_matrix_is_conservative() {
        for name in [
            "shell_exec",
            "fs_write",
            "fs_delete",
            "fs_restore",
            "fs_export_docx",
            "fs_export_xlsx",
            "zip_extract",
        ] {
            assert!(builtin_needs_approval(name), "`{name}` 必须审批");
        }
        for name in ["get_now", "fs_list", "fs_read", "file_extract", "zip_list"] {
            assert!(!builtin_needs_approval(name), "`{name}` 为只读,不应打扰");
        }
    }

    /// 文件工具只接受工作区相对路径:绝对路径与 `..` 必须在落盘前被拒绝。
    #[tokio::test]
    async fn fs_tools_reject_escaping_paths() {
        let (content, ok) = execute_builtin(
            "fs_read",
            &json!({ "path": "C:\\Windows\\win.ini" }),
            false,
            None,
            None,
        )
        .await;
        assert!(!ok);
        assert!(content.contains("相对路径"), "实际内容:{content}");

        let (_c, ok) = execute_builtin(
            "fs_write",
            &json!({ "path": "../escape.txt", "content": "x" }),
            false,
            None,
            None,
        )
        .await;
        assert!(!ok);
    }

    #[tokio::test]
    async fn shell_exec_rejects_non_whitelisted_command() {
        // 未通过白名单 → 必须返回失败,且不产生任何执行
        let (content, ok) =
            execute_builtin("shell_exec", &json!({ "command": "rm" }), false, None, None).await;
        assert!(!ok);
        assert!(content.contains("白名单") || content.contains("拒绝"));
    }

    /// H-1 回归:解释器必须被拒绝(即便参数不含任何元字符)。
    #[tokio::test]
    async fn shell_exec_rejects_interpreters() {
        let (content, ok) = execute_builtin(
            "shell_exec",
            &json!({ "command": "python", "args": ["-c", "print(1)"] }),
            false,
            None,
            None,
        )
        .await;
        assert!(!ok);
        assert!(content.contains("解释器"), "实际内容:{content}");
    }

    #[tokio::test]
    async fn shell_exec_requires_command_argument() {
        let (content, ok) = execute_builtin("shell_exec", &json!({}), false, None, None).await;
        assert!(!ok);
        assert!(content.contains("command"));
    }

    #[tokio::test]
    async fn shell_exec_denies_git_write_subcommand() {
        // git 只读白名单:commit 必须被拒绝
        let (_content, ok) = execute_builtin(
            "shell_exec",
            &json!({ "command": "git", "args": ["commit", "-m", "x"] }),
            false,
            None,
            None,
        )
        .await;
        assert!(!ok);
    }
}
