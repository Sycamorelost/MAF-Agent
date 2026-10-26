//! OpenAI 兼容 Provider 单测:chunk 解析 / 图像响应 / 消息序列化 / 思考参数映射。

use super::OpenAiCompat;
use crate::provider::{ChatMessage, EffortLevel, ReasoningSpec, StreamChunk};

#[test]
fn parse_delta() {
    let data =
        r#"{"id":"x","choices":[{"index":0,"delta":{"content":"你好"},"finish_reason":null}]}"#;
    match OpenAiCompat::parse_chunk(data) {
        Some(StreamChunk::Delta { content }) => assert_eq!(content, "你好"),
        _ => panic!("expected delta"),
    }
}

#[test]
fn parse_done() {
    let data = r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":20}}"#;
    match OpenAiCompat::parse_chunk(data) {
        Some(StreamChunk::Done {
            finish_reason,
            prompt_tokens,
            completion_tokens,
            ..
        }) => {
            assert_eq!(finish_reason, Some("stop".into()));
            assert_eq!(prompt_tokens, Some(10));
            assert_eq!(completion_tokens, Some(20));
        }
        _ => panic!("expected done"),
    }
}

#[test]
fn parse_done_marker() {
    match OpenAiCompat::parse_chunk("[DONE]") {
        Some(StreamChunk::Done { .. }) => {}
        _ => panic!("expected done"),
    }
}

#[test]
fn parse_garbage_returns_none() {
    assert!(OpenAiCompat::parse_chunk("not json").is_none());
}

/// stream_options.include_usage 的收尾 chunk:choices 为空、仅带 usage。
#[test]
fn parse_usage_only_chunk() {
    let data =
        r#"{"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":34,"total_tokens":46}}"#;
    match OpenAiCompat::parse_chunk(data) {
        Some(StreamChunk::Usage {
            input_tokens,
            output_tokens,
            total_tokens,
            ..
        }) => {
            assert_eq!(input_tokens, Some(12));
            assert_eq!(output_tokens, Some(34));
            assert_eq!(total_tokens, Some(46));
        }
        _ => panic!("expected usage"),
    }
}

