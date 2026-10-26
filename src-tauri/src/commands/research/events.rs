//! 研究会话运行控制与检查点操作。

use std::sync::atomic::Ordering;

use tauri::State;

use crate::memory;
use crate::research::{self, stage, store};

use super::ResearchRegistry;

/// 请求取消一次正在运行的研究(置取消标记,引擎在下一个检查点停下)。
#[tauri::command]
pub fn research_cancel(id: String, registry: State<'_, ResearchRegistry>) -> Result<(), String> {
    memory::validate_id(&id)?;
    let m = registry.0.lock().map_err(|e| e.to_string())?;
    if let Some(entry) = m.get(&id) {
        entry.ctl.cancel.store(true, Ordering::Relaxed);
        Ok(())
    } else {
        Err("研究任务不存在或已结束".into())
    }
}

/// 暂停一次正在运行的研究(引擎在下一个检查点挂起)。
#[tauri::command]
pub fn research_pause(id: String, registry: State<'_, ResearchRegistry>) -> Result<(), String> {
    memory::validate_id(&id)?;
    let m = registry.0.lock().map_err(|e| e.to_string())?;
    if let Some(entry) = m.get(&id) {
        entry.ctl.paused.store(true, Ordering::Relaxed);
        Ok(())
    } else {
        Err("研究任务不存在或已结束".into())
    }
}

/// 继续一次已暂停的研究。
#[tauri::command]
pub fn research_resume(id: String, registry: State<'_, ResearchRegistry>) -> Result<(), String> {
    memory::validate_id(&id)?;
    let m = registry.0.lock().map_err(|e| e.to_string())?;
    if let Some(entry) = m.get(&id) {
        entry.ctl.paused.store(false, Ordering::Relaxed);
        Ok(())
    } else {
        Err("研究任务不存在或已结束".into())
    }
}

/// 检查点确认继续(专项研究):语义等同 resume,供阶段间检查点操作条调用。
#[tauri::command]
pub fn research_stage_continue(
    id: String,
    registry: State<'_, ResearchRegistry>,
) -> Result<(), String> {
    memory::validate_id(&id)?;
    let m = registry.0.lock().map_err(|e| e.to_string())?;
    if let Some(entry) = m.get(&id) {
        entry.ctl.paused.store(false, Ordering::Relaxed);
        Ok(())
    } else {
        Err("研究任务不存在或已结束".into())
    }
}

/// H1:检查点「调整下阶段」— 写回 stages.json 的最新阶段编排。
/// 引擎在每个阶段起点热读 configs 使调整生效;运行期状态按 id 保留。
#[tauri::command]
pub fn research_update_stages(
    id: String,
    mut stages: Vec<stage::StageConfig>,
) -> Result<(), String> {
    memory::validate_id(&id)?;
    if stages.is_empty() {
        return Err("阶段编排不能为空".into());
    }
    // 与 research_start 相同的归一化:无名阶段给默认名,kind 白名单
    for (i, s) in stages.iter_mut().enumerate() {
        if s.name.trim().is_empty() {
            s.name = format!("阶段 {}", i + 1);
        }
        if s.kind != "synthesis" {
            s.kind = "research".into();
        }
    }
    let row = store::get(&id)?.ok_or_else(|| format!("研究 {id} 不存在"))?;
    if row.mode != "staged" {
        return Err("仅专项研究支持调整阶段".into());
    }
    let mut file =
        store::read_stages(&id).ok_or_else(|| "阶段快照 stages.json 缺失,无法调整".to_string())?;
    file.configs = stages;
    // 状态按 id 保留(含已完成阶段的摘要);新增阶段补 pending / skipped
    let mut states = Vec::with_capacity(file.configs.len());
    for c in &file.configs {
        let prev = file.stages.iter().find(|s| s.id == c.id).cloned();
        states.push(match prev {
            Some(mut s) => {
                s.name = c.name.clone();
                s.kind = c.kind.clone();
                s.enabled = c.enabled;
                // 停用尚未开始的阶段直接标 skipped;反向操作只发生在未开始时,回到 pending
                if !c.enabled && s.status == "pending" {
                    s.status = "skipped".into();
                } else if c.enabled && s.status == "skipped" && s.summary.is_empty() {
                    s.status = "pending".into();
                }
                s
            }
            None => research::stage::StageState {
                id: c.id.clone(),
                name: c.name.clone(),
                kind: c.kind.clone(),
                enabled: c.enabled,
                status: if c.enabled {
                    "pending".into()
                } else {
                    "skipped".into()
                },
                summary: String::new(),
                sources_count: 0,
                rounds: 0,
                partial: false,
            },
        });
    }
    file.stages = states;
    file.updated = chrono::Local::now().to_rfc3339();
    store::save_stages(&id, &file)
}
