//! 运行时预设(P5 / D18 / D19):预设不是代码分支,而是一份可合并的配置清单。
//!
//! - `builtin`:内置四档(经济/标准/深度/代码),只读;
//! - `resolve`:三级合并(会话钉住 > Agent 模板默认 > 全局默认)与工具名单匹配;
//! - 用户覆盖文件 `<data_dir>/presets.json`:同 id 整体替换内置档,新 id 追加(本轮无编辑 UI)。
//!
//! 预设与布局正交、与权限解耦:workbench_mode 管界面,preset 管工具/上下文/检索,
//! permission_mode 管审批;`approval` 字段只是默认建议值,最终以会话 permission_mode 为准。

pub mod builtin;
pub mod resolve;

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

/// 内置预设清单版本。会话创建时把该版本钉进 sessions.preset_version,
/// 老会话不因内置预设升级而漂移(D19)。
pub const PRESETS_VERSION: i64 = 1;

/// 全局默认档(设置缺省;val.md §10.2 建议「标准」)。
pub const DEFAULT_PRESET: &str = "standard";

/// 工具名单:include 命中(含 `*`)且未被 exclude 命中即放行;
/// exclude 的 `*` 只排除「未显式 include」的工具(白名单模式)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolFilter {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// 工具结果裁剪策略(threshold 触发,保留 head/tail)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrunePolicy {
    pub threshold_chars: u32,
    pub head: u32,
    pub tail: u32,
}

/// 上下文策略:max_messages = 注入 LLM 的最近消息条数上限;max_chars 为压缩触发参考。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextPolicy {
    pub max_messages: u32,
    pub max_chars: u32,
    pub prune: PrunePolicy,
}

/// 记忆注入建议(展示用;实际注入仍以 sessions.memory_injection_strategy 链为准)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryPolicy {
    pub strategy: String,
    pub max_tokens: u32,
}

/// 检索策略(P7 消费):lexical = trigram3;vector/rerank 为混合检索开关;top_k 为返回块数。
/// 本地无 embedding 设施,语义约定:vector=true → 启用第二路词法信号(覆盖率打分)
/// 与 FTS 路做 RRF 混合;rerank=true → 召回 top20 按覆盖率 + 章节命中软重排后截 top_k。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetrievalPolicy {
    pub lexical: String,
    pub vector: bool,
    pub rerank: bool,
    pub top_k: u32,
}

/// 运行时预设(与 presets.json 结构对齐)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub order: i32,
    #[serde(default)]
    pub description: String,
    pub tools: ToolFilter,
    pub context: ContextPolicy,
    pub memory: MemoryPolicy,
    /// 默认审批建议值(ask/workspace/full);实际审批以会话 permission_mode 为准。
    pub approval: String,
    pub retrieval: RetrievalPolicy,
}

#[derive(Debug, Deserialize)]
pub(super) struct PresetsFile {
    #[serde(default)]
    pub(super) presets: Vec<Preset>,
}

/// 全部预设:内置四档 + 用户覆盖文件(同 id 整体替换,新 id 追加),按 order 排序。
/// 覆盖文件缺失/损坏时静默忽略,保证本地优先可用。
pub fn list(dir: &Path) -> Vec<Preset> {
    let mut out = builtin::presets();
    if let Ok(text) = fs::read_to_string(dir.join("presets.json"))
        && let Ok(file) = serde_json::from_str::<PresetsFile>(&text)
    {
        for p in file.presets {
            if let Some(slot) = out.iter_mut().find(|b| b.id == p.id) {
                *slot = p;
            } else {
                out.push(p);
            }
        }
    }
    out.sort_by_key(|p| p.order);
    out
}

/// 按 id 取预设(含用户覆盖)。
pub fn get(dir: &Path, id: &str) -> Option<Preset> {
    list(dir).into_iter().find(|p| p.id == id)
}

/// 按会话取生效检索策略(P7):三级合并(会话钉住 > Agent 模板 > 全局默认)后的
/// preset.retrieval。与 `commands::presets::resolve_for` 同逻辑;放在本模块是为让
/// mcp / doc 层不反向依赖 commands。会话不存在或任一环节失败时回落内置 standard。
pub fn retrieval_for_session(session_id: &str) -> RetrievalPolicy {
    let fallback = || {
        builtin::presets()
            .into_iter()
            .find(|p| p.id == DEFAULT_PRESET)
            .expect("内置预设必须含 standard")
            .retrieval
    };
    let Ok(Some(s)) = crate::db::sessions::get(session_id) else {
        return fallback();
    };
    let all = list(&crate::paths::data_dir());
    let agent_preset = s
        .agent_id
        .as_deref()
        .and_then(crate::agents::find)
        .and_then(|t| t.preset);
    let default = crate::settings::load().default_preset;
    resolve::resolve(&all, s.preset_id.as_deref(), agent_preset.as_deref(), &default)
        .preset
        .retrieval
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_four_presets_parse_in_order() {
        let list = builtin::presets();
        assert_eq!(list.len(), 4);
        assert_eq!(
            list.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            vec!["economy", "standard", "deep", "code"]
        );
        // 标准档为全局默认且存在
        assert!(list.iter().any(|p| p.id == DEFAULT_PRESET));
        // 合法审批建议值
        for p in &list {
            assert!(
                ["ask", "workspace", "full"].contains(&p.approval.as_str()),
                "非法 approval:{}",
                p.approval
            );
        }
    }

    #[test]
    fn user_override_replaces_builtin_by_id() {
        let dir = std::env::temp_dir().join(format!("maf-presets-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("presets.json"),
            r#"{"presets_version":1,"presets":[
              {"id":"economy","name":"经济·改","order":1,"description":"",
               "tools":{"include":["*"],"exclude":[]},
               "context":{"max_messages":9,"max_chars":9000,"prune":{"threshold_chars":1,"head":1,"tail":1}},
               "memory":{"strategy":"none","max_tokens":1},"approval":"full",
               "retrieval":{"lexical":"trigram3","vector":false,"rerank":false,"top_k":1}}
            ]}"#,
        )
        .unwrap();
        let list = list(&dir);
        assert_eq!(list.len(), 4);
        let eco = list.iter().find(|p| p.id == "economy").unwrap();
        assert_eq!(eco.name, "经济·改");
        assert_eq!(eco.context.max_messages, 9);
        let _ = fs::remove_dir_all(&dir);
    }
}
