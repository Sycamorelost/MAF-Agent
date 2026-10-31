//! 内置工具路由:能力作用域映射、白名单判定与执行。
//!
//! 从 dispatcher 拆出,避免单文件过长;审批仍由 dispatcher 负责,本模块只做
//! 「是不是内置工具 / 要不要审批 / 作用域是什么 / 怎么执行」。

use serde_json::Value;

use super::approval::ApprovalRisk;
use super::capability::Scope;

/// 工具 → 能力作用域的映射。外部 MCP 工具按风险粗分:只读走 Net,其余走 Exec。
///
/// 内置工具的作用域从 schema 定义的「只读/写入」分组派生(见
/// `commands::mcp_builtin_tools`),仅 `shell_exec`(执行)、`delegate`(网络委派)
/// 与 `ui_*`(GUI 自动化)三个例外单独标注,避免名单重复维护。
pub(crate) fn capability_scope(server: &str, tool: &str, risk: ApprovalRisk) -> Scope {
    if server == "builtin" {
        return match tool {
            "shell_exec" => Scope::Exec,
            "delegate" => Scope::Net,
            t if t.starts_with("ui_") => Scope::Ui,
            t if crate::commands::mcp_builtin_tools::WRITE_TOOL_NAMES.contains(t) => Scope::FsWrite,
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
    crate::commands::mcp_builtin_tools::ALL_BUILTIN_TOOL_NAMES.contains(name)
        || crate::gui::tools::is_gui_tool(name)
}

/// 内置工具是否需要人工审批。写入/执行类工具(来自 `WRITE_TOOL_NAMES`)一律审批;
/// 只读工具免打扰。GUI 工具不在此列(由「完全信任」总开关在执行期直接拒绝)。
pub(crate) fn builtin_needs_approval(name: &str) -> bool {
    crate::commands::mcp_builtin_tools::WRITE_TOOL_NAMES.contains(name)
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
    // 审计日志:查一次 project_id,避免每条日志都查
    let project_id = session_id.and_then(|sid| {
        crate::db::sessions::get(sid)
            .ok()
            .flatten()
            .and_then(|s| s.project_id)
    });

    // GUI 自动化:UIA / SendInput 都是阻塞调用,隔离到阻塞线程池,避免卡住 tokio worker
    if crate::gui::tools::is_gui_tool(name) {
        let tool = name.to_string();
        let a = args.clone();
        return match tokio::task::spawn_blocking(move || crate::gui::tools::execute(&tool, &a))
            .await
        {
            Ok(r) => {
                let _ = write_log(session_id, project_id.as_deref(), name, args, &r.0, r.1);
                r
            }
            Err(e) => {
                let msg = format!("GUI 工具执行线程异常:{e}");
                let _ = write_log(session_id, project_id.as_deref(), name, args, &msg, false);
                (msg, false)
            }
        };
    }

    // 先处理会 return 的 early-exit 分支(shell_exec 参数校验)
    if name == "shell_exec" {
        let cmd = args
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if cmd.is_empty() {
            let msg = "缺少 command 参数\n[提示] command 填程序名(如 git、ls、where),参数放进 args 数组";
            let _ = write_log(session_id, project_id.as_deref(), name, args, msg, false);
            return (msg.into(), false);
        }
    }

    let (content, ok) = match name {
        "get_now" => match crate::commands::mcp::get_now(None).await {
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
        "doc_search" => doc_search_exec(args, session_id),
        "doc_read" => doc_read_exec(args, session_id),
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
            // args 容错:字符串/数字不再静默丢弃(见 shell::parse_shell_args)
            let (argv, warn) = crate::mcp::shell::parse_shell_args(args.get("args"));
            let cwd = args
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            // D8:完全信任模式下,高风险命令(格式化/删根/注册表等)仍需模型侧二次确认。
            // confirm=true 表示已征得用户同意;否则返回 DANGEROUS_CONFIRM 提示,模型应转述给用户。
            if trust_shell && let Some(pattern) = crate::mcp::shell::dangerous_pattern(cmd, &argv) {
                let confirm = args
                    .get("confirm")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if !confirm {
                    return (
                        format!(
                            "DANGEROUS_CONFIRM:检测到高风险命令片段 `{}`(可能导致数据丢失或系统损坏)。\n\
如需执行,请向用户确认后以 confirm=true 重试。",
                            pattern.trim()
                        ),
                        false,
                    );
                }
            }
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
            match crate::fs::text::list(path, session_id) {
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
            // C3/F6:大文件分页读取
            let offset = args
                .get("offset")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            match crate::fs::text::read_text(path, offset, max, session_id) {
                Ok(text) => (text, true),
                Err(e) => (format!("读取失败:{e}"), false),
            }
        }
        "fs_grep" => {
            let pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if pattern.is_empty() {
                return ("缺少 pattern 参数".into(), false);
            }
            let rel = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
            match crate::fs::text::grep(pattern, rel, limit, session_id) {
                Ok(v) if v.is_empty() => ("未命中任何文本行".into(), true),
                Ok(v) => (v.join("\n"), true),
                Err(e) => (format!("检索失败:{e}"), false),
            }
        }
        "fs_write" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            match crate::fs::text::write_text(path, content, session_id) {
                Ok(r) => (serde_json::to_string(&r).unwrap_or_default(), true),
                Err(e) => (format!("写入失败:{e}"), false),
            }
        }
        "fs_delete" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            match crate::fs::text::delete(path, session_id) {
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
            let profile = args.get("profile").and_then(|v| v.as_str());
            let toc = args.get("toc").and_then(|v| v.as_bool());
            let meta: crate::fs::docgen::DocMeta = args
                .get("meta")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let req = crate::fs::docgen::DocRequest::from_parts(title, body, profile, toc, meta);
            match crate::fs::text::export_docx_ext(path, &req, session_id) {
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
            match crate::fs::text::export_xlsx(path, &sheets, session_id) {
                Ok(r) => (serde_json::to_string(&r).unwrap_or_default(), true),
                Err(e) => (format!("导出 xlsx 失败:{e}"), false),
            }
        }
        "fs_restore" => {
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if id.is_empty() {
                return ("缺少 id 参数".into(), false);
            }
            let root = match crate::fs::text::root_for_session(session_id) {
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
            match crate::fs::formats::extract(path, session_id) {
                Ok(text) => (text, true),
                Err(e) => (format!("提取失败:{e}"), false),
            }
        }
        "zip_list" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            match crate::fs::formats::zip_list(path, session_id) {
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
            match crate::fs::formats::zip_extract(path, dest, session_id) {
                Ok(n) => (format!("解包完成,共 {n} 个文件"), true),
                Err(e) => (format!("解包失败:{e}"), false),
            }
        }
        _ => ("未知内置工具".into(), false),
    };

    // 统一审计日志(覆盖所有正常返回;early-return 分支 GUI 已在上面写)
    let _ = write_log(session_id, project_id.as_deref(), name, args, &content, ok);
    (content, ok)
}

/// 审计日志写入助手:从 args 里提取 path/cmd 作为 target_path,截断 args_preview 到 200 字符。
fn write_log(
    session_id: Option<&str>,
    project_id: Option<&str>,
    tool_name: &str,
    args: &Value,
    content: &str,
    ok: bool,
) {
    let Some(sid) = session_id else {
        return; // 无会话上下文时不写
    };
    let target_path = args
        .get("path")
        .and_then(|v| v.as_str())
        .or_else(|| args.get("dest").and_then(|v| v.as_str()))
        .or_else(|| args.get("command").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let args_preview = serde_json::to_string(args)
        .ok()
        .map(|s| if s.len() > 200 { format!("{}…", &s[..200]) } else { s });
    let result = if ok { "ok".to_string() } else { content.to_string() };
    crate::db::tool_log::record(
        sid,
        project_id,
        tool_name,
        target_path.as_deref(),
        args_preview.as_deref(),
        &result,
    );
}

/// todo_write 入参(模型提交完整列表;id 可省略,缺省按序编号)。
#[derive(Debug, serde::Deserialize)]
struct TodoItemIn {
    id: Option<String>,
    content: String,
    status: Option<String>,
    priority: Option<String>,
    /// 关联的目标级任务 id(V25;可选,空/无效值按未关联存储)
    task_id: Option<String>,
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
            task_id: r.task_id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
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
                "任务清单已更新:共 {} 项(进行中 {},待办 {},已完成 {},已取消 {})。\
                 记住:每完成一步必须立即再次调用 todo_write 更新状态(完成项→completed,下一项→in_progress),\
                 不要等全部做完才更新,用户在面板上实时跟踪该进度。",
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

/// doc_search:会话内文档分块检索(C2)。
fn doc_search_exec(args: &Value, session_id: Option<&str>) -> (String, bool) {
    let Some(sid) = session_id else {
        return ("文档检索需要会话上下文,当前不可用".into(), false);
    };
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if query.is_empty() {
        return ("缺少 query 参数".into(), false);
    }
    let hint = args.get("doc").and_then(|v| v.as_str());
    // 检索策略随会话预设(P7):limit 缺省取预设 top_k
    let pol = crate::presets::retrieval_for_session(sid);
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(pol.top_k as u64)
        .clamp(1, 20) as usize;
    let conn = match crate::db::conn().lock() {
        Ok(c) => c,
        Err(e) => return (format!("db lock 失败: {e}"), false),
    };
    match crate::doc::search_with_policy(&conn, sid, query, hint, limit, &pol) {
        Ok(out) if out.hits.is_empty() => (
            format!(
                "未命中:会话内共 {} 块,没有匹配「{query}」的内容。可换关键词,或用 doc_read 按块浏览。",
                out.scanned
            ),
            true,
        ),
        Ok(out) => {
            let mut s = format!("命中 {} 段(共扫描 {} 块):\n", out.hits.len(), out.scanned);
            for h in &out.hits {
                s.push_str(&format!("\n[{} · 第 {} 块", h.doc_name, h.ord + 1));
                if let Some(p) = h.page {
                    s.push_str(&format!(" · 第 {p} 页"));
                }
                if !h.section.is_empty() {
                    s.push_str(&format!(" · {}", h.section));
                }
                s.push_str(&format!("]\n{}\n", h.snippet));
            }
            s.push_str("\n需要完整上下文时用 doc_read(doc=文档名, start=块序号-1)。");
            (s, true)
        }
        Err(e) => (format!("文档检索失败:{e}"), false),
    }
}

/// doc_read:按块序号回看原文(C2)。
fn doc_read_exec(args: &Value, session_id: Option<&str>) -> (String, bool) {
    let Some(sid) = session_id else {
        return ("文档读取需要会话上下文,当前不可用".into(), false);
    };
    let hint = args.get("doc").and_then(|v| v.as_str());
    let start = args.get("start").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let count = args
        .get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(3)
        .clamp(1, 20) as usize;
    let conn = match crate::db::conn().lock() {
        Ok(c) => c,
        Err(e) => return (format!("db lock 失败: {e}"), false),
    };
    match crate::doc::read_range(&conn, sid, hint, start, count) {
        Ok(text) => (text, true),
        Err(e) => (format!("文档读取失败:{e}"), false),
    }
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
    use crate::commands::mcp_builtin_tools::ALL_BUILTIN_TOOL_NAMES;
    use serde_json::json;

    /// R4 工具注册表单源:schema 定义的每个工具都必须有执行分支。
    /// 新增工具时请同步更新此处清单与 `execute_builtin` 的 match。
    #[test]
    fn every_schema_tool_has_an_execution_handler() {
        // `execute_builtin` 的 match 兜底返回「未知内置工具」;
        // 逐个用空参数调用,只要不命中兜底即说明有对应分支。
        for name in ALL_BUILTIN_TOOL_NAMES.iter() {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let (content, _ok) = rt.block_on(execute_builtin(name, &json!({}), false, None, None));
            assert!(
                !content.contains("未知内置工具"),
                "工具 `{name}` 已在 schema 中定义,但 `execute_builtin` 缺少对应执行分支"
            );
        }
    }

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

    /// 文件工具:绝对路径可访问工作区外(全盘),相对路径 `..` 同样不再拦截(无沙箱)。
    #[tokio::test]
    async fn fs_tools_accept_absolute_paths_outside_workspace() {
        let outside = std::env::temp_dir().join(format!("maf-abs-{}.txt", std::process::id()));
        std::fs::write(&outside, "absolute-ok").unwrap();
        let (content, ok) = execute_builtin(
            "fs_read",
            &json!({ "path": outside.to_str().unwrap() }),
            false,
            None,
            None,
        )
        .await;
        assert!(ok, "绝对路径应可读取,实际内容:{content}");
        assert!(content.contains("absolute-ok"));

        // 相对路径 `..`:不再要求留在工作区内
        let (content, ok) =
            execute_builtin("fs_list", &json!({ "path": ".." }), false, None, None).await;
        assert!(ok, "`..` 应可列出,实际内容:{content}");
        std::fs::remove_file(&outside).ok();
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
