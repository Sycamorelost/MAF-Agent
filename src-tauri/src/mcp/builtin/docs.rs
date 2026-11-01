//! 任务清单与会话文档工具:todo_write / todo_read / doc_search / doc_read。
//!
//! 均为会话内状态,不触碰用户文件;审批层不拦(不改磁盘、不外发网络)。

use serde_json::Value;

/// 本模块负责的工具名。
pub(super) fn handles(name: &str) -> bool {
    matches!(name, "todo_write" | "todo_read" | "doc_search" | "doc_read")
}

pub(super) fn exec(name: &str, args: &Value, session_id: Option<&str>) -> (String, bool) {
    match name {
        "todo_write" => todo_write_exec(args, session_id),
        "todo_read" => todo_read_exec(session_id),
        "doc_search" => doc_search_exec(args, session_id),
        "doc_read" => doc_read_exec(args, session_id),
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
