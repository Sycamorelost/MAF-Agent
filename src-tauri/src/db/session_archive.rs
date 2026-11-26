//! 会话归档(V41):已归档列表查询与批量标记。
//!
//! 归档只改 `sessions.archived` 标记,不改 updated、不动消息与记忆,
//! 因此恢复后一切属性原样保留。列表主查询在 `db::sessions::list`。

use crate::db::sessions::{self, SessionRow, SELECT_COLS};

/// 已归档会话(侧栏「已归档」分组数据源)。
///
/// 排除「因项目归档而被级联归档」的会话:它们跟着项目一起展示/恢复,
/// 避免同一批内容在归档页出现两次,也避免单独恢复后落回仍被归档的项目里看不见。
pub fn list_archived() -> Result<Vec<SessionRow>, String> {
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM sessions WHERE archived=1 \
             AND (project_id IS NULL OR project_id NOT IN \
                  (SELECT id FROM projects WHERE archived=1)) \
             ORDER BY updated DESC"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], sessions::row_to_session)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// 项目下全部会话 id(含已归档与未归档),归档项目时级联使用。
pub fn session_ids_in_project(project_id: &str) -> Result<Vec<String>, String> {
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id FROM sessions WHERE project_id=?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([project_id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// 批量设置归档状态:只改 archived 标记,不动 updated 与任何其它属性。
pub fn set_archived_many(ids: &[String], archived: bool) -> Result<(), String> {
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("UPDATE sessions SET archived=?1 WHERE id=?2")
        .map_err(|e| e.to_string())?;
    for id in ids {
        stmt.execute(rusqlite::params![archived as i64, id])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
