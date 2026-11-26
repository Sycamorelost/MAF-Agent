//! 会话归档与批量操作命令(V41)。
//!
//! 归档语义:仅置 `sessions.archived=1`,会话本身、消息、记忆、附件全部保留,
//! 列表默认不再返回;用户可随时恢复。批量删除复用单条删除的清理链路。

use tauri::State;

use crate::db::{session_archive, sessions};
use crate::memory::{self, watcher::WatcherState};

/// 已归档会话列表(侧栏「已归档」分组数据源)。
#[tauri::command]
pub async fn list_archived_sessions() -> Result<Vec<sessions::SessionRow>, String> {
    session_archive::list_archived()
}

/// 单个会话归档 / 恢复。
#[tauri::command]
pub async fn set_session_archived(id: String, archived: bool) -> Result<(), String> {
    session_archive::set_archived_many(&[id], archived)
}

/// 批量归档 / 恢复,返回实际处理的条数。
#[tauri::command]
pub async fn set_sessions_archived(ids: Vec<String>, archived: bool) -> Result<usize, String> {
    if ids.is_empty() {
        return Ok(0);
    }
    session_archive::set_archived_many(&ids, archived)?;
    Ok(ids.len())
}

/// 批量移入项目(project_id=None 表示移出项目),复用单条的 guard 注入逻辑。
#[tauri::command]
pub async fn set_sessions_project(ids: Vec<String>, project_id: Option<String>) -> Result<(), String> {
    for id in &ids {
        sessions::set_project(id, project_id.as_deref())?;
    }
    Ok(())
}

/// 批量删除:与 `delete_session` 同一清理链路(DB 记录 / 压缩摘要 / 磁盘目录)。
#[tauri::command]
pub async fn delete_sessions(
    ids: Vec<String>,
    state: State<'_, WatcherState>,
) -> Result<usize, String> {
    let mut done = 0usize;
    for id in &ids {
        memory::validate_id(id)?;
        // 先解除 watcher,避免 Windows 句柄占用导致目录删不掉
        let _ = state.registry.unwatch(id);
        sessions::delete(id)?;
        let _ = crate::db::compaction::clear(id);
        let dir = memory::session_dir(id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .map_err(|e| format!("remove {} failed: {e}", dir.display()))?;
        }
        done += 1;
    }
    Ok(done)
}
