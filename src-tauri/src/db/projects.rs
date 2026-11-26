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
    /// 是否已归档(V42):归档项目会级联归档其会话,项目与数据全部保留
    pub archived: bool,
}

pub(crate) fn row_to_project(r: &rusqlite::Row) -> rusqlite::Result<ProjectRow> {
    Ok(ProjectRow {
        id: r.get(0)?,
        name: r.get(1)?,
        path: r.get(2)?,
        model_override: r.get(3)?,
        shared_context: r.get(4)?,
        sort: r.get(5)?,
        created: r.get(6)?,
        updated: r.get(7)?,
        archived: r.get::<_, i64>(8)? != 0,
    })
}

pub(crate) const PROJECT_COLS: &str =
    "id, name, path, model_override, shared_context, sort, created, updated, archived";

pub fn list() -> Result<Vec<ProjectRow>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {PROJECT_COLS} FROM projects WHERE archived=0 ORDER BY sort ASC, created ASC"
        ))
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

/// 规范化并校验工作文件夹:清理 Windows UNC 前缀,必须是绝对路径且目录存在。
fn normalize_work_dir(path: &str) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("项目路径不能为空".into());
    }

    // Windows UNC 路径前缀清理(Tauri dialog 可能返回 \\?\C:\...)
    let path = path
        .strip_prefix(r"\\?\")
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.to_string());

    // 绝对路径校验(Windows 盘符 / POSIX 根 / UNC)
    let is_abs = std::path::Path::new(&path).is_absolute()
        || (path.len() >= 2 && path.chars().nth(1) == Some(':'))
        || path.starts_with(r"\\");
    if !is_abs {
        log::warn!("project path 不是绝对路径: {path}");
        return Err(format!("路径不是绝对路径: {path}"));
    }

    // 目录必须实际存在于磁盘上
    if !std::path::Path::new(&path).is_dir() {
        log::warn!("project path 不是有效目录: {path}");
        return Err(format!("路径不是有效目录: {path}"));
    }

    Ok(path)
}

/// 创建项目 —— path 必须是绝对路径且目录存在。
/// name 为空时自动从 path 推断。
pub fn create(name: &str, path: &str) -> Result<ProjectRow, String> {
    let path = normalize_work_dir(path)?;

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
        archived: false,
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
    let path = normalize_work_dir(path)?;
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let changed = conn
        .execute(
            "UPDATE projects SET path=?1, updated=?2 WHERE id=?3",
            rusqlite::params![path, chrono::Local::now().to_rfc3339(), id],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err(format!("项目不存在: {id}"));
    }
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
    conn.execute(
        "UPDATE sessions SET project_id=NULL WHERE project_id=?1",
        [id],
    )
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
        .prepare(&format!("SELECT {PROJECT_COLS} FROM projects WHERE id=?1"))
        .map_err(|e| e.to_string())?;
    let row = stmt.query_row([id], row_to_project).ok();
    Ok(row)
}

/// 构建项目工作区守卫提示词(追加到 system_prompt 末尾)。
pub fn build_project_guard_prompt(project: &ProjectRow) -> String {
    build_guard(
        &project.name,
        &project.path,
        project.shared_context.as_deref(),
    )
}

fn build_guard(name: &str, path: &str, shared_context: Option<&str>) -> String {
    let ctx = shared_context.unwrap_or("");
    let ctx_display = if ctx.trim().is_empty() {
        "(无)"
    } else {
        ctx.trim()
    };
    format!(
        "\n\n## 当前项目工作区\n\
         - 项目名: {name}\n\
         - 工作区根目录: {path}\n\
         - 共享上下文: {ctx}\n\n\
         你是一个专业的代码助手,所有文件读写、命令执行、搜索操作都以该工作区根目录为基准路径。\n\
         使用 MCP 工具时,path 参数直接写工作区内的相对路径,不要写绝对路径。\n\
         如果任务涉及工作区外的文件,先征得用户确认。",
        name = name,
        path = path,
        ctx = ctx_display,
    )
}

/// 请求时实时构建项目 guard(P1-8):直接读项目当前值,编辑项目后已绑定会话立即生效,
/// 不再依赖「绑定时」写进 sessions.system_prompt 的快照。
pub fn guard_for_session(session_id: &str) -> Option<String> {
    use rusqlite::OptionalExtension;
    let conn = crate::db::conn().lock().ok()?;
    let (name, path, ctx) = conn
        .query_row(
            "SELECT p.name, p.path, p.shared_context FROM sessions s \
             JOIN projects p ON p.id = s.project_id WHERE s.id=?1",
            [session_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .ok()??;
    Some(build_guard(&name, &path, ctx.as_deref()))
}

/// 剥掉 system 文本里绑定时写入的旧 guard 快照(请求时由 `guard_for_session` 重建)。
pub fn strip_guard(text: &str) -> String {
    match text.find(GUARD_MARKER) {
        Some(idx) => text[..idx].trim_end().to_string(),
        None => text.to_string(),
    }
}

/// 项目级模型覆盖,格式 `provider_id::model_id`(P1-8:会话未显式选模型时的回退)。
pub fn model_override_for_session(
    session_id: &str,
) -> Result<Option<(String, String)>, String> {
    use rusqlite::OptionalExtension;
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let raw: Option<Option<String>> = conn
        .query_row(
            "SELECT p.model_override FROM sessions s JOIN projects p ON p.id = s.project_id \
             WHERE s.id=?1",
            [session_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(raw.flatten().as_deref().and_then(parse_model_override))
}

/// 解析 `provider_id::model_id`;格式不合法返回 None(宁可不覆盖也不要选错模型)。
pub fn parse_model_override(raw: &str) -> Option<(String, String)> {
    let (p, m) = raw.trim().split_once("::")?;
    let (p, m) = (p.trim(), m.trim());
    (!p.is_empty() && !m.is_empty()).then(|| (p.to_string(), m.to_string()))
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
    let new_sp = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.as_str())
    };
    crate::db::sessions::set_system_prompt(session_id, new_sp)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{GUARD_MARKER, parse_model_override, strip_guard};

    #[test]
    fn parse_model_override_needs_both_parts() {
        assert_eq!(
            parse_model_override("deepseek::deepseek-chat"),
            Some(("deepseek".into(), "deepseek-chat".into()))
        );
        assert_eq!(parse_model_override(" deepseek :: v4 "), Some((
            "deepseek".into(),
            "v4".into()
        )));
        assert_eq!(parse_model_override("deepseek-chat"), None);
        assert_eq!(parse_model_override("::x"), None);
        assert_eq!(parse_model_override("x::"), None);
    }

    #[test]
    fn strip_guard_keeps_user_prompt_only() {
        let text = format!("我的自定义提示\n\n{GUARD_MARKER}\n- 项目名: x");
        assert_eq!(strip_guard(&text), "我的自定义提示");
        assert_eq!(strip_guard("无项目"), "无项目");
    }
}
