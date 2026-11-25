//! memory.md 读写 / 版本 diff commands。
//!
//! 提炼(LLM summarize)已拆分到 `summarize.rs`,本文件只保留记忆文件本身的 CRUD 与版本管理。

use tauri::State;

use super::{BudgetStatus, MemoryDto, VersionItem};
use crate::memory::{self, watcher::WatcherState};

// ---------- memory commands ----------

/// 读取 memory.md(frontmatter + body)。
#[tauri::command]
pub async fn read_memory(session_id: String) -> Result<MemoryDto, String> {
    let (fm, body) = memory::read_memory(&session_id)?;
    Ok(MemoryDto {
        frontmatter: fm,
        body,
    })
}

/// 写入 memory.md(自动快照旧版到 memory.versions/ 并写表)。
#[tauri::command]
pub async fn write_memory(
    session_id: String,
    content: String,
    note: Option<String>,
) -> Result<(), String> {
    memory::write_memory(&session_id, &content, note.as_deref().unwrap_or("manual"))
}

/// 追加文本到 memory.md 指定节(缺省「资料」,节名限定 KNOWN_SECTIONS 白名单;自动补标题)。
/// note 统一记为 pin_message(消息钉入 / 来源存入 / 研究报告存入均属钉入行为)。
#[tauri::command]
pub async fn append_memory(
    session_id: String,
    text: String,
    section: Option<String>,
) -> Result<(), String> {
    let sec = resolve_section(section.as_deref())?;
    let (fm, body) = memory::read_memory(&session_id)?;
    let new_body = if body.trim().is_empty() {
        format!("# {sec}\n\n{}", text.trim())
    } else {
        append_to_section(&body, sec, &text)
    };
    memory::write_memory(
        &session_id,
        &memory::frontmatter::render(&fm, &new_body)?,
        "pin_message",
    )
}

/// 解析目标节名:空白缺省「资料」;非 KNOWN_SECTIONS 白名单直接报错。
pub(crate) fn resolve_section(section: Option<&str>) -> Result<&str, String> {
    let sec = section
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("资料");
    if memory::template::KNOWN_SECTIONS.contains(&sec) {
        Ok(sec)
    } else {
        Err(format!("未知记忆节: {sec}(仅支持 6 个标准节)"))
    }
}

/// 把 text 追加到 body 中第一个以 `# heading` 开头的一级/二级/三级标题下方。
/// 若不存在该标题则在末尾追加 `\n\n# heading\n\ntext`。
pub(crate) fn append_to_section(body: &str, heading: &str, text: &str) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut insert_idx = lines.len();
    let mut found = false;
    for (i, line) in lines.iter().enumerate() {
        if line.trim() == format!("# {}", heading)
            || line.trim() == format!("## {}", heading)
            || line.trim() == format!("### {}", heading)
        {
            insert_idx = i + 1;
            found = true;
        } else if found {
            // 找到下一节标题行时,在该行之前插入
            let trimmed = line.trim_start();
            if trimmed.starts_with("# ")
                || trimmed.starts_with("## ")
                || trimmed.starts_with("### ")
            {
                insert_idx = i;
                break;
            }
        }
    }
    if !found {
        // 标题不存在,直接追加
        let sep = if body.ends_with('\n') { "" } else { "\n\n" };
        return format!("{}{}# {}\n\n{}", body, sep, heading, text.trim());
    }
    let mut out = lines[..insert_idx].join("\n");
    out.push_str(&format!("\n\n{}", text.trim()));
    if insert_idx < lines.len() {
        out.push('\n');
        out.push_str(&lines[insert_idx..].join("\n"));
    }
    out
}

/// 手动触发一次版本快照,返回该版本的数字 id(可直接用于 rollback/read_version)。
#[tauri::command]
pub async fn snapshot_memory(session_id: String, note: Option<String>) -> Result<i64, String> {
    memory::snapshot_version(&session_id, note.as_deref().unwrap_or("manual"))
}

/// 列出会话的历史版本(倒序)。
#[tauri::command]
pub async fn list_memory_versions(session_id: String) -> Result<Vec<VersionItem>, String> {
    let rows = memory::list_versions(&session_id)?;
    Ok(rows
        .into_iter()
        .map(|(id, created, note)| VersionItem { id, created, note })
        .collect())
}

/// 回滚到指定版本。
#[tauri::command]
pub async fn rollback_memory(session_id: String, version_id: i64) -> Result<(), String> {
    memory::rollback_to(&session_id, version_id)
}

/// 读取指定历史版本的完整快照内容(迭代 D3.3 版本 diff 用)。
#[tauri::command]
pub async fn read_memory_version(session_id: String, version_id: i64) -> Result<String, String> {
    memory::read_version(&session_id, version_id)
}

#[tauri::command]
pub async fn memory_budget_status(session_id: String) -> Result<BudgetStatus, String> {
    let (_fm, body) = memory::read_memory(&session_id)?;
    let tokens = memory::budget::estimate_tokens(&body);
    Ok(BudgetStatus {
        tokens,
        limit: memory::budget::MEMORY_BUDGET_TOKENS,
        over: tokens > memory::budget::MEMORY_BUDGET_TOKENS,
    })
}

/// 开始监听 memory.md 外部改动(前端拿到 `memory_changed` 事件)。
#[tauri::command]
pub async fn watch_memory(
    session_id: String,
    app: tauri::AppHandle,
    state: State<'_, WatcherState>,
) -> Result<(), String> {
    memory::validate_id(&session_id)?;
    state.registry.watch(app, &session_id)
}

/// 停止监听。
#[tauri::command]
pub async fn unwatch_memory(
    session_id: String,
    state: State<'_, WatcherState>,
) -> Result<(), String> {
    memory::validate_id(&session_id)?;
    state.registry.unwatch(&session_id)
}

/// 判断是否需要提炼(规则:距上次 N 轮 / token 超阈值)。
#[tauri::command]
pub async fn needs_summarize(
    session_id: String,
    every_n_rounds: Option<i64>,
    token_threshold: Option<i64>,
) -> Result<bool, String> {
    memory::needs_summarize(
        &session_id,
        every_n_rounds.unwrap_or(2),
        token_threshold.unwrap_or(8000),
    )
}
