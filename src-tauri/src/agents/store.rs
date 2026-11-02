//! 自定义模板落盘读写与查询/增删校验。

use std::{fs, path::PathBuf};

use super::builtin::builtin_templates;
use super::template::{AgentTemplate, CATEGORIES};

fn custom_path() -> PathBuf {
    crate::paths::data_dir().join("agents.json")
}

/// 读取用户自定义/变体模板;文件缺失/损坏时返回空(不报错,本地优先)。
pub fn load_custom() -> Vec<AgentTemplate> {
    let Ok(text) = fs::read_to_string(custom_path()) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_custom(list: &[AgentTemplate]) -> Result<(), String> {
    let p = custom_path();
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    fs::write(&p, text).map_err(|e| format!("write {} failed: {e}", p.display()))
}

/// 全部模板 = 内置 + 自定义(自定义 id 与内置冲突时忽略自定义,防覆盖)。
pub fn list_all() -> Vec<AgentTemplate> {
    let builtins = builtin_templates();
    let ids: Vec<String> = builtins.iter().map(|t| t.agent_id.clone()).collect();
    let mut out = builtins;
    for mut c in load_custom() {
        if ids.contains(&c.agent_id) {
            continue;
        }
        c.builtin = false;
        out.push(c);
    }
    out
}

/// 按 id 查模板(内置优先)。
pub fn find(agent_id: &str) -> Option<AgentTemplate> {
    list_all().into_iter().find(|t| t.agent_id == agent_id)
}

/// 新增/更新自定义模板或变体。校验:id 非空且不与内置冲突;分类合法;
/// 变体的 base_agent 必须存在。
pub fn upsert_custom(agent: AgentTemplate) -> Result<(), String> {
    if agent.agent_id.trim().is_empty() {
        return Err("agent_id 不能为空".into());
    }
    if !CATEGORIES.contains(&agent.category.as_str()) {
        return Err(format!(
            "非法分类:{}(可选:{})",
            agent.category,
            CATEGORIES.join("/")
        ));
    }
    if !["full", "summary", "none"].contains(&agent.memory_strategy.as_str()) {
        return Err(format!("非法 memory_strategy:{}", agent.memory_strategy));
    }
    if builtin_templates()
        .iter()
        .any(|b| b.agent_id == agent.agent_id)
    {
        return Err(format!("不能覆盖内置模板:{}", agent.agent_id));
    }
    if let Some(base) = &agent.base_agent
        && find(base).is_none()
    {
        return Err(format!("变体基座不存在:{base}"));
    }
    let mut list = load_custom();
    let mut agent = agent;
    agent.builtin = false;
    if let Some(x) = list.iter_mut().find(|x| x.agent_id == agent.agent_id) {
        *x = agent;
    } else {
        list.push(agent);
    }
    save_custom(&list)
}

/// 删除自定义模板;内置模板拒绝删除。
pub fn remove_custom(agent_id: &str) -> Result<(), String> {
    if builtin_templates().iter().any(|b| b.agent_id == agent_id) {
        return Err(format!("内置模板不可删除:{agent_id}"));
    }
    let list = load_custom();
    let next: Vec<_> = list
        .into_iter()
        .filter(|t| t.agent_id != agent_id)
        .collect();
    save_custom(&next)
}
