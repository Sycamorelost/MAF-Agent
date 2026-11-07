//! 长期记忆(知识图谱)命令:解析 MCP memory server 写入的 data/mcp-memory.jsonl。
//!
//! 与 native memory(memory.md 管线)独立:本文件只读该 JSONL 快照供前端可视化,
//! 另含清除入口(先停 server-memory 释放句柄,再删文件)。

use serde::Serialize;
use tauri::State;

use crate::commands::mcp::McpState;
use crate::mcp::{self, McpRuntime};
use crate::paths::data_dir as data_dir_fn;

/// 长期记忆中的一个实体(知识图谱节点)。
#[derive(Serialize, Clone, Debug)]
pub struct MemoryEntityInfo {
    pub name: String,
    pub entity_type: String,
    pub observations: Vec<String>,
}

/// 长期记忆中的一条关系(知识图谱边)。
#[derive(Serialize, Clone, Debug)]
pub struct MemoryRelationInfo {
    pub from: String,
    pub to: String,
    pub relation_type: String,
}

/// 长期记忆全量快照(供设置页 / 记忆页可视化)。
#[derive(Serialize, Clone, Debug)]
pub struct MemoryGraphInfo {
    pub path: String,
    pub file_bytes: u64,
    pub entities: Vec<MemoryEntityInfo>,
    pub relations: Vec<MemoryRelationInfo>,
    /// 观测条目总数(所有实体的 observations 累加)
    pub observation_count: usize,
    /// 实体类型数
    pub type_count: usize,
}

/// 解析 memory server 的 JSONL:实体按 name 合并(后写覆盖),关系按 (from,to,type) 去重;
/// 无法解析的行直接跳过,不影响其余数据。
fn parse_memory_jsonl(text: &str) -> (Vec<MemoryEntityInfo>, Vec<MemoryRelationInfo>) {
    use std::collections::{HashMap, HashSet};

    let mut entities: Vec<MemoryEntityInfo> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut relations: Vec<MemoryRelationInfo> = Vec::new();
    let mut seen_relations: HashSet<(String, String, String)> = HashSet::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match v.get("type").and_then(|t| t.as_str()) {
            Some("entity") => {
                let Some(name) = v.get("name").and_then(|x| x.as_str()) else {
                    continue;
                };
                let entity_type = v
                    .get("entityType")
                    .and_then(|x| x.as_str())
                    .unwrap_or("unknown");
                let observations: Vec<String> = v
                    .get("observations")
                    .and_then(|x| x.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|o| o.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                match index.get(name) {
                    Some(&i) => {
                        entities[i].entity_type = entity_type.to_string();
                        entities[i].observations = observations;
                    }
                    None => {
                        index.insert(name.to_string(), entities.len());
                        entities.push(MemoryEntityInfo {
                            name: name.to_string(),
                            entity_type: entity_type.to_string(),
                            observations,
                        });
                    }
                }
            }
            Some("relation") => {
                let (Some(from), Some(to)) = (
                    v.get("from").and_then(|x| x.as_str()),
                    v.get("to").and_then(|x| x.as_str()),
                ) else {
                    continue;
                };
                let relation_type = v
                    .get("relationType")
                    .and_then(|x| x.as_str())
                    .unwrap_or("related_to");
                let key = (from.to_string(), to.to_string(), relation_type.to_string());
                if seen_relations.insert(key.clone()) {
                    relations.push(MemoryRelationInfo {
                        from: key.0,
                        to: key.1,
                        relation_type: key.2,
                    });
                }
            }
            _ => {}
        }
    }
    (entities, relations)
}

/// 读取长期记忆数据(data/mcp-memory.jsonl)并解析为知识图谱快照。
/// 文件不存在 = 空图谱;单行损坏只跳过该行。
#[tauri::command]
pub async fn mcp_memory_graph() -> Result<MemoryGraphInfo, String> {
    let path = data_dir_fn().join("mcp-memory.jsonl");
    let (text, file_bytes) = match std::fs::read_to_string(&path) {
        Ok(t) => {
            let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            (t, bytes)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (String::new(), 0),
        Err(e) => return Err(format!("读取长期记忆失败: {e}")),
    };
    let (entities, relations) = parse_memory_jsonl(&text);
    let observation_count = entities.iter().map(|e| e.observations.len()).sum();
    let type_count = entities
        .iter()
        .map(|e| e.entity_type.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();
    Ok(MemoryGraphInfo {
        path: path.display().to_string(),
        file_bytes,
        entities,
        relations,
        observation_count,
        type_count,
    })
}

/// 清除「长期记忆」MCP server 的全局记忆数据(data/mcp-memory.jsonl)。
/// 先停止所有 server-memory 实例释放文件句柄,再删除文件;返回被删除的字节数(0 = 本为空)。
#[tauri::command]
pub async fn mcp_clear_memory(
    state: State<'_, McpState>,
    runtime: State<'_, McpRuntime>,
) -> Result<u64, String> {
    let names: Vec<String> = state
        .0
        .lock()
        .map_err(|e| format!("lock 失败: {e}"))?
        .servers
        .iter()
        .filter(|s| s.args.iter().any(|a| a.contains("server-memory")))
        .map(|s| s.name.clone())
        .collect();
    for n in &names {
        runtime.stop_server(n).await;
    }
    let path = data_dir_fn().join("mcp-memory.jsonl");
    if !path.exists() {
        return Ok(0);
    }
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    std::fs::remove_file(&path).map_err(|e| format!("删除记忆文件失败: {e}"))?;
    mcp::audit(
        &data_dir_fn(),
        "memory_clear",
        "长期记忆",
        "-",
        "user_clear",
    );
    Ok(size)
}

#[cfg(test)]
mod tests {
    /// 长期记忆 JSONL:实体按 name 合并(后写覆盖)、关系去重、坏行跳过。
    #[test]
    fn memory_jsonl_parses_and_merges() {
        use super::parse_memory_jsonl;
        let text = r#"
{"type":"entity","name":"Rust","entityType":"language","observations":["fast"]}
{"type":"entity","name":"Alice","entityType":"person","observations":["engineer"]}
{"type":"relation","from":"Alice","to":"Rust","relationType":"uses"}
{"type":"relation","from":"Alice","to":"Rust","relationType":"uses"}
{"type":"entity","name":"Rust","entityType":"language","observations":["fast","safe"]}
not json at all
{"type":"entity","name":"Alice","entityType":"person","observations":["engineer","reader"]}
"#;
        let (entities, relations) = parse_memory_jsonl(text);
        assert_eq!(entities.len(), 2, "同名实体应合并");
        let rust = entities.iter().find(|e| e.name == "Rust").unwrap();
        assert_eq!(rust.observations, vec!["fast", "safe"], "后写应覆盖");
        assert_eq!(relations.len(), 1, "重复关系应去重");
        assert_eq!(relations[0].relation_type, "uses");
    }

    /// 空文件/只有坏行:返回空图谱而不是报错。
    #[test]
    fn memory_jsonl_empty_or_broken_is_safe() {
        use super::parse_memory_jsonl;
        assert!(parse_memory_jsonl("").0.is_empty());
        let (entities, relations) = parse_memory_jsonl("{\"type\":\"entity\"}\n{oops\n");
        assert!(entities.is_empty());
        assert!(relations.is_empty());
    }
}
