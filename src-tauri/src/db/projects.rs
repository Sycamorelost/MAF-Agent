//! projects 表的 CRUD(P8.1 项目化升级)。
//!
//! 参考 Cursor Projects 设计:
//! - Project = 用户从磁盘选一个本地目录(path) + 名字(name)
//! - 可选 model_override(项目级默认模型,覆盖全局)
//! - 可选 shared_context(项目级共享系统提示,所有会话共享)
//! - 一个 Project 下可以归入多个任务会话(sessions.project_id)
//!
//! Agent 写文件时:如果 session.project_id 有 path 且目录存在,
//! 用它做 workspace_root;否则回退全局 workspace_root。

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::db;

/// 项目行(path = 本地磁盘目录绝对路径)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub path: String,
    pub model_override: Option<String>,
    pub shared_context: Option<String>,
    pub sort: i64,
    pub created: String,
    pub updated: String,
}

fn row_to_project(r: &rusqlite::Row) -> rusqlite::Result<ProjectRow> {
    Ok(ProjectRow {
        id: r.get(0)?,
        name: r.get(1)?,
        path: r.get(2)?,
        model_override: r.get(3)?,
        shared_context: r.get(4)?,
        sort: r.get(5)?,
        created: r.get(6)?,
        updated: r.get(7)?,
    })
}

pub fn list() -> Result<Vec<ProjectRow>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, path, model_override, shared_context, sort, created, updated \
             FROM projects ORDER BY sort ASC, created ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_project)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// 从路径推断默认项目名(取最后一段目录名)。
pub fn name_from_path(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(['\\', '/']);
    if trimmed.is_empty() {
        return "新项目".to_string();
    }
    trimmed
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(trimmed)
        .to_string()
}

/// 创建项目 —— path 必须是绝对路径且目录存在。
/// name 为空时自动从 path 推断。
pub fn create(name: &str, path: &str) -> Result<ProjectRow, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("项目路径不能为空".into());
    }

    // Windows UNC 路径前缀清理(Tauri dialog 可能返回 \\?\C:\...)
    let path = path
        .strip_prefix(r"\\?\")
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.to_string());

    // Windows 绝对路径校验
    let is_abs = std::path::Path::new(&path).is_absolute()
        || (path.len() >= 2 && path.chars().nth(1) == Some(':'))
        || path.starts_with(r"\\");
    if !is_abs {
        log::warn!("project_create: path 不是绝对路径: {path}");
        return Err(format!("路径不是绝对路径: {path}"));
    }

    // 目录必须实际存在于磁盘上
    if !std::path::Path::new(&path).is_dir() {
        log::warn!("project_create: 路径不是有效目录: {path}");
        return Err(format!("路径不是有效目录: {path}"));
    }

    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let id: String = {
        let mut buf = [0u8; 4];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut buf);
        buf.iter().map(|b| format!("{:02x}", b)).collect()
    };
    let now = chrono::Local::now().to_rfc3339();
    let final_name = if name.trim().is_empty() {
        name_from_path(&path)
    } else {
        name.trim().to_string()
    };
    let sort: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort), -1) + 1 FROM projects",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    conn.execute(
        "INSERT INTO projects(id, name, path, model_override, shared_context, sort, created, updated) \
         VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?5, ?6)",
        rusqlite::params![id, final_name, path, sort, now, now],
    )
    .map_err(|e| {
        log::error!("project_create SQL 失败: {e}, path={path}");
        format!("数据库插入失败: {e}")
    })?;

    log::info!("project_create: id={id}, name={final_name}, path={path}");
    Ok(ProjectRow {
        id,
        name: final_name,
        path,
        model_override: None,
        shared_context: None,
        sort,
        created: now.clone(),
        updated: now,
    })
}

