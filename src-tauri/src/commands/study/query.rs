//! 计划列表/详情/删除/导出/深挖研究联动。

use serde::Serialize;
use tauri::State;

use crate::memory;
use crate::study::{self, PlanPayload, StudyEvent, store};

use super::registry::StudyRegistry;

// ---------- 查询 / 删除 ----------

#[derive(Debug, Serialize)]
pub struct StudyDetail {
    pub plan: store::PlanRow,
    /// 生成的计划载荷(尚未生成/失败为 None)
    pub payload: Option<PlanPayload>,
    pub trail: Vec<StudyEvent>,
}

#[tauri::command]
pub async fn study_list() -> Result<Vec<store::PlanRow>, String> {
    store::list_plans()
}

#[tauri::command]
pub async fn study_get(id: String) -> Result<StudyDetail, String> {
    memory::validate_id(&id)?;
    let plan = store::get_plan(&id)?.ok_or("学习计划不存在")?;
    let trail = store::read_trail(&id)?;
    let payload = store::read_payload(&id);
    Ok(StudyDetail {
        plan,
        payload,
        trail,
    })
}

/// 删除学习计划:运行中先终止,等引擎自行退出(≤5s)后删行删目录。
#[tauri::command]
pub async fn study_delete(id: String, registry: State<'_, StudyRegistry>) -> Result<(), String> {
    memory::validate_id(&id)?;
    // 引擎退出时会把自己从注册表移除;以此判断是否仍在运行
    let running = |reg: &StudyRegistry| reg.0.lock().map(|m| m.contains_key(&id)).unwrap_or(false);
    if running(&registry) {
        if let Ok(m) = registry.0.lock()
            && let Some(entry) = m.get(&id)
        {
            entry.ctl.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            entry.ctl.paused.store(false, std::sync::atomic::Ordering::Relaxed);
        }
        // 等任务真正退出,避免删后引擎又写盘「删而又生」
        for _ in 0..50 {
            if !running(&registry) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    store::delete(&id)
}

// ---------- 深挖研究联动 ----------

/// 记录一次「深入研究」:study_logs 落 kind=research 行,返回预填给新会话的主题。
#[tauri::command]
pub async fn study_deep_dig(id: String, node_name: String) -> Result<String, String> {
    memory::validate_id(&id)?;
    let name = node_name.trim().to_string();
    if name.is_empty() {
        return Err("知识点名称不能为空".into());
    }
    let plan = store::get_plan(&id)?.ok_or("学习计划不存在")?;
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    let node_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM knowledge_nodes WHERE plan_id=?1 AND name=?2",
            rusqlite::params![id, name],
            |r| r.get(0),
        )
        .ok();
    store::add_log(&conn, &id, node_id, "research", 0)?;
    Ok(format!("深入学习:{}(源自学习计划「{}」)", name, plan.topic))
}

// ---------- 导出 ----------

/// 导出学习计划 Markdown,返回实际写入路径。
/// token 为 pick_save_path 签发的路径令牌(复用通用导出的 N-6 机制)。
#[tauri::command]
pub async fn study_export_markdown(
    id: String,
    token: String,
    overwrite: bool,
    tokens: State<'_, crate::export::ExportPathTokens>,
) -> Result<String, String> {
    memory::validate_id(&id)?;
    let target = tokens.resolve(&token)?;
    if target
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        != Some("md".into())
    {
        return Err("导出文件扩展名必须为 .md".into());
    }
    let payload = store::read_payload(&id).ok_or("学习计划尚未生成,无法导出")?;
    let md = study::export_markdown(&payload);
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("mkdir failed: {e}"))?;
    }
    if target.exists() && !overwrite {
        return Err("TARGET_EXISTS".into());
    }
    std::fs::write(&target, md).map_err(|e| format!("write failed: {e}"))?;
    Ok(target.to_string_lossy().to_string())
}
