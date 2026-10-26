//! OpenAI 兼容请求/响应体序列化与思考参数方言映射。

use serde_json::json;

use crate::provider::{ChatMessage, EffortLevel, ImageResponse, ProviderError, ReasoningSpec};

use super::OpenAiCompat;

impl OpenAiCompat {
    /// 序列化单条 ChatMessage:纯文本或 vision content 数组(兼容 OpenAI / DeepSeek 等)。
    pub(super) fn serialize_message(m: &ChatMessage) -> serde_json::Value {
        let mut v = if m.images.is_empty() {
            json!({
                "role": m.role,
                "content": m.content,
            })
        } else {
            let mut parts = vec![json!({
                "type": "text",
                "text": m.content,
            })];
            for data_url in &m.images {
                parts.push(json!({
                    "type": "image_url",
                    "image_url": { "url": data_url },
                }));
            }
            json!({
                "role": m.role,
                "content": parts,
            })
        };
        // Phase 5e:多轮工具调用的必要字段(缺任一字段模型会 400)
        if let Some(calls) = &m.tool_calls {
            v["tool_calls"] = json!(calls);
        }
        if let Some(id) = &m.tool_call_id {
            v["tool_call_id"] = json!(id);
        }
        v
    }

    /// 按Provider 方言把统一思考规格翻译成请求参数(improving.md §1.4 映射表)。
    ///
    /// 约束:
    ///   - 只发官方合法值,不杜撰字段;档位缺失时向上退化(如仅 low/high/max 三档的模型
    ///     Medium→high、Ultra→max),退化差异由前端 tooltip 透明展示;
    ///   - Off 对「传 disabled 会报错」的不可关模型(GLM-5.3 / Kimi K3)干脆不发参数(防御);
    ///   - 自定义 / 未知模板只发 OpenAI 标准 `reasoning_effort`,不发扩展字段,最大化中转站兼容。
    pub(super) fn apply_reasoning_params(
        body: &mut serde_json::Value,
        provider_id: &str,
        model: &str,
        spec: ReasoningSpec,
    ) {
        let m = model.to_ascii_lowercase();
        // 仅 low/high/max 三档模型的统一档位退化映射(OpenAI 例外:有 medium)
        let low_high_max = |l: EffortLevel| match l {
            EffortLevel::Light => "low",
            EffortLevel::Medium | EffortLevel::High => "high",
            EffortLevel::Ultra => "max",
        };
        match provider_id {
            // OpenAI:顶层 reasoning_effort minimal/low/medium/high;minimal 仅 gpt-5 系接受,
            // o 系列不认 minimal → Off 不发参数
            "openai" => match spec {
                ReasoningSpec::Effort { level } => {
                    body["reasoning_effort"] = json!(match level {
                        EffortLevel::Light => "low",
                        EffortLevel::Medium => "medium",
                        EffortLevel::High | EffortLevel::Ultra => "high",
                    })
                }
                ReasoningSpec::Budget { .. } => {}
                ReasoningSpec::Off => {
                    if m.starts_with("gpt-5") {
                        body["reasoning_effort"] = json!("minimal");
                    }
                }
            },
            // DeepSeek:thinking.type + 顶层 reasoning_effort none/low/high/max
            "deepseek" => match spec {
                ReasoningSpec::Effort { level } => {
                    body["thinking"] = json!({ "type": "enabled" });
                    body["reasoning_effort"] = json!(low_high_max(level));
                }
                ReasoningSpec::Budget { .. } => {}
                ReasoningSpec::Off => body["thinking"] = json!({ "type": "disabled" }),
            },
            // Kimi:K3/K2.7 不可关,仅顶层 reasoning_effort low/high/max(不接受 thinking.type);
            // K2.6 等仅开关(thinking.type),无档位
            "kimi" => {
                if m.contains("k3") || m.contains("k2.7") {
                    if let ReasoningSpec::Effort { level } = spec {
                        body["reasoning_effort"] = json!(low_high_max(level));
                    }
                } else {
                    match spec {
                        ReasoningSpec::Effort { .. } | ReasoningSpec::Budget { .. } => {
                            body["thinking"] = json!({ "type": "enabled" });
                        }
                        ReasoningSpec::Off => body["thinking"] = json!({ "type": "disabled" }),
                    }
                }
            }
            // 通义:enable_thinking + thinking_budget(Light≈2k / Medium≈8k / High≈24k / Ultra 不设上限)
            "qwen" => match spec {
                ReasoningSpec::Effort { level } => {
                    body["enable_thinking"] = json!(true);
                    match level {
                        EffortLevel::Light => body["thinking_budget"] = json!(2048),
                        EffortLevel::Medium => body["thinking_budget"] = json!(8192),
                        EffortLevel::High => body["thinking_budget"] = json!(24576),
                        // Ultra:不设上限,不发送 thinking_budget
                        EffortLevel::Ultra => {}
                    }
                }
                ReasoningSpec::Budget { tokens } => {
                    body["enable_thinking"] = json!(true);
                    body["thinking_budget"] = json!(tokens);
                }
                ReasoningSpec::Off => body["enable_thinking"] = json!(false),
            },
            // 智谱:thinking.type;GLM-5.3 附档(不可关,Off 防御性不发);
            // GLM-5.2 附档(全档);GLM-4.5/4.6 仅开关无档
            "zhipu" => {
                if m.contains("glm-5.3") {
                    if let ReasoningSpec::Effort { level } = spec {
                        body["thinking"] = json!({ "type": "enabled" });
                        body["reasoning_effort"] = json!(low_high_max(level));
                    }
                } else if m.contains("glm-5.2") {
                    match spec {
                        ReasoningSpec::Effort { level } => {
                            body["thinking"] = json!({ "type": "enabled" });
                            body["reasoning_effort"] = json!(match level {
                                EffortLevel::Light => "low",
                                EffortLevel::Medium => "medium",
                                EffortLevel::High => "high",
                                EffortLevel::Ultra => "max",
                            });
                        }
                        ReasoningSpec::Budget { .. } => {
                            body["thinking"] = json!({ "type": "enabled" });
                        }
                        ReasoningSpec::Off => body["thinking"] = json!({ "type": "disabled" }),
                    }
                } else {
                    match spec {
                        ReasoningSpec::Effort { .. } | ReasoningSpec::Budget { .. } => {
                            body["thinking"] = json!({ "type": "enabled" });
                        }
                        ReasoningSpec::Off => body["thinking"] = json!({ "type": "disabled" }),
                    }
                }
            }
            // 自定义 / 其他:仅发 OpenAI 标准 reasoning_effort(最通用),Off 不发参数
            _ => {
                if let ReasoningSpec::Effort { level } = spec {
                    body["reasoning_effort"] = json!(match level {
                        EffortLevel::Light => "low",
                        EffortLevel::Medium => "medium",
                        EffortLevel::High | EffortLevel::Ultra => "high",
                    });
                }
            }
        }
    }

    /// 解析 images/generations 响应 JSON,兼容 b64_json 与 url 两种返回。
    pub(super) fn parse_image_response(
        v: &serde_json::Value,
    ) -> Result<ImageResponse, ProviderError> {
        let item = v
            .get("data")
            .and_then(|d| d.get(0))
            .ok_or_else(|| ProviderError::Parse("响应缺少 data[0]".into()))?;
        let revised = item
            .get("revised_prompt")
            .and_then(|r| r.as_str())
            .map(|s| s.to_string());
        if let Some(b64) = item.get("b64_json").and_then(|b| b.as_str()) {
            return Ok(ImageResponse {
                url: format!("data:image/png;base64,{b64}"),
                revised_prompt: revised,
            });
        }
        if let Some(u) = item.get("url").and_then(|u| u.as_str()) {
            return Ok(ImageResponse {
                url: u.to_string(),
                revised_prompt: revised,
            });
        }
        Err(ProviderError::Parse(
            "data[0] 既无 b64_json 也无 url".into(),
        ))
    }
}
