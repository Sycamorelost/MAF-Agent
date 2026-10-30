//! 任务清单(todo)存储:每会话一份,模型通过 `todo_write` 整表替换式写入。
//! 设计对齐 harness 的 TodoWrite:不追加、不细改,每次提交完整列表,保证模型与 UI 看到的状态一致。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// 单条任务。status / priority 的合法值在 builtin 层校验,这里只做存储。
/// task_id(V25)可空:关联到 tasks 表的目标级任务;旧数据为 NULL,按会话维度工作。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    /// pending | in_progress | completed | cancelled
    pub status: String,
    /// high | medium | low
    pub priority: String,
    #[serde(default)]
    pub task_id: Option<String>,
}

/// 整表替换:先清后写,顺序即展示顺序。
pub fn replace(conn: &Connection, session_id: &str, items: &[TodoItem]) -> Result<(), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("todo 事务开启失败: {e}"))?;
    tx.execute("DELETE FROM todos WHERE session_id = ?1", [session_id])
        .map_err(|e| format!("todo 清空失败: {e}"))?;
    let now = chrono::Utc::now().timestamp();
    for (i, it) in items.iter().enumerate() {
        tx.execute(
            "INSERT INTO todos(session_id, item_id, content, status, priority, ord, updated, task_id)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                session_id,
                it.id,
                it.content,
                it.status,
                it.priority,
                i as i64,
                now,
                it.task_id,
            ],
        )
        .map_err(|e| format!("todo 写入失败: {e}"))?;
    }
    tx.commit().map_err(|e| format!("todo 提交失败: {e}"))
}

/// 按写入顺序读出某会话的清单。
pub fn list(conn: &Connection, session_id: &str) -> Result<Vec<TodoItem>, String> {
    let mut st = conn
        .prepare(
            "SELECT item_id, content, status, priority, task_id FROM todos
             WHERE session_id = ?1 ORDER BY ord ASC",
        )
        .map_err(|e| format!("todo 查询失败: {e}"))?;
    let rows = st
        .query_map([session_id], |r| {
            Ok(TodoItem {
                id: r.get(0)?,
                content: r.get(1)?,
                status: r.get(2)?,
                priority: r.get(3)?,
                task_id: r.get(4)?,
            })
        })
        .map_err(|e| format!("todo 读取失败: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("todo 行解析失败: {e}"))?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    fn mem() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        schema::migrate(&c).unwrap();
        c
    }

    fn item(id: &str, status: &str) -> TodoItem {
        TodoItem {
            id: id.into(),
            content: format!("任务 {id}"),
            status: status.into(),
            priority: "medium".into(),
            task_id: None,
        }
    }

    #[test]
    fn replace_is_atomic_full_swap() {
        let conn = mem();
        replace(
            &conn,
            "s1",
            &[item("a", "pending"), item("b", "in_progress")],
        )
        .unwrap();
        assert_eq!(list(&conn, "s1").unwrap().len(), 2);

        // 替换后旧项消失、顺序保持
        replace(&conn, "s1", &[item("b", "completed")]).unwrap();
        let l = list(&conn, "s1").unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].id, "b");
        assert_eq!(l[0].status, "completed");
        // 其他会话不受影响
        assert!(list(&conn, "s2").unwrap().is_empty());
    }

    #[test]
    fn list_preserves_insert_order() {
        let conn = mem();
        replace(&conn, "s1", &[item("c", "pending"), item("a", "pending")]).unwrap();
        let ids: Vec<String> = list(&conn, "s1")
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(ids, vec!["c", "a"]);
    }
}
