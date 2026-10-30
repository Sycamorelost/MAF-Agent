#![allow(dead_code)]

//! 运行时预设 IPC(P5 / D18 / D19):列表 / 会话解析 / 会话级覆盖钉住。
//!
//! 三级合并:会话钉住 > Agent 模板默认 > 全局默认(settings.default_preset);
//! 会话创建时已在 create_session 钉住 preset_id + preset_version,此处处理「切换」。

use serde::Serialize;

use crate::db::sessions;
use crate::presets::resolve::{PresetSource, ResolvedPreset, resolve};
use crate::presets::{self, PRESETS_VERSION, Preset};

/// 列出全部预设(内置四档 + 用户覆盖文件),按 order 排序。
#[tauri::command]
pub async fn list_presets() -> Vec<Preset> {
    presets::list(&crate::paths::data_dir())
}

/// 会话预设解析结果:生效预设 + 来源 + 会话钉住状态。
#[derive(Debug, Clone, Serialize)]
pub struct SessionPresetInfo {
    pub preset: Preset,
    pub source: PresetSource,
    /// 会话钉住的 preset_id(None = 跟随 Agent / 全局)
    pub pinned_id: Option<String>,
    /// 钉住时的预设清单版本(未钉住 = None)
    pub pinned_version: Option<i64>,
}

/// 按会话行解析生效预设(会话 > Agent > 全局默认)。
pub(crate) fn resolve_for(s: &sessions::SessionRow) -> SessionPresetInfo {
    let all = presets::list(&crate::paths::data_dir());
    let agent_preset = s
        .agent_id
        .as_deref()
        .and_then(crate::agents::find)
        .and_then(|t| t.preset);
    let default = crate::settings::load().default_preset;
    let ResolvedPreset { preset, source } =
        resolve(&all, s.preset_id.as_deref(), agent_preset.as_deref(), &default);
    SessionPresetInfo {
        preset,
        source,
        pinned_id: s.preset_id.clone(),
        pinned_version: s.preset_version,
    }
}

/// 解析会话生效预设(工具下发过滤与前端 chip / 上下文阈值共用)。
#[tauri::command]
pub async fn resolve_session_preset(session_id: String) -> Result<SessionPresetInfo, String> {
    let s = sessions::get(&session_id)?.ok_or_else(|| format!("会话不存在:{session_id}"))?;
    Ok(resolve_for(&s))
}

/// 设置会话级预设覆盖:Some(id) 钉住并记录当前清单版本;None 恢复继承(清版本)。
/// 切换后即时生效:下一轮消息的工具下发与上下文阈值即按新预设。
#[tauri::command]
pub async fn set_session_preset(
    session_id: String,
    preset_id: Option<String>,
) -> Result<SessionPresetInfo, String> {
    sessions::get(&session_id)?.ok_or_else(|| format!("会话不存在:{session_id}"))?;
    match &preset_id {
        Some(pid) => {
            // 校验预设存在(含用户覆盖),避免钉住一个无效 id 后静默回落
            presets::get(&crate::paths::data_dir(), pid)
                .ok_or_else(|| format!("预设不存在:{pid}"))?;
            sessions::set_preset(&session_id, Some((pid, PRESETS_VERSION)))?;
        }
        None => sessions::set_preset(&session_id, None)?,
    }
    resolve_session_preset(session_id).await
}
