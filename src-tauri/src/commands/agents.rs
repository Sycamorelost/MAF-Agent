//! Agent 模板相关 Tauri commands(迭代 B2)。

use std::path::PathBuf;

use tauri_plugin_dialog::DialogExt;

use crate::agents::{self, export_agent, import_agent, AgentTemplate};
use crate::db::sessions;

/// 列出全部 Agent 模板(内置 14 个 + 用户变体/自定义)。
#[tauri::command]
pub async fn list_agent_templates() -> Vec<AgentTemplate> {
    agents::list_all()
}

/// 保存自定义模板或变体(upsert;允许与内置同 id 以覆盖内置 prompt)。
#[tauri::command]
pub async fn save_agent_template(agent: AgentTemplate) -> Result<(), String> {
    agents::upsert_custom(agent)
}

/// 删除自定义模板/变体;若为内置 id 则删除自定义覆盖、恢复内置。
#[tauri::command]
pub async fn delete_agent_template(agent_id: String) -> Result<(), String> {
    agents::remove_custom(&agent_id)
}

/// 导出单个智能体为 JSON 字符串(便于分享/备份)。
#[tauri::command]
pub async fn export_agent_template(agent_id: String) -> Result<String, String> {
    let agent = agents::find(&agent_id).ok_or_else(|| format!("智能体不存在:{}", agent_id))?;
    export_agent(&agent)
}

/// 导出单个智能体并弹出原生「另存为」对话框,由用户选择保存路径。
/// 用户取消返回 None;选定并写入成功返回 Some(实际路径字符串)。
#[tauri::command]
pub async fn export_agent_to_file(
    app: tauri::AppHandle,
    agent_id: String,
) -> Result<Option<String>, String> {
    let agent = agents::find(&agent_id).ok_or_else(|| format!("智能体不存在:{}", agent_id))?;
    let json = export_agent(&agent)?;
    let default_name = format!("{}.json", sanitize_filename(&agent_id));
    let picked: Option<PathBuf> = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("导出智能体")
            .set_file_name(default_name)
            .add_filter("JSON", &["json"])
            .blocking_save_file()
            .and_then(|p| p.into_path().ok())
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(path) = picked else {
        return Ok(None);
    };
    std::fs::write(&path, json).map_err(|e| format!("写入文件失败:{}", e))?;
    Ok(Some(path.display().to_string()))
}

/// 把文件名里不合法的字符替换为下划线(仅用于默认文件名兜底)。
fn sanitize_filename(name: &str) -> String {
    name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

/// 弹出原生「打开文件」对话框,供用户选择关联的 prompt / tools 文件。
/// `filter_ext` 如 "md" / "json";用户取消返回 None。
#[tauri::command]
pub async fn pick_agent_file(
    app: tauri::AppHandle,
    title: String,
    filter_name: String,
    filter_ext: String,
) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title(title)
            .add_filter(filter_name, &[filter_ext.as_str()])
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.display().to_string())
    })
    .await
    .ok()
    .flatten()
}

/// 导入 JSON 字符串为智能体并落盘(按其 storage 字段路由)。
#[tauri::command]
pub async fn import_agent_template(json: String) -> Result<(), String> {
    let mut agent = import_agent(&json)?;
    // 导入时强制非内置;若 id 已存在则覆盖(由 upsert 处理)
    agent.builtin = false;
    agents::upsert_custom(agent)
}

/// 绑定/解绑会话的 Agent(agent_id=None 解绑)。
/// 绑定时同步把 Agent 的 memory_strategy 写入会话注入策略,
/// 并把渲染变量后的 system_prompt 写入会话(B4 / F2),
/// 使「切换后 system prompt 与记忆注入策略变化」可独立验证。
#[tauri::command]
pub async fn set_session_agent(id: String, agent_id: Option<String>) -> Result<(), String> {
    let strategy = match &agent_id {
        Some(aid) => {
            let tpl = agents::find(aid).ok_or_else(|| format!("Agent 模板不存在:{aid}"))?;
            // 渲染 {date}/{model}/{language} 后落盘;{model} 取会话当前模型
            let model = sessions::get(&id)?
                .and_then(|s| s.model_id)
                .unwrap_or_default();
            let rendered = agents::render_prompt(&tpl.system_prompt, &model);
            sessions::set_system_prompt(&id, Some(&rendered))?;
            Some(tpl.memory_strategy)
        }
        None => None,
    };
    sessions::set_agent(&id, agent_id.as_deref())?;
    if let Some(s) = strategy {
        // Agent 策略只含 full/summary/none,均为合法会话策略
        sessions::set_memory_strategy(&id, &s)?;
    }
    Ok(())
}

/// 应用 Agent 的 system_prompt 为会话提示词(B4 / F2):
/// 仅渲染并写入 sessions.system_prompt,不改变 Agent 绑定与记忆注入策略。
#[tauri::command]
pub async fn apply_agent_prompt(id: String, agent_id: String) -> Result<(), String> {
    let tpl = agents::find(&agent_id).ok_or_else(|| format!("Agent 模板不存在:{agent_id}"))?;
    let model = sessions::get(&id)?
        .and_then(|s| s.model_id)
        .unwrap_or_default();
    let rendered = agents::render_prompt(&tpl.system_prompt, &model);
    sessions::set_system_prompt(&id, Some(&rendered))
}
