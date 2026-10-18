//! OpenAI 兼容 Provider 实现。
//!
//! 适用于:OpenAI / DeepSeek / Kimi / 通义(兼容模式)/ 智谱 / Moonshot / vLLM / Ollama
//! 等任何遵守 `POST /chat/completions` + SSE 协议的服务。

use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::{Client, StatusCode};
use serde_json::json;

use super::{
    ChatMessage, ChatRequest, ChatStream, ImageRequest, ImageResponse, Provider, ProviderError,
    StreamChunk,
};

/// OpenAI 兼容 Provider。
pub struct OpenAiCompat {
    pub id: String,
    pub display_name: String,
    pub base_url: String,        // 例如 https://api.openai.com/v1
    pub api_key: String,
    /// 是否发送 `stream_options.include_usage`(用于采集精确 Token 用量)。
    /// 默认 **关闭**;仅对话路径按配置开启(部分中转站不认该字段会返回 400)。
    stream_options: bool,
    client: Client,
}

impl OpenAiCompat {
    pub fn new(
        id: impl Into<String>,
        display_name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            id: id.into(),
            display_name: display_name.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            stream_options: false,
            client,
        }
    }

    /// 关闭 / 开启 `stream_options`(兼容回退开关)。
    pub fn with_stream_options(mut self, on: bool) -> Self {
        self.stream_options = on;
        self
    }

    /// 序列化单条 ChatMessage:纯文本或 vision content 数组(兼容 OpenAI / DeepSeek 等)。
    fn serialize_message(m: &ChatMessage) -> serde_json::Value {
        if m.images.is_empty() {
            return json!({
                "role": m.role,
                "content": m.content,
            });
        }
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
    }

    /// 把 OpenAI 单次响应(JSON)解析成 StreamChunk。
    fn parse_chunk(data: &str) -> Option<StreamChunk> {
        if data == "[DONE]" {
            return Some(StreamChunk::Done {
                finish_reason: Some("stop".into()),
                prompt_tokens: None,
                completion_tokens: None,
                reasoning_tokens: None,
            });
        }
        let v: serde_json::Value = serde_json::from_str(data).ok()?;

        let usage_of = |u: &serde_json::Value| {
            (
                u.get("prompt_tokens").and_then(|t| t.as_u64()).map(|t| t as u32),
                u.get("completion_tokens").and_then(|t| t.as_u64()).map(|t| t as u32),
                u.get("total_tokens").and_then(|t| t.as_u64()).map(|t| t as u32),
                // 思考 token:OpenAI 官方放在 completion_tokens_details.reasoning_tokens;
                // 部分国产厂商直接平铺 reasoning_tokens,两处都尝试
                u.get("completion_tokens_details")
                    .and_then(|d| d.get("reasoning_tokens"))
                    .and_then(|t| t.as_u64())
                    .or_else(|| u.get("reasoning_tokens").and_then(|t| t.as_u64()))
                    .map(|t| t as u32),
            )
        };

        if let Some(choice) = v.get("choices").and_then(|c| c.get(0)) {
            // delta.reasoning_content(思考流):DeepSeek 首创、Kimi/GLM/百炼/中转生态事实统一,
            // 保证先于正文;OpenRouter 风格的 delta.reasoning 兜底兼容。
            // 字段缺失时静默降级为无思考(老中转站零影响)。
            if let Some(rc) = choice
                .get("delta")
                .and_then(|d| d.get("reasoning_content").or_else(|| d.get("reasoning")))
                .and_then(|c| c.as_str())
            {
                if !rc.is_empty() {
                    return Some(StreamChunk::Reasoning {
                        content: rc.to_string(),
                    });
                }
            }
            // delta.content
            if let Some(content) = choice
                .get("delta")
                .and_then(|d| d.get("content"))
                .and_then(|c| c.as_str())
            {
                if !content.is_empty() {
                    return Some(StreamChunk::Delta {
                        content: content.to_string(),
                    });
                }
            }
            // finish_reason
            if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                let usage = v.get("usage");
                return Some(StreamChunk::Done {
                    finish_reason: Some(fr.to_string()),
                    prompt_tokens: usage
                        .and_then(|u| u.get("prompt_tokens"))
                        .and_then(|t| t.as_u64())
                        .map(|t| t as u32),
                    completion_tokens: usage
                        .and_then(|u| u.get("completion_tokens"))
                        .and_then(|t| t.as_u64())
                        .map(|t| t as u32),
                    reasoning_tokens: usage.and_then(|u| usage_of(u).3),
                });
            }
        }
        // `stream_options.include_usage` 的收尾 chunk:choices 为空,仅带 usage。
        // 该 chunk 仅用于后端采集用量,不转发前端(命令层会拦截)。
        if v.get("choices")
            .and_then(|c| c.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(false)
        {
            if let Some(usage) = v.get("usage").filter(|u| !u.is_null()) {
                let (input_tokens, output_tokens, total_tokens, reasoning_tokens) = usage_of(usage);
                if input_tokens.is_some()
                    || output_tokens.is_some()
                    || total_tokens.is_some()
                    || reasoning_tokens.is_some()
                {
                    return Some(StreamChunk::Usage {
                        input_tokens,
                        output_tokens,
                        total_tokens,
                        reasoning_tokens,
                    });
                }
            }
        }
        None
    }

    /// 按Provider 方言把统一思考规格翻译成请求参数(improving.md §1.4 映射表)。
    ///
    /// 约束:
    ///   - 只发官方合法值,不杜撰字段;档位缺失时向上退化(如仅 low/high/max 三档的模型
    ///     Medium→high、Ultra→max),退化差异由前端 tooltip 透明展示;
    ///   - Off 对「传 disabled 会报错」的不可关模型(GLM-5.3 / Kimi K3)干脆不发参数(防御);
    ///   - 自定义 / 未知模板只发 OpenAI 标准 `reasoning_effort`,不发扩展字段,最大化中转站兼容。
    fn apply_reasoning_params(
        body: &mut serde_json::Value,
        provider_id: &str,
        model: &str,
        spec: super::ReasoningSpec,
    ) {
        use super::{EffortLevel, ReasoningSpec};
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
                ReasoningSpec::Effort { level } => body["reasoning_effort"] = json!(match level {
                    EffortLevel::Light => "low",
                    EffortLevel::Medium => "medium",
                    EffortLevel::High | EffortLevel::Ultra => "high",
                }),
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

    /// 把 HTTP 错误状态映射到 ProviderError。
    pub(crate) async fn map_http_error(resp: reqwest::Response) -> ProviderError {
        let status = resp.status();
        // Retry-After(秒):部分服务商以该头给出建议等待时长;HTTP-date 形式不支持(视为 None)。
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok());
        let body = resp.text().await.unwrap_or_default();
        // OpenAI 风格错误:{ "error": { "message": "...", "type": "..." } }
        let msg = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| body.chars().take(200).collect());

        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ProviderError::Auth(msg),
            StatusCode::TOO_MANY_REQUESTS => ProviderError::RateLimited {
                message: msg,
                retry_after,
            },
            StatusCode::BAD_REQUEST => ProviderError::BadRequest(msg),
            s if s.is_server_error() => ProviderError::Server(msg),
            _ => ProviderError::Other(format!("HTTP {}: {}", status.as_u16(), msg)),
        }
    }

    /// 解析 images/generations 响应 JSON,兼容 b64_json 与 url 两种返回。
    fn parse_image_response(v: &serde_json::Value) -> Result<ImageResponse, ProviderError> {
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

#[async_trait]
impl Provider for OpenAiCompat {
    fn display_name(&self) -> &str {
        &self.display_name
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
        let url = format!("{}/chat/completions", self.base_url);

        // 序列化消息:无图片时 content 为纯字符串;有图片时走 vision content 数组。
        let messages: Vec<serde_json::Value> = req
            .messages
            .iter()
            .map(|m: &ChatMessage| Self::serialize_message(m))
            .collect();

        // 采样参数互斥(官方明示:思考开启时 temperature/top_p 被服务端忽略或直接报错):
        // 思考生效(Effort/Budget)时整个省略这两个键(不发 null,兼容严格校验的端点);
        // max_tokens 保留——思考 token 计入其额度,思考模型建议 ≥16k(UI 侧提示)。
        let reasoning_active = req.reasoning.map(|r| r.is_active()).unwrap_or(false);
        let mut body = json!({
            "model": req.model,
            "messages": messages,
            "stream": req.stream,
            "max_tokens": req.max_tokens,
        });
        if !reasoning_active {
            body["temperature"] = json!(req.temperature);
            body["top_p"] = json!(req.top_p);
        }
        // 思考参数:按 provider_id 方言翻译(五家映射 + 自定义保守透传,见 apply_reasoning_params)
        if let Some(spec) = req.reasoning {
            Self::apply_reasoning_params(&mut body, &self.id, &req.model, spec);
        }
        // 请求精确 Token 用量(部分中转站不认该字段 → 由调用方以 with_stream_options(false) 重试)
        if req.stream && self.stream_options {
            body["stream_options"] = json!({ "include_usage": true });
        }

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("{e}")))?;

        if !resp.status().is_success() {
            return Err(Self::map_http_error(resp).await);
        }

        // SSE 解析:按行读取,提取 data: 行,喂给 parse_chunk。
        // 用字节缓冲拼行:多字节 UTF-8 字符(如 4 字节 emoji)可能被 HTTP chunk 切断,
        // 直接对单个 chunk from_utf8_lossy 会产生 U+FFFD 乱码;先在字节层找 '\n'
        // (0x0A 不可能出现在多字节序列内部),只对完整行解码即可避免。
        let byte_stream = resp.bytes_stream();

        // SSE 单行字节上限:16MB。正常 data: 行(一个 JSON chunk)不足 1MB,
        // 超限说明对端未按 SSE 帧分隔或在恶意灌数据,及时报错终止防内存耗尽(M-9)。
        const MAX_SSE_LINE_BYTES: usize = 16 * 1024 * 1024;

        let s = futures::stream::unfold(
            (byte_stream, Vec::<u8>::new(), false),
            |(mut bs, mut buf, aborted)| async move {
                // N-4:已产出过超限错误则终止流,防止恶意端反复灌数据制造错误风暴
                if aborted {
                    return None;
                }
                loop {
                    // 先消费 buf 里的完整行
                    if let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                        let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
                        let line = String::from_utf8_lossy(&line_bytes);
                        let line = line.trim_end_matches('\r');
                        if line.is_empty() {
                            continue;
                        }
                        if let Some(data) = line.strip_prefix("data:") {
                            let data = data.trim();
                            if let Some(chunk) = Self::parse_chunk(data) {
                                return Some((chunk, (bs, buf, false)));
                            }
                            continue;
                        }
                        // 其他行(event: / id: / retry:)忽略
                        continue;
                    }
                    // buf 没完整行,从 stream 读
                    match bs.next().await {
                        Some(Ok(bytes)) => {
                            buf.extend_from_slice(&bytes);
                            if buf.len() > MAX_SSE_LINE_BYTES {
                                // 报错一次后终止(aborted=true):错误已上抛,不再消耗对端数据
                                buf.clear();
                                return Some((
                                    StreamChunk::Error {
                                        message: "stream 单行超过 16MB 上限,已中止".into(),
                                    },
                                    (bs, buf, true),
                                ));
                            }
                        }
                        Some(Err(e)) => {
                            return Some((
                                StreamChunk::Error {
                                    message: format!("stream read error: {e}"),
                                },
                                (bs, buf, false),
                            ));
                        }
                        None => return None,
                    }
                }
            },
        );

        Ok(Box::pin(s))
    }

    /// 图像生成:POST {base_url}/images/generations。
    /// 响应兼容两种形态:data[0].b64_json(包成 data URI)或 data[0].url。
    async fn generate_image(&self, req: ImageRequest) -> Result<ImageResponse, ProviderError> {
        let url = format!("{}/images/generations", self.base_url);

        let mut body = json!({
            "model": req.model,
            "prompt": req.prompt,
            "n": req.n.unwrap_or(1),
        });
        if let Some(size) = &req.size {
            body["size"] = json!(size);
        }
        if let Some(style) = &req.style {
            body["style"] = json!(style);
        }

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("{e}")))?;

        if !resp.status().is_success() {
            return Err(Self::map_http_error(resp).await);
        }

        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::Parse(format!("{e}")))?;
        Self::parse_image_response(&v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_delta() {
        let data = r#"{"id":"x","choices":[{"index":0,"delta":{"content":"你好"},"finish_reason":null}]}"#;
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
        let data = r#"{"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":34,"total_tokens":46}}"#;
        match OpenAiCompat::parse_chunk(data) {
            Some(StreamChunk::Usage { input_tokens, output_tokens, total_tokens, .. }) => {
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
        };
        let v = OpenAiCompat::serialize_message(&m);
        assert_eq!(v["content"], serde_json::json!("你好"));
    }

    #[test]
    fn serialize_vision_message_builds_content_array() {
        let m = ChatMessage {
            role: "user".into(),
            content: "看图说话".into(),
            images: vec!["data:image/png;base64,AAA".into(), "data:image/png;base64,BBB".into()],
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
        let empty = r#"{"choices":[{"index":0,"delta":{"reasoning_content":""},"finish_reason":null}]}"#;
        assert!(OpenAiCompat::parse_chunk(empty).is_none());
    }

    /// OpenAI 官方:思考仅计数,收尾 chunk usage.completion_tokens_details.reasoning_tokens。
    #[test]
    fn parse_usage_reasoning_tokens_nested() {
        let data = r#"{"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":50,"total_tokens":60,"completion_tokens_details":{"reasoning_tokens":38}}}"#;
        match OpenAiCompat::parse_chunk(data) {
            Some(StreamChunk::Usage { reasoning_tokens, .. }) => {
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
            Some(StreamChunk::Done { reasoning_tokens, .. }) => {
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

    use super::super::{EffortLevel, ReasoningSpec};

    #[test]
    fn reasoning_params_openai_effort_and_off() {
        let b = apply("openai", "gpt-5.2", ReasoningSpec::Effort { level: EffortLevel::Ultra });
        assert_eq!(b["reasoning_effort"], "high"); // Ultra 退化到 high
        // gpt-5 系 Off → minimal 近似关闭
        let b2 = apply("openai", "gpt-5", ReasoningSpec::Off);
        assert_eq!(b2["reasoning_effort"], "minimal");
        // o 系列不认 minimal → 不发
        assert!(apply("openai", "o3", ReasoningSpec::Off).get("reasoning_effort").is_none());
    }

    #[test]
    fn reasoning_params_deepseek_effort_ladder() {
        let l = apply("deepseek", "deepseek-v3.2", ReasoningSpec::Effort { level: EffortLevel::Light });
        assert_eq!(l["thinking"]["type"], "enabled");
        assert_eq!(l["reasoning_effort"], "low");
        let m = apply("deepseek", "deepseek-v3.2", ReasoningSpec::Effort { level: EffortLevel::Medium });
        assert_eq!(m["reasoning_effort"], "high"); // Medium 向上退化
        let u = apply("deepseek", "deepseek-reasoner", ReasoningSpec::Effort { level: EffortLevel::Ultra });
        assert_eq!(u["reasoning_effort"], "max");
        let off = apply("deepseek", "deepseek-v3.2", ReasoningSpec::Off);
        assert_eq!(off["thinking"]["type"], "disabled");
    }

    #[test]
    fn reasoning_params_kimi_k3_vs_k26() {
        // K3:不可关,仅顶层 effort,不发 thinking.type
        let k3 = apply("kimi", "kimi-k3-latest", ReasoningSpec::Effort { level: EffortLevel::Medium });
        assert_eq!(k3["reasoning_effort"], "high");
        assert!(k3.get("thinking").is_none());
        assert!(apply("kimi", "kimi-k3-latest", ReasoningSpec::Off).get("reasoning_effort").is_none());
        // K2.6:仅开关
        let k26 = apply("kimi", "kimi-k2.6", ReasoningSpec::Effort { level: EffortLevel::Ultra });
        assert_eq!(k26["thinking"]["type"], "enabled");
        assert!(k26.get("reasoning_effort").is_none());
        let k26off = apply("kimi", "kimi-k2.6", ReasoningSpec::Off);
        assert_eq!(k26off["thinking"]["type"], "disabled");
    }

    #[test]
    fn reasoning_params_qwen_budget_ladder() {
        let l = apply("qwen", "qwen3-max", ReasoningSpec::Effort { level: EffortLevel::Light });
        assert_eq!(l["enable_thinking"], true);
        assert_eq!(l["thinking_budget"], 2048);
        let h = apply("qwen", "qwen-plus", ReasoningSpec::Effort { level: EffortLevel::High });
        assert_eq!(h["thinking_budget"], 24576);
        // Ultra 不设上限:不发送 thinking_budget
        let u = apply("qwen", "qwen3-max", ReasoningSpec::Effort { level: EffortLevel::Ultra });
        assert_eq!(u["enable_thinking"], true);
        assert!(u.get("thinking_budget").is_none());
        let off = apply("qwen", "qwen3-max", ReasoningSpec::Off);
        assert_eq!(off["enable_thinking"], false);
    }

    #[test]
    fn reasoning_params_zhipu_glm53_locked_on() {
        // GLM-5.3:不可关,Off 防御性不发任何参数
        assert!(apply("zhipu", "glm-5.3", ReasoningSpec::Off).get("thinking").is_none());
        let u = apply("zhipu", "glm-5.3", ReasoningSpec::Effort { level: EffortLevel::Ultra });
        assert_eq!(u["thinking"]["type"], "enabled");
        assert_eq!(u["reasoning_effort"], "max");
        // GLM-5.2:全档
        let m = apply("zhipu", "glm-5.2", ReasoningSpec::Effort { level: EffortLevel::Medium });
        assert_eq!(m["reasoning_effort"], "medium");
        // GLM-4.6:仅开关无档
        let g46 = apply("zhipu", "glm-4.6", ReasoningSpec::Effort { level: EffortLevel::Ultra });
        assert_eq!(g46["thinking"]["type"], "enabled");
        assert!(g46.get("reasoning_effort").is_none());
        assert_eq!(apply("zhipu", "glm-4.6", ReasoningSpec::Off)["thinking"]["type"], "disabled");
    }

    #[test]
    fn reasoning_params_custom_only_standard_field() {
        // 自定义模板:只发 reasoning_effort,不发扩展字段(中转站兼容)
        let b = apply("my-relay", "some-model", ReasoningSpec::Effort { level: EffortLevel::High });
        assert_eq!(b["reasoning_effort"], "high");
        assert!(b.get("thinking").is_none());
        assert!(b.get("enable_thinking").is_none());
        assert!(apply("my-relay", "some-model", ReasoningSpec::Off).get("reasoning_effort").is_none());
    }
}
