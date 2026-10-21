//! OpenAI 兼容 Provider 实现。
//!
//! 适用于:OpenAI / DeepSeek / Kimi / 通义(兼容模式)/ 智谱 / Moonshot / vLLM / Ollama
//! 等任何遵守 `POST /chat/completions` + SSE 协议的服务。
//!
//! 拆分:`chunk` 负责 SSE 解析与流组装,`body` 负责消息/思考参数序列化与响应解析,
//! 本文件保留类型定义、HTTP 错误映射与 Provider 协议编排。

mod body;
mod chunk;
#[cfg(test)]
mod tests;

use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde_json::json;

use crate::provider::{
    ChatMessage, ChatRequest, ChatStream, ImageRequest, ImageResponse, Provider, ProviderError,
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

        // SSE 解析:按行读取,提取 data: 行,喂给 parse_chunk(实现见 chunk 模块)。
        Ok(chunk::sse_stream(resp))
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
