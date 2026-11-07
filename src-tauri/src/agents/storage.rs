//! 解耦存储:单个智能体拆为目录多文件(config.json / prompt.md / tools.json)。
//!
//! 与耦合存储(单文件 `agents.json` 数组)并存;`store.rs` 根据 `AgentTemplate.storage`
//! 字段路由。目录结构:
//! ```text
//! data/agents/<agent_id>/
//!   config.json   名称/图标/分类/模型/策略等元数据(不含 system_prompt 与 tool_policy)
//!   prompt.md     system_prompt(纯文本,便于外部编辑与版本管理)
//!   tools.json    tool_policy(可选;仅当用户显式配置工具白名单时存在)
//! ```

use std::{fs, path::PathBuf};

use super::template::AgentTemplate;

/// 解耦存储根目录:data/agents/
fn decoupled_root() -> PathBuf {
    crate::paths::data_dir().join("agents")
}

/// 单个解耦智能体的目录。
fn decoupled_dir(agent_id: &str) -> PathBuf {
    decoupled_root().join(agent_id)
}

/// 把 AgentTemplate 拆为(metadata, prompt, tools)。
/// metadata 不含 system_prompt / tool_policy / overrides_builtin(运行时标记)。
fn split_agent(agent: &AgentTemplate) -> (serde_json::Value, String, Option<serde_json::Value>) {
    let mut meta = serde_json::to_value(agent).unwrap_or(serde_json::Value::Null);
    if let Some(obj) = meta.as_object_mut() {
        obj.remove("system_prompt");
        obj.remove("tool_policy");
        obj.remove("overrides_builtin");
    }
    let tools = agent
        .tool_policy
        .as_ref()
        .and_then(|v| serde_json::to_value(v).ok());
    (meta, agent.system_prompt.clone(), tools)
}

/// 保存为解耦目录。已存在则覆盖。
/// 若 agent.prompt_file / tools_file 设置了外部绝对路径,则同步写入该外部文件
/// (便于用户在外部编辑器维护);内部 prompt.md / tools.json 仍作为兜底。
pub fn save_decoupled(agent: &AgentTemplate) -> Result<(), String> {
    let dir = decoupled_dir(&agent.agent_id);
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    let (meta, prompt, tools) = split_agent(agent);
    let meta_text = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    fs::write(dir.join("config.json"), meta_text)
        .map_err(|e| format!("write config.json failed: {e}"))?;
    fs::write(dir.join("prompt.md"), prompt.clone())
        .map_err(|e| format!("write prompt.md failed: {e}"))?;
    if let Some(tv) = tools {
        let t = serde_json::to_string_pretty(&tv).map_err(|e| e.to_string())?;
        fs::write(dir.join("tools.json"), t)
            .map_err(|e| format!("write tools.json failed: {e}"))?;
    } else {
        // 无工具策略时清理旧 tools.json
        let _ = fs::remove_file(dir.join("tools.json"));
    }
    // 写入用户关联的外部文件(若指定)
    if let Some(pf) = &agent.prompt_file {
        let path = PathBuf::from(pf);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&path, &prompt).map_err(|e| format!("write prompt_file {} failed: {e}", pf))?;
    }
    if let Some(tf) = &agent.tools_file {
        let path = PathBuf::from(tf);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Some(tv) = &agent.tool_policy {
            let t = serde_json::to_string_pretty(tv).map_err(|e| e.to_string())?;
            fs::write(&path, t).map_err(|e| format!("write tools_file {} failed: {e}", tf))?;
        } else {
            let _ = fs::remove_file(&path);
        }
    }
    Ok(())
}

/// 读取所有解耦目录下的智能体。
pub fn load_decoupled() -> Vec<AgentTemplate> {
    let root = decoupled_root();
    let Ok(entries) = fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(meta_text) = fs::read_to_string(dir.join("config.json")) else {
            continue;
        };
        let Ok(mut meta) = serde_json::from_str::<serde_json::Value>(&meta_text) else {
            continue;
        };
        // 优先从用户关联的外部文件读取(若配置了且文件存在),否则回退内部文件。
        let prompt_file = meta
            .get("prompt_file")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let tools_file = meta
            .get("tools_file")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let prompt = match &prompt_file {
            Some(pf) => fs::read_to_string(pf)
                .unwrap_or_else(|_| fs::read_to_string(dir.join("prompt.md")).unwrap_or_default()),
            None => fs::read_to_string(dir.join("prompt.md")).unwrap_or_default(),
        };
        let tools = match &tools_file {
            Some(tf) => fs::read_to_string(tf)
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()),
            None => fs::read_to_string(dir.join("tools.json"))
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()),
        };
        if let Some(obj) = meta.as_object_mut() {
            obj.insert("system_prompt".into(), serde_json::Value::String(prompt));
            if let Some(tv) = tools {
                obj.insert("tool_policy".into(), tv);
            }
        }
        if let Ok(mut a) = serde_json::from_value::<AgentTemplate>(meta) {
            a.storage = Some("decoupled".into());
            a.overrides_builtin = false;
            out.push(a);
        }
    }
    out
}

/// 删除单个解耦目录。
pub fn remove_decoupled(agent_id: &str) -> Result<(), String> {
    let dir = decoupled_dir(agent_id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("remove {} failed: {e}", dir.display()))?;
    }
    Ok(())
}

/// 导出单个智能体为 JSON 字符串(耦合形式,便于分享/备份)。
pub fn export_agent(agent: &AgentTemplate) -> Result<String, String> {
    serde_json::to_string_pretty(agent).map_err(|e| e.to_string())
}

/// 导入 JSON 字符串为 AgentTemplate(不直接落盘,由调用方决定存储方式)。
pub fn import_agent(json: &str) -> Result<AgentTemplate, String> {
    serde_json::from_str(json).map_err(|e| format!("解析失败: {e}"))
}