pub fn rename(id: &str, name: &str) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE projects SET name=?1, updated=?2 WHERE id=?3",
        rusqlite::params![name.trim(), chrono::Local::now().to_rfc3339(), id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn update_path(id: &str, path: &str) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE projects SET path=?1, updated=?2 WHERE id=?3",
        rusqlite::params![path, chrono::Local::now().to_rfc3339(), id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn update_model_override(id: &str, model: Option<&str>) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE projects SET model_override=?1, updated=?2 WHERE id=?3",
        rusqlite::params![model, chrono::Local::now().to_rfc3339(), id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn update_shared_context(id: &str, ctx: Option<&str>) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE projects SET shared_context=?1, updated=?2 WHERE id=?3",
        rusqlite::params![ctx, chrono::Local::now().to_rfc3339(), id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 删除项目,同时把该项目下所有会话的 project_id 置为 NULL(归入「未分类」)。
pub fn delete(id: &str) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE sessions SET project_id=NULL WHERE project_id=?1", [id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM projects WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn update_sort(id: &str, sort: i64) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE projects SET sort=?1, updated=?2 WHERE id=?3",
        rusqlite::params![sort, chrono::Local::now().to_rfc3339(), id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get(id: &str) -> Result<Option<ProjectRow>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, path, model_override, shared_context, sort, created, updated \
             FROM projects WHERE id=?1",
        )
        .map_err(|e| e.to_string())?;
    let row = stmt
        .query_row([id], row_to_project)
        .ok();
    Ok(row)
}

/// 构建项目工作区守卫提示词(追加到 system_prompt 末尾)。
pub fn build_project_guard_prompt(project: &ProjectRow) -> String {
    let ctx = project.shared_context.as_deref().unwrap_or("");
    let ctx_display = if ctx.trim().is_empty() { "(无)" } else { ctx.trim() };
    format!(
        "\n\n## 当前项目工作区\n\
         - 项目名: {name}\n\
         - 工作区根目录: {path}\n\
         - 共享上下文: {ctx}\n\n\
         你是一个专业的代码助手,所有文件读写、命令执行、搜索操作都以该工作区根目录为基准路径。\n\
         使用 MCP 工具时,path 参数直接写工作区内的相对路径,不要写绝对路径。\n\
         如果任务涉及工作区外的文件,先征得用户确认。",
        name = project.name,
        path = project.path,
        ctx = ctx_display,
    )
}

/// 「当前项目工作区」守卫标记 —— system_prompt 中以此为锚点定位 guard 片段。
pub const GUARD_MARKER: &str = "## 当前项目工作区";

/// 追加项目 guard 到会话 system_prompt(幂等:已有 marker 则跳过)。
pub fn append_project_guard(session_id: &str, project: &ProjectRow) -> Result<(), String> {
    let session = crate::db::sessions::get(session_id)?
        .ok_or_else(|| format!("session 不存在: {session_id}"))?;
    let existing = session.system_prompt.clone().unwrap_or_default();
    if existing.contains(GUARD_MARKER) {
        return Ok(()); // 已注入,跳过(幂等)
    }
    let guard = build_project_guard_prompt(project);
    let new_sp = if existing.is_empty() {
        guard
    } else {
        format!("{existing}{guard}")
    };
    crate::db::sessions::set_system_prompt(session_id, Some(&new_sp))?;
    Ok(())
}

/// 从会话 system_prompt 中移除项目 guard 片段(会话移出项目时调用)。
pub fn remove_project_guard(session_id: &str) -> Result<(), String> {
    let session = crate::db::sessions::get(session_id)?
        .ok_or_else(|| format!("session 不存在: {session_id}"))?;
    let existing = session.system_prompt.clone().unwrap_or_default();
    if !existing.contains(GUARD_MARKER) {
        return Ok(()); // 没有 guard,无需清除
    }
    // 从 GUARD_MARKER 开头截断
    let trimmed = match existing.find(GUARD_MARKER) {
        Some(idx) => existing[..idx].trim_end().to_string(),
        None => existing,
    };
    let new_sp = if trimmed.is_empty() { None } else { Some(trimmed.as_str()) };
    crate::db::sessions::set_system_prompt(session_id, new_sp)?;
    Ok(())
}
