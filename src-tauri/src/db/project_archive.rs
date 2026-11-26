//! 项目归档(V42):已归档项目查询与标记。
//!
//! 归档项目时由命令层级联归档其会话;这里只负责项目自身的标记与查询。

use crate::db::projects::{self, ProjectRow, PROJECT_COLS};

/// 已归档项目(归档视图用)。
pub fn list_archived() -> Result<Vec<ProjectRow>, String> {
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {PROJECT_COLS} FROM projects WHERE archived=1 ORDER BY sort ASC, created ASC"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], projects::row_to_project)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// 设置项目归档标记(只改标记,不动 sort / updated / 会话归属)。
pub fn set_archived(id: &str, archived: bool) -> Result<(), String> {
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE projects SET archived=?1 WHERE id=?2",
        rusqlite::params![archived as i64, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
