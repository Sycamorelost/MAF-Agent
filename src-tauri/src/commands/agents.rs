//! Agent 模板相关 Tauri commands(迭代 B2)。

use crate::agents::{self, AgentTemplate};
use crate::db::sessions;

/// 列出全部 Agent 模板(内置 14 个 + 用户变体/自定义)。
#[tauri::command]
pub fn list_agent_templates() -> Vec<AgentTemplate> {
    agents::list_all()
}

/// 保存自定义模板或变体(upsert;内置 id 拒绝覆盖)。
#[tauri::command]
pub fn save_agent_template(agent: AgentTemplate) -> Result<(), String> {
    agents::upsert_custom(agent)
}

/// 删除自定义模板/变体(内置模板拒绝)。
#[tauri::command]
pub fn delete_agent_template(agent_id: String) -> Result<(), String> {
    agents::remove_custom(&agent_id)
}

/// 绑定/解绑会话的 Agent(agent_id=None 解绑)。
/// 绑定时同步把 Agent 的 memory_strategy 写入会话注入策略,
/// 并把渲染变量后的 system_prompt 写入会话(B4 / F2),
/// 使「切换后 system prompt 与记忆注入策略变化」可独立验证。
#[tauri::command]
pub fn set_session_agent(id: String, agent_id: Option<String>) -> Result<(), String> {
    let strategy = match &agent_id {
        Some(aid) => {
            let tpl = agents::find(aid).ok_or_else(|| format!("Agent 模板不存在:{aid}"))?;
            // 渲染 {date}/{model}/{language} 后落盘;{model} 取会话当前模型
            let model = sessions::get(&id)?.and_then(|s| s.model_id).unwrap_or_default();
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
pub fn apply_agent_prompt(id: String, agent_id: String) -> Result<(), String> {
    let tpl = agents::find(&agent_id).ok_or_else(|| format!("Agent 模板不存在:{agent_id}"))?;
    let model = sessions::get(&id)?.and_then(|s| s.model_id).unwrap_or_default();
    let rendered = agents::render_prompt(&tpl.system_prompt, &model);
    sessions::set_system_prompt(&id, Some(&rendered))
}
