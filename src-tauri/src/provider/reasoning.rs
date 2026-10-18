//! 模型思考能力规则表(v1.9.0 工作块 I,improving.md §1.4.2.5)。
//!
//! 解析顺序(能力声明链):
//!   1. 手动修正 — `provider_models.json` 模型对象的 `reasoning` 字段(用户在模型卡标注,优先);
//!   2. 内置规则表 — 按模型名前缀/关键词推断(覆盖五家内置模板常见思考型号);
//!   3. 推断不出 → 按「不支持」处理,前端隐藏控件(保守策略,避免对不认扩展字段的端点 400)。
//!
//! 说明:厂家 `/models` 接口不返回思考能力元数据,因此 `reasoning` 字段
//! 实际只由手动修正写入;读取顺序上「缓存字段优先于推断」即等价于「手动修正优先」。

use serde::{Deserialize, Serialize};

use super::model_cards;

/// 一个模型的思考能力声明。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReasoningCapability {
    /// 是否支持思考控制(false = 控件整体隐藏)
    pub enabled: bool,
    /// 是否允许关闭(false = 不可关模型,Kimi K3 / GLM-5.3 等,UI 锁定为开)
    pub can_disable: bool,
    /// 是否有档位差异(false = 仅开关,如 Kimi K2.6;四档控件整体禁用)
    pub levels: bool,
}

/// 不支持思考(隐藏控件)。
pub const UNSUPPORTED: ReasoningCapability = ReasoningCapability {
    enabled: false,
    can_disable: false,
    levels: false,
};

/// 解析某模型的能力声明(手动修正 > 内置规则表推断 > 不支持)。
pub fn resolve(provider_id: &str, model: &str) -> ReasoningCapability {
    // 1. 手动修正:provider_models.json 缓存条目的 reasoning 字段
    for f in model_cards::load_model_cache()
        .get(provider_id)
        .into_iter().flatten()
    {
        if f.id == model {
            if let Some(cap) = f.reasoning {
                return cap;
            }
            break;
        }
    }
    // 2. 内置规则表推断
    infer(provider_id, model)
}

/// 内置型号规则表(保守:宁可不显示控件,不可对不支持端点发扩展字段)。
pub fn infer(provider_id: &str, model: &str) -> ReasoningCapability {
    let m = model.to_ascii_lowercase();
    let cap = |enabled: bool, can_disable: bool, levels: bool| ReasoningCapability {
        enabled,
        can_disable,
        levels,
    };
    match provider_id {
        // OpenAI:gpt-5 系列 minimal≈关闭;o1/o3/o4 系列恒开仅有档
        "openai" => {
            if m.starts_with("gpt-5") {
                cap(true, true, true)
            } else if m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") {
                cap(true, false, true)
            } else {
                UNSUPPORTED
            }
        }
        // DeepSeek:R1/reasoner 恒开;V3.2+/V4 混合思考可关有档
        "deepseek" => {
            if m.contains("r1") || m.contains("reasoner") {
                cap(true, false, false)
            } else if m.contains("v3") || m.contains("v4") || m.contains("flash") {
                cap(true, true, true)
            } else {
                UNSUPPORTED
            }
        }
        // Kimi:k3 / k2.7 不可关有档;k2.x(k2.6)仅开关
        "kimi" => {
            if m.contains("k3") || m.contains("k2.7") {
                cap(true, false, true)
            } else if m.contains("k2") {
                cap(true, true, false)
            } else {
                UNSUPPORTED
            }
        }
        // 通义:qwen3 开源系 / 商用主力(max/plus/turbo)/ thinking 变体均可关有档(预算式)
        "qwen" => {
            if m.starts_with("qwen3")
                || m.starts_with("qwen-max")
                || m.starts_with("qwen-plus")
                || m.starts_with("qwen-turbo")
                || m.contains("think")
            {
                cap(true, true, true)
            } else {
                UNSUPPORTED
            }
        }
        // 智谱:glm-5.3 不可关有档;glm-5.2 可关有档;glm-4.5/4.6 仅开关(thinking.type)
        "zhipu" => {
            if m.contains("glm-5.3") {
                cap(true, false, true)
            } else if m.contains("glm-5") {
                cap(true, true, true)
            } else if m.contains("glm-4.5") || m.contains("glm-4.6") {
                cap(true, true, false)
            } else {
                UNSUPPORTED
            }
        }
        // 自定义 / 未知模板:按通用关键词保守推断,命中即给「可关 + 有档」
        _ => {
            const KEYS: [&str; 10] = [
                "r1", "reasoner", "think", "o3", "o4", "gpt-5", "qwen3", "glm-5", "glm-4.5",
                "glm-4.6",
            ];
            if KEYS.iter().any(|k| m.contains(k)) {
                cap(true, true, true)
            } else {
                UNSUPPORTED
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_family() {
        assert!(infer("openai", "gpt-5.2").enabled);
        assert!(infer("openai", "gpt-5").can_disable);
        assert!(infer("openai", "o3-mini").enabled && !infer("openai", "o3-mini").can_disable);
        assert!(!infer("openai", "gpt-4o").enabled);
    }

    #[test]
    fn deepseek_family() {
        let r1 = infer("deepseek", "deepseek-reasoner");
        assert!(r1.enabled && !r1.can_disable && !r1.levels);
        let v32 = infer("deepseek", "deepseek-v3.2");
        assert!(v32.enabled && v32.can_disable && v32.levels);
        assert!(!infer("deepseek", "deepseek-coder").enabled);
    }

    #[test]
    fn kimi_family() {
        let k3 = infer("kimi", "kimi-k3-latest");
        assert!(k3.enabled && !k3.can_disable && k3.levels);
        let k26 = infer("kimi", "kimi-k2.6");
        assert!(k26.enabled && k26.can_disable && !k26.levels);
        assert!(!infer("kimi", "moonshot-v1-8k").enabled);
    }

    #[test]
    fn qwen_family() {
        assert!(infer("qwen", "qwen3-max").enabled);
        assert!(infer("qwen", "qwen-plus-latest").enabled);
        assert!(infer("qwen", "qwen-turbo").enabled);
        assert!(!infer("qwen", "qwen2.5-72b-instruct").enabled);
    }

    #[test]
    fn zhipu_family() {
        let g53 = infer("zhipu", "glm-5.3");
        assert!(g53.enabled && !g53.can_disable && g53.levels);
        let g52 = infer("zhipu", "glm-5.2");
        assert!(g52.enabled && g52.can_disable && g52.levels);
        let g46 = infer("zhipu", "glm-4.6");
        assert!(g46.enabled && g46.can_disable && !g46.levels);
        assert!(!infer("zhipu", "glm-4-air").enabled);
    }

    #[test]
    fn custom_conservative() {
        assert!(infer("custom", "my-reasoner-x").enabled);
        assert!(!infer("custom", "llama-3.1-8b").enabled);
        // 大小写不敏感
        assert!(infer("custom", "DeepSeek-R1-Distill").enabled);
    }
}
