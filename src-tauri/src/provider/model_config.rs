//! 单模型配置(迭代 D8)。
//!
//! 设置页可为每个 Provider 下的每个模型做独立配置:
//!   - enabled:是否启用(停用后不出现在「切换模型」列表)
//!   - display_name:显示名覆盖(仅影响 UI)
//!   - order:排序权重(仅影响 UI)
//!   - temperature / max_tokens / top_p:默认生成参数(input 未显式传值时生效)
//!   - memory_injection:切换到此模型时的默认记忆注入策略
//!
//! 落盘:`<repo>/data/provider_model_config.json`
//! 结构:`{ provider_id: { model_id: ModelOverride } }`。
//! 全部字段可缺省;缺省 = 不覆盖,走原有默认行为。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 单模型覆盖项。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelOverride {
    /// 是否启用(缺省 true)
    #[serde(default)]
    pub enabled: Option<bool>,
    /// 显示名覆盖(空串视为不覆盖)
    #[serde(default)]
    pub display_name: Option<String>,
    /// 排序权重(越小越靠前;缺省排在已指定项之后,保持原相对顺序)
    #[serde(default)]
    pub order: Option<i32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    /// full / summary / hybrid / none
    #[serde(default)]
    pub memory_injection: Option<String>,
    /// 默认思考强度(v1.9.0 工作块 I;light/medium/high/ultra,缺省 = 跟随全局默认 medium)。
    /// 仅对支持思考的模型生效;能力判定见 provider/reasoning.rs。
    #[serde(default)]
    pub default_reasoning: Option<String>,
}

/// provider_id -> model_id -> 覆盖项。
pub type ProviderModelConfig = HashMap<String, HashMap<String, ModelOverride>>;

/// 配置文件:`<data_dir>/provider_model_config.json`。
fn config_path() -> PathBuf {
    crate::paths::data_dir().join("provider_model_config.json")
}

/// 读取配置(不存在/损坏返回空表)。
pub fn load() -> ProviderModelConfig {
    let p = config_path();
    if !p.exists() {
        return Default::default();
    }
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 保存配置。
pub fn save(cfg: &ProviderModelConfig) -> Result<(), String> {
    let p = config_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir data failed: {e}"))?;
    }
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&p, text).map_err(|e| format!("write provider_model_config.json failed: {e}"))
}

/// 取某 provider + model 的覆盖项。
pub fn get<'a>(
    cfg: &'a ProviderModelConfig,
    provider_id: &str,
    model: &str,
) -> Option<&'a ModelOverride> {
    cfg.get(provider_id).and_then(|m| m.get(model))
}

/// 应用「排序」覆盖(稳定排序:未指定 order 的项保持原有相对顺序并排在已指定项之后)。
pub fn sort_models(models: Vec<String>, cfg: &ProviderModelConfig, provider_id: &str) -> Vec<String> {
    let per = cfg.get(provider_id);
    let mut items: Vec<(usize, String, i32)> = models
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let ord = per
                .and_then(|p| p.get(&m))
                .and_then(|o| o.order)
                .unwrap_or(i32::MAX);
            (i, m, ord)
        })
        .collect();
    items.sort_by_key(|(i, _, ord)| (*ord, *i));
    items.into_iter().map(|(_, m, _)| m).collect()
}

/// 过滤掉被用户停用的模型(缺省 enabled = true)。
pub fn filter_enabled(models: Vec<String>, cfg: &ProviderModelConfig, provider_id: &str) -> Vec<String> {
    let per = cfg.get(provider_id);
    models
        .into_iter()
        .filter(|m| {
            per.and_then(|p| p.get(m))
                .and_then(|o| o.enabled)
                .unwrap_or(true)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with(provider: &str, model: &str, ov: ModelOverride) -> ProviderModelConfig {
        let mut cfg: ProviderModelConfig = Default::default();
        cfg.entry(provider.into())
            .or_default()
            .insert(model.into(), ov);
        cfg
    }

    #[test]
    fn disabled_model_is_filtered() {
        let cfg = cfg_with("openai", "b", ModelOverride {
            enabled: Some(false),
            ..Default::default()
        });
        let all = sort_models(vec!["a".into(), "b".into(), "c".into()], &cfg, "openai");
        assert_eq!(all, vec!["a", "b", "c"]);
        let enabled = filter_enabled(all, &cfg, "openai");
        assert_eq!(enabled, vec!["a", "c"]);
    }

    #[test]
    fn order_moves_model_to_front_and_keeps_rest() {
        let cfg = cfg_with("openai", "c", ModelOverride {
            order: Some(0),
            ..Default::default()
        });
        let out = sort_models(vec!["a".into(), "b".into(), "c".into()], &cfg, "openai");
        assert_eq!(out, vec!["c", "a", "b"]);
    }

    #[test]
    fn order_sorted_ascending() {
        let mut cfg: ProviderModelConfig = Default::default();
        cfg.entry("p".into()).or_default().insert("a".into(), ModelOverride {
            order: Some(5),
            ..Default::default()
        });
        cfg.entry("p".into()).or_default().insert("b".into(), ModelOverride {
            order: Some(1),
            ..Default::default()
        });
        let out = sort_models(vec!["a".into(), "b".into(), "c".into()], &cfg, "p");
        assert_eq!(out, vec!["b", "a", "c"]);
    }

    #[test]
    fn roundtrip_serialization() {
        let cfg = cfg_with("deepseek", "deepseek-chat", ModelOverride {
            display_name: Some("DeepSeek 对话".into()),
            temperature: Some(0.3),
            max_tokens: Some(4096),
            memory_injection: Some("hybrid".into()),
            ..Default::default()
        });
        let text = serde_json::to_string(&cfg).unwrap();
        let back: ProviderModelConfig = serde_json::from_str(&text).unwrap();
        let ov = get(&back, "deepseek", "deepseek-chat").unwrap();
        assert_eq!(ov.display_name.as_deref(), Some("DeepSeek 对话"));
        assert_eq!(ov.temperature, Some(0.3));
        assert_eq!(ov.max_tokens, Some(4096));
        assert_eq!(ov.memory_injection.as_deref(), Some("hybrid"));
    }
}
