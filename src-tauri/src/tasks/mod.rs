#![allow(dead_code)]

//! 任务领域模型(V25 / D8):目标级任务 + 步骤计划(plan JSON)。
//!
//! 与 todos 的分工:tasks 是用户可见的「目标级」卡片(标题/目标/状态/优先级/计划),
//! todos 是模型执行的「步骤级」清单;todos.task_id 可空关联到 tasks,旧数据为 NULL,
//! 仍按会话维度工作。UI 文案固定:任务 = 目标级,清单 = 步骤级(val.md 风险表)。
//!
//! plan 只存结构化 JSON({"steps":[{id,title,status}]}),模型生成、用户可编辑,
//! 不存大段正文,避免数据库膨胀。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// 任务行(与 tasks 表结构对齐)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub session_id: String,
    pub title: String,
    pub goal: String,
    /// pending / running / paused / done / failed / cancelled
    pub status: String,
    /// low / normal / high
    pub priority: String,
    /// 计划步骤 JSON;None = 未规划
    pub plan: Option<String>,
    pub created: String,
    pub updated: String,
}

/// 合法任务状态。done / failed / cancelled 为终态(仅约束取值,流转不做硬限制:
/// 用户可以把误标的任务改回 running,符合「计划可编辑」原则)。
pub const TASK_STATUSES: [&str; 6] = ["pending", "running", "paused", "done", "failed", "cancelled"];

/// 合法任务优先级。
pub const TASK_PRIORITIES: [&str; 3] = ["low", "normal", "high"];

fn now() -> String {
    chrono::Local::now().to_rfc3339()
}

fn row_to_task(r: &rusqlite::Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        session_id: r.get(1)?,
        title: r.get(2)?,
        goal: r.get(3)?,
        status: r.get(4)?,
        priority: r.get(5)?,
        plan: r.get(6)?,
        created: r.get(7)?,
        updated: r.get(8)?,
    })
}

const COLS: &str = "id, session_id, title, goal, status, priority, plan, created, updated";

fn check_priority(priority: &str) -> Result<(), String> {
    if TASK_PRIORITIES.contains(&priority) {
        Ok(())
    } else {
        Err(format!(
            "invalid priority: {priority}(可选:{})",
            TASK_PRIORITIES.join("/")
        ))
    }
}

fn check_status(status: &str) -> Result<(), String> {
    if TASK_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(format!(
            "invalid status: {status}(可选:{})",
            TASK_STATUSES.join("/")
        ))
    }
}

/// 新建任务(默认 pending)。title 去空白后为空则报错。
pub fn create(
    conn: &Connection,
    session_id: &str,
    title: &str,
    goal: &str,
    priority: &str,
) -> Result<Task, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("任务标题不能为空".into());
    }
    check_priority(priority)?;
    let t = Task {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.into(),
        title: title.into(),
        goal: goal.trim().into(),
        status: "pending".into(),
        priority: priority.into(),
        plan: None,
        created: now(),
        updated: now(),
    };
    conn.execute(
        "INSERT INTO tasks(id, session_id, title, goal, status, priority, plan, created, updated)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8)",
        rusqlite::params![
            t.id,
            t.session_id,
            t.title,
            t.goal,
            t.status,
            t.priority,
            t.created,
            t.updated
        ],
    )
    .map_err(|e| format!("任务写入失败: {e}"))?;
    Ok(t)
}

