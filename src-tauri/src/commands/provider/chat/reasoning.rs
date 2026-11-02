//! 思考档位解析与合并(请求显式值 > 单模型默认档 > 全局默认 medium)。

use crate::provider::{EffortLevel, ReasoningSpec, reasoning};

/// 解析统一思考档位字符串 → ReasoningSpec;非法值返回 None(静默不发送,保守)。
pub(super) fn parse_reasoning(s: &str) -> Option<ReasoningSpec> {
    match s {
        "off" => Some(ReasoningSpec::Off),
        "light" => Some(ReasoningSpec::Effort {
            level: EffortLevel::Light,
        }),
        "medium" => Some(ReasoningSpec::Effort {
            level: EffortLevel::Medium,
        }),
        "high" => Some(ReasoningSpec::Effort {
            level: EffortLevel::High,
        }),
        "ultra" => Some(ReasoningSpec::Effort {
            level: EffortLevel::Ultra,
        }),
        _ => None,
    }
}

/// 合并思考档位(优先级:请求显式值 > 单模型默认档 > 全局默认 medium)。
/// 前提:该模型具备思考能力(reasoning::resolve),否则一律 None(不发送,避免 400)。
pub(super) fn merge_reasoning(
    input_level: Option<&str>,
    ov_default: Option<&str>,
    provider_id: &str,
    model: &str,
) -> Option<ReasoningSpec> {
    let cap = reasoning::resolve(provider_id, model);
    if !cap.enabled {
        return None;
    }
    input_level
        .and_then(parse_reasoning)
        .or_else(|| ov_default.and_then(parse_reasoning))
        .or_else(|| parse_reasoning("medium"))
}
