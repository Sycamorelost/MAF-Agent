//! 任务命令(V25 / D8):参数校验与转发,业务落 `tasks` 领域模块。

use crate::db;
use crate::tasks::{self, Task};

/// 新建目标级任务(工作台任务计划面板)。
#[tauri::command]
pub async fn task_create(
    session_id: String,
    title: String,
    goal: Option<String>,
    priority: Option<String>,
) -> Result<Task, String> {
    let conn = db::conn().lock().map_err(|e| format!("db lock 失败: {e}"))?;
    tasks::create(
        &conn,
        &session_id,
        &title,
        goal.as_deref().unwrap_or(""),
        priority.as_deref().unwrap_or("normal"),
    )
}

/// 列出会话全部任务(按创建序;前端按状态分组)。
#[tauri::command]
pub async fn task_list(session_id: String) -> Result<Vec<Task>, String> {
    let conn = db::conn().lock().map_err(|e| format!("db lock 失败: {e}"))?;
    tasks::list_by_session(&conn, &session_id)
}

/// 部分更新任务字段(None = 不变)。
#[tauri::command]
pub async fn task_update(
    id: String,
    title: Option<String>,
    goal: Option<String>,
    status: Option<String>,
    priority: Option<String>,
) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| format!("db lock 失败: {e}"))?;
    tasks::update(
        &conn,
        &id,
        title.as_deref(),
        goal.as_deref(),
        status.as_deref(),
        priority.as_deref(),
    )
}

/// 写回任务计划(plan JSON);null 清除。
#[tauri::command]
pub async fn task_set_plan(id: String, plan: Option<String>) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| format!("db lock 失败: {e}"))?;
    tasks::set_plan(&conn, &id, plan.as_deref())
}

/// 删除任务。
#[tauri::command]
pub async fn task_delete(id: String) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| format!("db lock 失败: {e}"))?;
    tasks::remove(&conn, &id)
}
