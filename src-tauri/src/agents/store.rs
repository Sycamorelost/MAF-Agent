//! 自定义模板落盘读写与查询/增删校验。

use std::{collections::HashSet, fs, path::PathBuf};

use super::builtin::builtin_templates;
use super::storage::{load_decoupled, remove_decoupled, save_decoupled};
use super::template::{AgentTemplate, CATEGORIES};

fn custom_path() -> PathBuf {
    crate::paths::data_dir().join("agents.json")
}

/// 默认助手 id:唯一不可删除的内置智能体。
pub const DEFAULT_ASSISTANT_ID: &str = "bare-assistant";

fn deleted_builtins_path() -> PathBuf {
    crate::paths::data_dir().join("agents_deleted.json")
}

/// 读取被用户删除的内置 agent_id 清单(缺失/损坏时为空)。
fn load_deleted_builtins() -> Vec<String> {
    match fs::read_to_string(deleted_builtins_path()) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_deleted_builtins(ids: &[String]) -> Result<(), String> {
    let p = deleted_builtins_path();
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    }
    let text = serde_json::to_string(ids).map_err(|e| e.to_string())?;
    fs::write(&p, text).map_err(|e| format!("write {} failed: {e}", p.display()))
}

/// 读取用户自定义/变体模板 = 耦合(agents.json) + 解耦目录;缺失/损坏时跳过。
pub fn load_custom() -> Vec<AgentTemplate> {
    let coupled = match fs::read_to_string(custom_path()) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let mut out = coupled;
    for mut a in load_decoupled() {
        a.storage = Some("decoupled".into());
        out.push(a);
    }
    out
}

fn save_custom(list: &[AgentTemplate]) -> Result<(), String> {
    let p = custom_path();
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    fs::write(&p, text).map_err(|e| format!("write {} failed: {e}", p.display()))
}

/// 全部模板 = 内置 + 自定义;自定义 id 与内置相同时覆盖内置(允许用户编辑内置 prompt)。
/// 被用户删除的内置(默认助手除外)在此过滤掉。
pub fn list_all() -> Vec<AgentTemplate> {
    let deleted: HashSet<String> = load_deleted_builtins().into_iter().collect();
    let builtins = builtin_templates();
    let mut out = builtins;
    for mut c in load_custom() {
        c.builtin = false;
        if let Some(pos) = out.iter().position(|b| b.agent_id == c.agent_id) {
            // 自定义覆盖同 id 内置
            c.overrides_builtin = true;
            out[pos] = c;
        } else {
            out.push(c);
        }
    }
    out.retain(|t| !deleted.contains(t.agent_id.as_str()));
    out
}

/// 按 id 查模板(自定义覆盖优先)。
pub fn find(agent_id: &str) -> Option<AgentTemplate> {
    list_all().into_iter().find(|t| t.agent_id == agent_id)
}

/// 新增/更新自定义模板或变体。校验:id 非空;分类合法;
/// memory_strategy 合法;变体的 base_agent 必须存在。
/// 允许与内置同 id(视为覆盖内置 prompt);保存后 builtin 强制为 false。
/// 根据 `storage` 字段路由:decoupled → 目录多文件;否则耦合 agents.json。
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
    if let Some(base) = &agent.base_agent
        && find(base).is_none()
    {
        return Err(format!("变体基座不存在:{base}"));
    }
    let mut agent = agent;
    agent.builtin = false;
    agent.overrides_builtin = false;

    let is_decoupled = agent.storage.as_deref() == Some("decoupled");
    if is_decoupled {
        // 解耦保存:先从耦合存储中移除同 id(防止双写),再写目录
        remove_from_coupled(&agent.agent_id)?;
        save_decoupled(&agent)
    } else {
        // 耦合保存:先移除解耦目录,再写 agents.json
        let _ = remove_decoupled(&agent.agent_id);
        let mut list: Vec<AgentTemplate> = match fs::read_to_string(custom_path()) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        if let Some(x) = list.iter_mut().find(|x| x.agent_id == agent.agent_id) {
            *x = agent;
        } else {
            list.push(agent);
        }
        save_custom(&list)
    }
}

/// 从耦合 agents.json 中移除指定 id(不影响解耦目录)。
fn remove_from_coupled(agent_id: &str) -> Result<(), String> {
    let list: Vec<AgentTemplate> = match fs::read_to_string(custom_path()) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => return Ok(()),
    };
    let next: Vec<_> = list
        .into_iter()
        .filter(|t| t.agent_id != agent_id)
        .collect();
    save_custom(&next)
}

/// 删除自定义模板;若为内置 id,删除自定义覆盖即恢复内置(不报错)。
/// 同时清理耦合与解耦两种存储。(统一删除入口见 `remove_template`)
pub fn remove_custom(agent_id: &str) -> Result<(), String> {
    remove_from_coupled(agent_id)?;
    let _ = remove_decoupled(agent_id);
    Ok(())
}

/// 删除模板:默认助手不可删;内置(非默认助手)记入已删除清单并清理自定义覆盖,
/// 使其从列表消失(可通过 `restore_deleted_builtins` 找回);自定义/变体直接移除。
pub fn remove_template(agent_id: &str) -> Result<(), String> {
    if agent_id == DEFAULT_ASSISTANT_ID {
        return Err("默认助手不可删除".into());
    }
    // 先清理自定义存储(覆盖或纯自定义)
    remove_custom(agent_id)?;
    // 若是内置,记入已删除清单,list_all 将不再返回它
    if builtin_templates().iter().any(|b| b.agent_id == agent_id) {
        let mut deleted = load_deleted_builtins();
        if !deleted.iter().any(|x| x == agent_id) {
            deleted.push(agent_id.to_string());
            save_deleted_builtins(&deleted)?;
        }
    }
    Ok(())
}

/// 恢复所有被删除的内置智能体(清空已删除清单)。
pub fn restore_deleted_builtins() -> Result<(), String> {
    save_deleted_builtins(&[])
}
