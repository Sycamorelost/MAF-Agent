//! 研究预设(迭代 D4):参数组合的保存 / 复用 / 删除。
//!
//! 落盘:`data/research/presets.json`(与 research 会话目录同级)。

use std::fs;
use std::path::PathBuf;

use chrono::Local;
use serde::{Deserialize, Serialize};

use super::ResearchParams;

/// 一条研究预设。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchPreset {
    pub name: String,
    pub params: ResearchParams,
    pub created: String,
}

/// presets.json 路径:`<data_dir>/research/presets.json`
fn presets_path() -> PathBuf {
    crate::paths::data_dir().join("research").join("presets.json")
}

/// 读取全部预设(文件缺失 / 损坏时返回空表,不报错)。
pub fn load_presets() -> Vec<ResearchPreset> {
    let p = presets_path();
    if !p.exists() {
        return Vec::new();
    }
    let Ok(text) = fs::read_to_string(&p) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn write_all(presets: &[ResearchPreset]) -> Result<(), String> {
    let p = presets_path();
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {} failed: {e}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(presets).map_err(|e| e.to_string())?;
    fs::write(&p, text).map_err(|e| format!("write presets.json failed: {e}"))
}

/// 保存预设(同名覆盖)。
pub fn save_preset(name: &str, params: ResearchParams) -> Result<Vec<ResearchPreset>, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("预设名称不能为空".into());
    }
    let mut list = load_presets();
    if let Some(existing) = list.iter_mut().find(|x| x.name == name) {
        existing.params = params;
    } else {
        list.push(ResearchPreset {
            name,
            params,
            created: Local::now().to_rfc3339(),
        });
    }
    write_all(&list)?;
    Ok(list)
}

/// 按名删除预设。
pub fn delete_preset(name: &str) -> Result<Vec<ResearchPreset>, String> {
    let mut list = load_presets();
    list.retain(|x| x.name != name);
    write_all(&list)?;
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_serde_roundtrip_with_defaults() {
        // 缺字段时走默认(向后兼容旧预设文件)
        let json = r#"{"max_rounds": 5}"#;
        let p: ResearchParams = serde_json::from_str(json).unwrap();
        assert_eq!(p.max_rounds, 5);
        assert_eq!(p.queries_per_round, 4);
        assert_eq!(p.source_mode, "web");
        assert_eq!(p.reflect, "every");
        assert_eq!(p.output_lang, "zh");
    }

    #[test]
    fn rewrite_query_modes() {
        let mut p = ResearchParams::default();
        assert_eq!(p.rewrite_query("AI 记忆"), "AI 记忆");
        p.source_mode = "academic".into();
        assert!(p.rewrite_query("AI 记忆").contains("学术"));
        p.source_mode = "whitelist".into();
        p.whitelist_domains = vec!["arxiv.org".into(), "github.com".into()];
        let q = p.rewrite_query("LLM agent");
        assert!(q.contains("site:arxiv.org"));
        assert!(q.contains("OR"));
    }
}
