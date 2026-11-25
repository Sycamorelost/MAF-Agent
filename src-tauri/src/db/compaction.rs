//! 跨轮上下文压缩落库(P0)。
//!
//! 早期实现只在「单次请求」内用 LLM 摘要替换历史,请求结束即丢弃:
//! 下一轮前端再把原文发上来,重复计费且照样被硬裁剪。现在把摘要与
//! 「已覆盖到的消息 id」持久化,下一轮请求先按水位线丢弃旧消息、只带摘要。

use rusqlite::{Connection, OptionalExtension};

pub const DDL: &str = "
CREATE TABLE IF NOT EXISTS session_compactions (
  session_id     TEXT PRIMARY KEY,
  through_msg_id INTEGER NOT NULL DEFAULT 0,
  summary        TEXT NOT NULL DEFAULT '',
  updated        TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);
";

#[derive(Debug, Clone)]
pub struct CompactionRow {
    /// 已并入摘要的最大消息 id(含);该 id 及之前的历史可从请求里移除。
    pub through_msg_id: i64,
    pub summary: String,
}

pub fn get_on(conn: &Connection, session_id: &str) -> Result<Option<CompactionRow>, String> {
    conn.query_row(
        "SELECT through_msg_id, summary FROM session_compactions WHERE session_id=?1",
        [session_id],
        |r| {
            Ok(CompactionRow {
                through_msg_id: r.get(0)?,
                summary: r.get(1)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn get(session_id: &str) -> Result<Option<CompactionRow>, String> {
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    get_on(&conn, session_id)
}

/// 写入压缩摘要。水位线只前进不后退(同一轮可能多次压缩,取最大值)。
pub fn save_on(
    conn: &Connection,
    session_id: &str,
    through_msg_id: i64,
    summary: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO session_compactions(session_id, through_msg_id, summary, updated)
         VALUES(?1, ?2, ?3, datetime('now','localtime'))
         ON CONFLICT(session_id) DO UPDATE SET
           through_msg_id = MAX(session_compactions.through_msg_id, excluded.through_msg_id),
           summary = excluded.summary,
           updated = excluded.updated",
        rusqlite::params![session_id, through_msg_id, summary],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn save(session_id: &str, through_msg_id: i64, summary: &str) -> Result<(), String> {
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    save_on(&conn, session_id, through_msg_id, summary)
}

/// 会话历史被清空/重写时丢弃旧摘要,避免水位线指向已不存在的消息。
pub fn clear(session_id: &str) -> Result<(), String> {
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    clear_on(&conn, session_id)
}

pub fn clear_on(conn: &Connection, session_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM session_compactions WHERE session_id=?1",
        [session_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(DDL).unwrap();
        conn
    }

    #[test]
    fn save_is_monotonic_and_updates_summary() {
        let conn = setup();
        save_on(&conn, "s1", 10, "第一段").unwrap();
        save_on(&conn, "s1", 6, "旧水位不应回退").unwrap();
        let row = get_on(&conn, "s1").unwrap().unwrap();
        assert_eq!(row.through_msg_id, 10);
        assert_eq!(row.summary, "旧水位不应回退");
        save_on(&conn, "s1", 20, "第二段").unwrap();
        let row = get_on(&conn, "s1").unwrap().unwrap();
        assert_eq!(row.through_msg_id, 20);
        assert_eq!(row.summary, "第二段");
    }

    #[test]
    fn missing_row_is_none_and_clear_works() {
        let conn = setup();
        assert!(get_on(&conn, "s1").unwrap().is_none());
        save_on(&conn, "s1", 1, "x").unwrap();
        clear_on(&conn, "s1").unwrap();
        assert!(get_on(&conn, "s1").unwrap().is_none());
    }
}