/// 按创建时间正序列出会话任务(面板按状态分组,组内按创建序)。
pub fn list_by_session(conn: &Connection, session_id: &str) -> Result<Vec<Task>, String> {
    let mut st = conn
        .prepare(&format!(
            "SELECT {COLS} FROM tasks WHERE session_id = ?1 ORDER BY created ASC, id ASC"
        ))
        .map_err(|e| format!("任务查询失败: {e}"))?;
    let rows = st
        .query_map([session_id], row_to_task)
        .map_err(|e| format!("任务读取失败: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("任务行解析失败: {e}"))?);
    }
    Ok(out)
}

/// 部分更新:title / goal / status / priority 传 Some 才改;非法值报错。
pub fn update(
    conn: &Connection,
    id: &str,
    title: Option<&str>,
    goal: Option<&str>,
    status: Option<&str>,
    priority: Option<&str>,
) -> Result<(), String> {
    if let Some(s) = status {
        check_status(s)?;
    }
    if let Some(p) = priority {
        check_priority(p)?;
    }
    if let Some(t) = title
        && t.trim().is_empty()
    {
        return Err("任务标题不能为空".into());
    }
    let cur: Option<Task> = conn
        .query_row(
            &format!("SELECT {COLS} FROM tasks WHERE id = ?1"),
            [id],
            row_to_task,
        )
        .ok();
    let Some(cur) = cur else {
        return Err(format!("任务不存在:{id}"));
    };
    conn.execute(
        "UPDATE tasks SET title=?1, goal=?2, status=?3, priority=?4, updated=?5 WHERE id=?6",
        rusqlite::params![
            title.map(str::trim).unwrap_or(&cur.title),
            goal.map(str::trim).unwrap_or(&cur.goal),
            status.unwrap_or(&cur.status),
            priority.unwrap_or(&cur.priority),
            now(),
            id
        ],
    )
    .map_err(|e| format!("任务更新失败: {e}"))?;
    Ok(())
}

/// 写回计划(JSON 结构化步骤)。None 清除;Some 必须是合法 JSON,防止把大段正文写进库。
pub fn set_plan(conn: &Connection, id: &str, plan: Option<&str>) -> Result<(), String> {
    if let Some(p) = plan {
        serde_json::from_str::<serde_json::Value>(p)
            .map_err(|e| format!("plan 必须是合法 JSON: {e}"))?;
    }
    let n = conn
        .execute(
            "UPDATE tasks SET plan=?1, updated=?2 WHERE id=?3",
            rusqlite::params![plan, now(), id],
        )
        .map_err(|e| format!("计划写回失败: {e}"))?;
    if n == 0 {
        return Err(format!("任务不存在:{id}"));
    }
    Ok(())
}

/// 删除任务。todos.task_id 的关联不清空(步骤清单仍按会话维度可用)。
pub fn remove(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM tasks WHERE id=?1", [id])
        .map_err(|e| format!("任务删除失败: {e}"))?;
    Ok(())
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

    #[test]
    fn create_and_list_roundtrip() {
        let conn = mem();
        let t = create(&conn, "s1", "  整理报表  ", "月度数据", "high").unwrap();
        assert_eq!(t.title, "整理报表");
        assert_eq!(t.status, "pending");
        let list = list_by_session(&conn, "s1").unwrap();
        assert_eq!(list.len(), 1);
        assert!(list_by_session(&conn, "s2").unwrap().is_empty());
    }

    #[test]
    fn update_validates_enums_and_title() {
        let conn = mem();
        let t = create(&conn, "s1", "a", "", "normal").unwrap();
        assert!(update(&conn, &t.id, None, None, Some("running"), None).is_ok());
        assert!(update(&conn, &t.id, None, None, Some("bogus"), None).is_err());
        assert!(update(&conn, &t.id, None, None, None, Some("urgent")).is_err());
        assert!(update(&conn, &t.id, Some("  "), None, None, None).is_err());
        assert!(update(&conn, "missing", None, None, Some("done"), None).is_err());
        let cur = list_by_session(&conn, "s1").unwrap().pop().unwrap();
        assert_eq!(cur.status, "running");
    }

    #[test]
    fn set_plan_requires_valid_json() {
        let conn = mem();
        let t = create(&conn, "s1", "a", "", "low").unwrap();
        assert!(set_plan(&conn, &t.id, Some("not json")).is_err());
        set_plan(&conn, &t.id, Some(r#"{"steps":[{"id":"1","title":"查数","status":"pending"}]}"#))
            .unwrap();
        let cur = list_by_session(&conn, "s1").unwrap().pop().unwrap();
        assert!(cur.plan.unwrap().contains("查数"));
        set_plan(&conn, &t.id, None).unwrap();
        assert!(set_plan(&conn, "missing", Some("{}")).is_err());
    }

    #[test]
    fn remove_keeps_other_sessions() {
        let conn = mem();
        let a = create(&conn, "s1", "a", "", "normal").unwrap();
        create(&conn, "s2", "b", "", "normal").unwrap();
        remove(&conn, &a.id).unwrap();
        assert!(list_by_session(&conn, "s1").unwrap().is_empty());
        assert_eq!(list_by_session(&conn, "s2").unwrap().len(), 1);
    }
}
