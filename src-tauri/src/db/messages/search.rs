//! messages 读操作:分支可见链列表 / 最近 N 条。

use rusqlite::Connection;

use super::{row_to_message, MessageRow, CHAIN_CTE, SELECT_COLS};

/// 列出分支可见的全部消息(沿链回溯,按时间正序)。
pub fn list_on(conn: &Connection, session_id: &str, branch_id: &str) -> Result<Vec<MessageRow>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "{} SELECT {} FROM messages WHERE id IN (SELECT id FROM chain) ORDER BY id ASC",
            CHAIN_CTE, SELECT_COLS
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![session_id, branch_id], row_to_message)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// 列出分支可见的全部消息(全局连接包装)。
pub fn list(session_id: &str, branch_id: &str) -> Result<Vec<MessageRow>, String> {
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    list_on(&conn, session_id, branch_id)
}

/// 分支可见链的最近 N 条消息(用于拼接 LLM 上下文 / 记忆提炼)。
pub fn recent_on(
    conn: &Connection,
    session_id: &str,
    branch_id: &str,
    limit: i64,
) -> Result<Vec<MessageRow>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "{} SELECT {} FROM messages WHERE id IN (SELECT id FROM chain) ORDER BY id DESC LIMIT ?3",
            CHAIN_CTE, SELECT_COLS
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![session_id, branch_id, limit], row_to_message)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    out.reverse(); // 反回正序
    Ok(out)
}

/// 当前激活分支可见链的最近 N 条消息(全局连接包装)。
pub fn recent(session_id: &str, limit: i64) -> Result<Vec<MessageRow>, String> {
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    let branch = crate::db::sessions::active_branch_on(&conn, session_id)?;
    recent_on(&conn, session_id, &branch, limit)
}
