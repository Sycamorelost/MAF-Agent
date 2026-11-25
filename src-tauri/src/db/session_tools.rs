//! 按需工具加载状态:记录会话内显式加载过的内置工具名。
//!
//! 默认只下发核心工具;模型调用 `tool_load` 后把工具名写入本表,
//! 后续轮次与下次对话都按此表补充下发,无需重复加载。

use rusqlite::{Connection, params};

pub(crate) const DDL: &str = "
CREATE TABLE IF NOT EXISTS session_tools (
  session_id TEXT NOT NULL,
  tool_name  TEXT NOT NULL,
  created    TEXT NOT NULL DEFAULT (datetime('now','localtime')),
  PRIMARY KEY (session_id, tool_name)
);
CREATE INDEX IF NOT EXISTS idx_session_tools_session ON session_tools(session_id);
";

/// 按会话列出已加载工具名(按加载顺序)。
pub fn list_on(conn: &Connection, session_id: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT tool_name FROM session_tools WHERE session_id=?1 ORDER BY rowid")?;
    let rows = stmt.query_map(params![session_id], |r| r.get::<_, String>(0))?;
    rows.collect()
}

pub fn list(session_id: &str) -> Vec<String> {
    if !crate::db::is_initialized() {
        return Vec::new();
    }
    let Ok(conn) = crate::db::conn().lock() else {
        return Vec::new();
    };
    list_on(&conn, session_id).unwrap_or_default()
}

/// 批量登记(已存在则忽略),返回新增条数。
pub fn add_many_on(
    conn: &Connection,
    session_id: &str,
    names: &[String],
) -> rusqlite::Result<usize> {
    let mut added = 0;
    for name in names {
        added += conn.execute(
            "INSERT OR IGNORE INTO session_tools(session_id, tool_name) VALUES(?1, ?2)",
            params![session_id, name],
        )?;
    }
    Ok(added)
}

pub fn add_many(session_id: &str, names: &[String]) -> usize {
    if !crate::db::is_initialized() {
        return 0;
    }
    let Ok(conn) = crate::db::conn().lock() else {
        return 0;
    };
    add_many_on(&conn, session_id, names).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, title, created, updated) VALUES('s1','新会话','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn add_list_dedupe_and_clear() {
        let conn = setup();
        let names = vec!["doc_generate".to_string(), "slides_export".to_string()];
        assert_eq!(add_many_on(&conn, "s1", &names).unwrap(), 2);
        // 重复加载不新增行
        assert_eq!(add_many_on(&conn, "s1", &names).unwrap(), 0);
        assert_eq!(
            list_on(&conn, "s1").unwrap(),
            vec!["doc_generate", "slides_export"]
        );
        let n = conn
            .execute("DELETE FROM session_tools WHERE session_id='s1'", [])
            .unwrap();
        assert_eq!(n, 2);
        assert!(list_on(&conn, "s1").unwrap().is_empty());
    }
}
