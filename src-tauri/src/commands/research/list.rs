//! 研究会话列表、详情、删除,以及预设 / 方案 / 模板元数据。

use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::State;

use crate::memory;
use crate::research::{self, ResearchEvent, ResearchParams, blueprint, preset, store, template};

use super::ResearchRegistry;

// ---------- 研究预设(迭代 D4) ----------

/// 列出全部研究预设。
#[tauri::command]
pub fn research_preset_list() -> Vec<preset::ResearchPreset> {
    preset::load_presets()
}

/// 保存研究预设(同名覆盖),返回最新列表。
#[tauri::command]
pub fn research_preset_save(
    name: String,
    params: ResearchParams,
) -> Result<Vec<preset::ResearchPreset>, String> {
    preset::save_preset(&name, params)
}

/// 删除研究预设,返回最新列表。
#[tauri::command]
pub fn research_preset_delete(name: String) -> Result<Vec<preset::ResearchPreset>, String> {
    preset::delete_preset(&name)
}

// ---------- 研究方案 Blueprint(专项研究步骤 4) ----------

/// 列出全部研究方案(内置 + 用户;旧预设已自动迁移)。
#[tauri::command]
pub fn blueprint_list() -> Vec<blueprint::Blueprint> {
    blueprint::list()
}

/// 保存研究方案(同名覆盖;内置名保护),返回最新列表。
#[tauri::command]
pub fn blueprint_save(
    name: String,
    desc: String,
    params: ResearchParams,
) -> Result<Vec<blueprint::Blueprint>, String> {
    blueprint::save(&name, &desc, params)
}

/// 删除用户方案(内置名保护),返回最新列表。
#[tauri::command]
pub fn blueprint_delete(name: String) -> Result<Vec<blueprint::Blueprint>, String> {
    blueprint::delete(&name)
}

// ---------- 提示词模板(专项研究步骤 1) ----------

/// 列出全部研究提示词模板(全局决策提示 / 终稿报告模板,自定义覆盖优先)。
#[tauri::command]
pub fn research_template_list() -> Vec<template::TemplateDto> {
    template::list()
}

/// 保存模板自定义覆盖,返回最新列表。
#[tauri::command]
pub fn research_template_save(
    key: String,
    content: String,
) -> Result<Vec<template::TemplateDto>, String> {
    template::save(&key, &content)
}

/// 恢复内置模板(删除自定义覆盖),返回最新列表。
#[tauri::command]
pub fn research_template_reset(key: String) -> Result<Vec<template::TemplateDto>, String> {
    template::reset(&key)
}

/// 列出全部研究会话(按创建时间倒序;阶段进度直接读 M4 回写列,零文件 IO)。
#[tauri::command]
pub fn research_list() -> Result<Vec<store::ResearchRow>, String> {
    store::list()
}

#[derive(Debug, Serialize)]
pub struct ResearchDetail {
    pub session: store::ResearchRow,
    pub trail: Vec<ResearchEvent>,
    pub report: Option<String>,
    /// 专项模式阶段编排快照 + 运行期状态(legacy 为 None)
    pub stages: Option<research::stage::StagesFile>,
}

/// 获取研究详情(元数据 + 过程事件回放 + 报告正文 + 阶段快照)。
#[tauri::command]
pub fn research_get(id: String) -> Result<ResearchDetail, String> {
    memory::validate_id(&id)?;
    let session = store::get(&id)?.ok_or_else(|| format!("研究 {id} 不存在"))?;
    let trail = store::read_trail(&id)?;
    let report = store::read_report(&id)?;
    let stages = store::read_stages(&id);
    Ok(ResearchDetail {
        session,
        trail,
        report,
        stages,
    })
}

/// 删除研究(若在运行先取消并等待任务真正退出,再删 db 行 + 磁盘目录)。
///
/// M-4:旧实现只置取消标志就立刻删目录,引擎的下一次落盘会重新创建目录与文件,
/// 造成「删了又生」。这里取走任务句柄等待退出(上限 10 秒),确保引擎不再写盘。
#[tauri::command]
pub async fn research_delete(
    id: String,
    registry: State<'_, ResearchRegistry>,
) -> Result<(), String> {
    memory::validate_id(&id)?;
    let entry = {
        let m = registry.0.lock().map_err(|e| e.to_string())?;
        m.get(&id).cloned()
    };
    let mut engine_stuck = false;
    if let Some(entry) = entry {
        // 同时解除暂停,避免引擎停在 wait_gate 里不响应取消
        entry.ctl.paused.store(false, Ordering::Relaxed);
        entry.ctl.cancel.store(true, Ordering::Relaxed);
        if let Some(handle) = entry.handle.lock().ok().and_then(|mut g| g.take()) {
            // 引擎在检查点轮询间隔为 200ms,正常应在秒级退出;10 秒兜底防卡死阻塞前端
            engine_stuck = tokio::time::timeout(std::time::Duration::from_secs(10), handle)
                .await
                .is_err();
        }
        // 双保险:任务句柄缺失(如启动竞态)时再尝试从表中剔除
        if let Ok(mut m) = registry.0.lock() {
            m.remove(&id);
        }
    }
    // N-7:引擎卡死时仅删数据库行并保留目录,避免引擎随后重建目录「删而又生」
    if engine_stuck {
        log::warn!("research {id} 引擎 10s 内未退出,仅删除数据库记录,目录保留待手动清理");
        store::delete_row(&id)
    } else {
        store::delete(&id)
    }
}

// ---------- 科研增强:技能资源健康检查(v1.9.0 工作块 II) ----------

/// 列出内置科研方法论技能文档(健康检查;资源随包只读分发)。
#[tauri::command]
pub fn research_list_skills() -> Result<Vec<research::skills::SkillDoc>, String> {
    Ok(research::skills::list_docs())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// H1 回归:路径遍历 id 必须在入口被拒绝,且不得触碰磁盘。
    #[test]
    fn research_get_rejects_traversal_id() {
        let r = research_get("../../x".to_string());
        assert!(r.is_err());
        let msg = r.err().unwrap();
        assert!(msg.contains("非法"), "错误文案应指出 id 非法,实际: {msg}");
    }

    #[test]
    fn research_get_rejects_slash_id() {
        assert!(research_get("a/b".to_string()).is_err());
        assert!(research_get("a\\b".to_string()).is_err());
        assert!(research_get(String::new()).is_err());
    }
}
