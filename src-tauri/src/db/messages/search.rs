//! messages 读操作:分支可见链列表 / 最近 N 条。

use rusqlite::Connection;

use super::{CHAIN_CTE, MessageRow, SELECT_COLS, row_to_message};

/// 列出分支可见的全部消息(沿链回溯,按时间正序)。
pub fn list_on(
    conn: &Connection,
    session_id: &str,
    branch_id: &str,
) -> Result<Vec<MessageRow>, String> {
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

/// 分页读取分支可见消息(按 id 升序返回,`before_id` = 更早一页的游标)。
/// 返回 (消息, 是否还有更早的消息)。
pub fn list_page_on(
    conn: &Connection,
    session_id: &str,
    branch_id: &str,
    before_id: Option<i64>,
    limit: usize,
) -> Result<(Vec<MessageRow>, bool), String> {
    let limit = limit.clamp(1, 500);
    let fetch = limit as i64 + 1; // 多取一条用于判断 has_more
    let sql = format!(
        "{} SELECT {} FROM messages WHERE id IN (SELECT id FROM chain) AND (?3 IS NULL OR id < ?3) ORDER BY id DESC LIMIT ?4",
        CHAIN_CTE, SELECT_COLS
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map(
            rusqlite::params![session_id, branch_id, before_id, fetch],
            row_to_message,
        )
        .map_err(|e| e.to_string())?;
    let mut collected = Vec::new();
    for r in &mut rows {
        collected.push(r.map_err(|e| e.to_string())?);
    }
    let has_more = collected.len() > limit;
    collected.truncate(limit);
    collected.reverse();
    Ok((collected, has_more))
}

/// 分页列出消息(全局连接包装)。
pub fn list_page(
    session_id: &str,
    branch_id: &str,
    before_id: Option<i64>,
    limit: usize,
) -> Result<(Vec<MessageRow>, bool), String> {
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    list_page_on(&conn, session_id, branch_id, before_id, limit)
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
        .query_map(
            rusqlite::params![session_id, branch_id, limit],
            row_to_message,
        )
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