/// 空 choices 且无 usage 的 chunk 仍被忽略。
#[test]
fn parse_empty_choices_without_usage_is_none() {
    assert!(OpenAiCompat::parse_chunk(r#"{"choices":[]}"#).is_none());
    assert!(OpenAiCompat::parse_chunk(r#"{"choices":[],"usage":null}"#).is_none());
}

/// V17:缓存命中 token 解析 + reasoning 兜底(output < reasoning 时取 reasoning)。
#[test]
fn parse_usage_cached_tokens_and_reasoning_floor() {
    let data = r#"{"choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":20,"total_tokens":120,"prompt_tokens_details":{"cached_tokens":60},"completion_tokens_details":{"reasoning_tokens":30}}}"#;
    match OpenAiCompat::parse_chunk(data) {
        Some(StreamChunk::Done {
            prompt_tokens,
            completion_tokens,
            reasoning_tokens,
            cached_tokens,
            ..
        }) => {
            assert_eq!(prompt_tokens, Some(100));
            assert_eq!(
                completion_tokens,
                Some(30),
                "reasoning 已含在 output 内,不得小于 reasoning"
            );
            assert_eq!(reasoning_tokens, Some(30));
            assert_eq!(cached_tokens, Some(60));
        }
        _ => panic!("expected done"),
    }
}

/// 缓存命中 token 超过输入时按输入封顶;Anthropic 风格字段同样可识别。
#[test]
fn parse_cached_tokens_clamped_and_anthropic_style() {
    let data = r#"{"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5,"cache_read_input_tokens":99}}"#;
    match OpenAiCompat::parse_chunk(data) {
        Some(StreamChunk::Usage { cached_tokens, .. }) => {
            assert_eq!(cached_tokens, Some(10), "命中数不得超过输入")
        }
        _ => panic!("expected usage"),
    }
}

#[test]
fn parse_image_b64() {
    let v = serde_json::json!({
        "data": [{ "b64_json": "aGVsbG8=", "revised_prompt": "rev" }]
    });
    let r = OpenAiCompat::parse_image_response(&v).expect("b64 ok");
    assert_eq!(r.url, "data:image/png;base64,aGVsbG8=");
    assert_eq!(r.revised_prompt, Some("rev".into()));
}

#[test]
fn parse_image_url() {
    let v = serde_json::json!({
        "data": [{ "url": "https://example.com/x.png" }]
    });
    let r = OpenAiCompat::parse_image_response(&v).expect("url ok");
    assert_eq!(r.url, "https://example.com/x.png");
    assert_eq!(r.revised_prompt, None);
}

#[test]
fn parse_image_missing_data() {
    let v = serde_json::json!({ "data": [] });
    assert!(OpenAiCompat::parse_image_response(&v).is_err());
    let v2 = serde_json::json!({ "data": [{ "foo": 1 }] });
    assert!(OpenAiCompat::parse_image_response(&v2).is_err());
}

#[test]
fn serialize_plain_message_keeps_string_content() {
    let m = ChatMessage {
        role: "user".into(),
        content: "你好".into(),
        images: vec![],
        ..Default::default()
    };
    let v = OpenAiCompat::serialize_message(&m);
    assert_eq!(v["content"], serde_json::json!("你好"));
}

#[test]
fn serialize_vision_message_builds_content_array() {
    let m = ChatMessage {
        role: "user".into(),
        content: "看图说话".into(),
        images: vec![
            "data:image/png;base64,AAA".into(),
            "data:image/png;base64,BBB".into(),
        ],
        ..Default::default()
    };
    let v = OpenAiCompat::serialize_message(&m);
    let parts = v["content"].as_array().expect("content should be array");
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0]["type"], "text");
    assert_eq!(parts[0]["text"], "看图说话");
    assert_eq!(parts[1]["type"], "image_url");
    assert_eq!(parts[1]["image_url"]["url"], "data:image/png;base64,AAA");
    assert_eq!(parts[2]["image_url"]["url"], "data:image/png;base64,BBB");
}

// ── v1.9.0 工作块 I:思考流解析 ────────────────────────────────────────────

#[test]
fn parse_reasoning_content_chunk() {
    let data = r#"{"choices":[{"index":0,"delta":{"reasoning_content":"我先想想"},"finish_reason":null}]}"#;
    match OpenAiCompat::parse_chunk(data) {
        Some(StreamChunk::Reasoning { content }) => assert_eq!(content, "我先想想"),
        _ => panic!("expected reasoning"),
    }
}

/// OpenRouter 风格 delta.reasoning 兜底。
#[test]
fn parse_reasoning_fallback_field() {
    let data = r#"{"choices":[{"index":0,"delta":{"reasoning":"think..."},"finish_reason":null}]}"#;
    match OpenAiCompat::parse_chunk(data) {
        Some(StreamChunk::Reasoning { content }) => assert_eq!(content, "think..."),
        _ => panic!("expected reasoning"),
    }
}

/// 思考与正文分帧:reasoning chunk 优先返回,不吞正文。
#[test]
fn parse_content_after_reasoning_not_swallowed() {
    let data = r#"{"choices":[{"index":0,"delta":{"content":"答案"},"finish_reason":null}]}"#;
    match OpenAiCompat::parse_chunk(data) {
        Some(StreamChunk::Delta { content }) => assert_eq!(content, "答案"),
        _ => panic!("expected delta"),
    }
    // 空字符串思考不产生 chunk
    let empty =
        r#"{"choices":[{"index":0,"delta":{"reasoning_content":""},"finish_reason":null}]}"#;
    assert!(OpenAiCompat::parse_chunk(empty).is_none());
}

/// OpenAI 官方:思考仅计数,收尾 chunk usage.completion_tokens_details.reasoning_tokens。
#[test]
fn parse_usage_reasoning_tokens_nested() {
    let data = r#"{"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":50,"total_tokens":60,"completion_tokens_details":{"reasoning_tokens":38}}}"#;
    match OpenAiCompat::parse_chunk(data) {
        Some(StreamChunk::Usage {
            reasoning_tokens, ..
        }) => {
            assert_eq!(reasoning_tokens, Some(38));
        }
        _ => panic!("expected usage"),
    }
}

/// 国产厂商平铺 reasoning_tokens 兜底。
#[test]
fn parse_usage_reasoning_tokens_flat() {
    let data = r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":50,"reasoning_tokens":30}}"#;
    match OpenAiCompat::parse_chunk(data) {
        Some(StreamChunk::Done {
            reasoning_tokens, ..
        }) => {
            assert_eq!(reasoning_tokens, Some(30));
        }
        _ => panic!("expected done"),
    }
}

// ── v1.9.0 工作块 I:五家参数映射(improving.md §1.4) ─────────────────────

fn apply(provider_id: &str, model: &str, spec: ReasoningSpec) -> serde_json::Value {
    let mut body = serde_json::json!({});
    OpenAiCompat::apply_reasoning_params(&mut body, provider_id, model, spec);
    body
}

#[test]
fn reasoning_params_openai_effort_and_off() {
    let b = apply(
        "openai",
        "gpt-5.2",
        ReasoningSpec::Effort {
            level: EffortLevel::Ultra,
        },
    );
    assert_eq!(b["reasoning_effort"], "high"); // Ultra 退化到 high
    // gpt-5 系 Off → minimal 近似关闭
    let b2 = apply("openai", "gpt-5", ReasoningSpec::Off);
    assert_eq!(b2["reasoning_effort"], "minimal");
    // o 系列不认 minimal → 不发
    assert!(
        apply("openai", "o3", ReasoningSpec::Off)
            .get("reasoning_effort")
            .is_none()
    );
}

#[test]
fn reasoning_params_deepseek_effort_ladder() {
    let l = apply(
        "deepseek",
        "deepseek-v3.2",
        ReasoningSpec::Effort {
            level: EffortLevel::Light,
        },
    );
    assert_eq!(l["thinking"]["type"], "enabled");
    assert_eq!(l["reasoning_effort"], "low");
    let m = apply(
        "deepseek",
        "deepseek-v3.2",
        ReasoningSpec::Effort {
            level: EffortLevel::Medium,
        },
    );
    assert_eq!(m["reasoning_effort"], "high"); // Medium 向上退化
    let u = apply(
        "deepseek",
        "deepseek-reasoner",
        ReasoningSpec::Effort {
            level: EffortLevel::Ultra,
        },
    );
    assert_eq!(u["reasoning_effort"], "max");
    let off = apply("deepseek", "deepseek-v3.2", ReasoningSpec::Off);
    assert_eq!(off["thinking"]["type"], "disabled");
}

#[test]
fn reasoning_params_kimi_k3_vs_k26() {
    // K3:不可关,仅顶层 effort,不发 thinking.type
    let k3 = apply(
        "kimi",
        "kimi-k3-latest",
        ReasoningSpec::Effort {
            level: EffortLevel::Medium,
        },
    );
    assert_eq!(k3["reasoning_effort"], "high");
    assert!(k3.get("thinking").is_none());
    assert!(
        apply("kimi", "kimi-k3-latest", ReasoningSpec::Off)
            .get("reasoning_effort")
            .is_none()
    );
    // K2.6:仅开关
    let k26 = apply(
        "kimi",
        "kimi-k2.6",
        ReasoningSpec::Effort {
            level: EffortLevel::Ultra,
        },
    );
    assert_eq!(k26["thinking"]["type"], "enabled");
    assert!(k26.get("reasoning_effort").is_none());
    let k26off = apply("kimi", "kimi-k2.6", ReasoningSpec::Off);
    assert_eq!(k26off["thinking"]["type"], "disabled");
}

#[test]
fn reasoning_params_qwen_budget_ladder() {
    let l = apply(
        "qwen",
        "qwen3-max",
        ReasoningSpec::Effort {
            level: EffortLevel::Light,
        },
    );
    assert_eq!(l["enable_thinking"], true);
    assert_eq!(l["thinking_budget"], 2048);
    let h = apply(
        "qwen",
        "qwen-plus",
        ReasoningSpec::Effort {
            level: EffortLevel::High,
        },
    );
    assert_eq!(h["thinking_budget"], 24576);
    // Ultra 不设上限:不发送 thinking_budget
    let u = apply(
        "qwen",
        "qwen3-max",
        ReasoningSpec::Effort {
            level: EffortLevel::Ultra,
        },
    );
    assert_eq!(u["enable_thinking"], true);
    assert!(u.get("thinking_budget").is_none());
    let off = apply("qwen", "qwen3-max", ReasoningSpec::Off);
    assert_eq!(off["enable_thinking"], false);
}

#[test]
fn reasoning_params_zhipu_glm53_locked_on() {
    // GLM-5.3:不可关,Off 防御性不发任何参数
    assert!(
        apply("zhipu", "glm-5.3", ReasoningSpec::Off)
            .get("thinking")
            .is_none()
    );
    let u = apply(
        "zhipu",
        "glm-5.3",
        ReasoningSpec::Effort {
            level: EffortLevel::Ultra,
        },
    );
    assert_eq!(u["thinking"]["type"], "enabled");
    assert_eq!(u["reasoning_effort"], "max");
    // GLM-5.2:全档
    let m = apply(
        "zhipu",
        "glm-5.2",
        ReasoningSpec::Effort {
            level: EffortLevel::Medium,
        },
    );
    assert_eq!(m["reasoning_effort"], "medium");
    // GLM-4.6:仅开关无档
    let g46 = apply(
        "zhipu",
        "glm-4.6",
        ReasoningSpec::Effort {
            level: EffortLevel::Ultra,
        },
    );
    assert_eq!(g46["thinking"]["type"], "enabled");
    assert!(g46.get("reasoning_effort").is_none());
    assert_eq!(
        apply("zhipu", "glm-4.6", ReasoningSpec::Off)["thinking"]["type"],
        "disabled"
    );
}

#[test]
fn reasoning_params_custom_only_standard_field() {
    // 自定义模板:只发 reasoning_effort,不发扩展字段(中转站兼容)
    let b = apply(
        "my-relay",
        "some-model",
        ReasoningSpec::Effort {
            level: EffortLevel::High,
        },
    );
    assert_eq!(b["reasoning_effort"], "high");
    assert!(b.get("thinking").is_none());
    assert!(b.get("enable_thinking").is_none());
    assert!(
        apply("my-relay", "some-model", ReasoningSpec::Off)
            .get("reasoning_effort")
            .is_none()
    );
}

/// 回归:多轮工具调用依赖 assistant.tool_calls 与 tool.tool_call_id 正确序列化,
/// 缺任一字段 OpenAI 兼容端点会直接 400。
#[test]
fn serialize_assistant_tool_calls_and_tool_result_messages() {
    use crate::provider::{ToolCall, ToolCallFunction};

    let assistant = ChatMessage {
        role: "assistant".into(),
        content: String::new(),
        images: vec![],
        tool_calls: Some(vec![ToolCall {
            id: "call_1".into(),
            r#type: "function".into(),
            function: ToolCallFunction {
                name: "echo".into(),
                arguments: r#"{"text":"hi"}"#.into(),
            },
        }]),
        ..Default::default()
    };
    let v = OpenAiCompat::serialize_message(&assistant);
    let calls = v["tool_calls"]
        .as_array()
        .expect("必须输出 tool_calls 数组");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["id"], "call_1");
    assert_eq!(calls[0]["type"], "function");
    assert_eq!(calls[0]["function"]["name"], "echo");
    assert!(
        v.get("tool_call_id").is_none(),
        "assistant 消息不应带 tool_call_id"
    );

    let tool_msg = ChatMessage {
        role: "tool".into(),
        content: "echo:hi".into(),
        images: vec![],
        tool_call_id: Some("call_1".into()),
        ..Default::default()
    };
    let v = OpenAiCompat::serialize_message(&tool_msg);
    assert_eq!(v["role"], "tool");
    assert_eq!(v["tool_call_id"], "call_1");
    assert_eq!(v["content"], "echo:hi");
    assert!(v.get("tool_calls").is_none(), "tool 消息不应带 tool_calls");
}

/// 回归:普通消息不得携带任何工具字段(避免污染旧端点请求)。
#[test]
fn serialize_plain_message_has_no_tool_fields() {
    let m = ChatMessage {
        role: "user".into(),
        content: "hi".into(),
        images: vec![],
        ..Default::default()
    };
    let v = OpenAiCompat::serialize_message(&m);
    assert!(v.get("tool_calls").is_none());
    assert!(v.get("tool_call_id").is_none());
}
