//! 模型抽象层(Provider trait)。
//!
//! 对应 plan/01-phase1-mvp.md §3.3:
//!   - Provider trait:chat_stream 流式;generate_image / translate 预留
//!   - OpenAI 兼容 Provider(base_url + api_key + model)
//!   - API Key 本地加密存储
//!   - 内置 Provider 模板
//!
//! 设计要点:
//!   - 一份 Provider 协议,多模型可插拔
//!   - 流式输出通过 Channel 推到前端(tauri::ipc::Channel)
//!   - 图像/翻译接口预留默认实现,未接入的 Provider 返回 NotSupported

pub mod openai;
pub mod templates;
pub mod keychain;
pub mod custom;
pub mod model_cards;
pub mod model_config;
pub mod reasoning;
pub mod rotation;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use futures::Stream;

/// 对话消息(与 OpenAI chat.completions 协议对齐)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,      // system / user / assistant
    pub content: String,
    /// 图片附件(base64 data URL 列表;迭代 D1)。
    /// 非空时序列化为 OpenAI vision content 数组;空数组完全不参与序列化,向后兼容。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
}

/// 统一思考强度档位(与前端四档 Light/Medium/High/Ultra 一一对应)。
/// 适配层按 Provider 方言翻译成原生参数(见 openai.rs::apply_reasoning_params),
/// 有档位差异的模型(如 DeepSeek/K3/GLM-5.3 仅 low/high/max)由适配层向上退化,
/// 前端通过能力接口拿到「实际发送值」做透明化展示。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffortLevel {
    Light,
    Medium,
    High,
    Ultra,
}

/// 统一思考规格(improving.md §1.4):适配层负责翻译,不把厂商差异泄漏给上层。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningSpec {
    /// 关闭。仅「可关」模型有意义:适配层决定发 disabled 还是干脆省略;
    /// 不可关模型(如 GLM-5.3)传 disabled 会 400,适配层对这类模型省略参数。
    Off,
    /// 档位式(OpenAI / DeepSeek / Kimi K3 / GLM 等 reasoning_effort 生态)。
    Effort { level: EffortLevel },
    /// 预算式(通义 thinking_budget 生态;统一档位经映射得到 token 上限,Ultra 不设上限)。
    Budget { tokens: u32 },
}

impl ReasoningSpec {
    /// 是否处于「思考生效」状态(Effort/Budget)。用于采样参数互斥判断。
    pub fn is_active(&self) -> bool {
        !matches!(self, ReasoningSpec::Off)
    }
}

/// 流式请求参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// 思考规格;None = 不涉及(不支持思考的调用路径/模型)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningSpec>,
    /// 是否流式;false 时退化为一次性返回(包成单元素流)
    #[serde(default = "default_true")]
    pub stream: bool,
}

fn default_true() -> bool {
    true
}

/// 流式输出的最小单元。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamChunk {
    /// 增量文本
    Delta { content: String },
    /// 思考增量(delta.reasoning_content;与正文分流,前端折叠块单独累积展示)。
    /// 老中转站/不支持厂商不产生该 chunk,前端对未知类型亦有忽略分支,天然向前兼容。
    Reasoning { content: String },
    /// Token 用量(stream_options.include_usage 的最后一个 data chunk)。
    /// 仅后端使用:命令层据此落 usage_record,**不转发给前端**。
    Usage {
        input_tokens: Option<u32>,
        output_tokens: Option<u32>,
        total_tokens: Option<u32>,
        /// 思考 token(usage.completion_tokens_details.reasoning_tokens;缺失为 None)
        reasoning_tokens: Option<u32>,
        /// 缓存命中输入 token(prompt_tokens_details.cached_tokens 等;缺失为 None = 未知)
        cached_tokens: Option<u32>,
    },
    /// 完成(可选 finish_reason / usage)
    Done {
        finish_reason: Option<String>,
        prompt_tokens: Option<u32>,
        completion_tokens: Option<u32>,
        /// 思考 token(仅计数无明文思考的端点如 OpenAI 官方,靠它展示「思考消耗 N tokens」)
        reasoning_tokens: Option<u32>,
        /// 缓存命中输入 token(缺失为 None = 厂商未返回)
        cached_tokens: Option<u32>,
    },
    /// 错误
    Error { message: String },
}

/// 流式响应:Pin<Box<dyn Stream>>。Provider 内部可自由组合 reqwest SSE / 测试 mock。
pub type ChatStream = Pin<Box<dyn Stream<Item = StreamChunk> + Send>>;

/// 图像生成请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRequest {
    /// 图像模型名(如 dall-e-3);由命令层指定,Provider 不再持有“当前模型”状态
    pub model: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,        // "1024x1024"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,       // vivid / natural
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageResponse {
    /// 图像可访问 URL 或 base64 data URI
    pub url: String,
    pub revised_prompt: Option<String>,
}

/// 构造翻译用 system prompt:约束模型只输出译文、保留原文格式。命令层流式翻译使用。
pub fn translate_system_prompt(source_lang: Option<&str>, target_lang: &str) -> String {
    let src = source_lang
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("auto-detected language");
    format!(
        "You are a professional translation engine. Translate the user's text from {src} into {target_lang}. \
         Output ONLY the translated text. Preserve the original formatting (line breaks, lists, markdown). \
         Do not add explanations, notes, or extra quotation marks."
    )
}

/// Provider 错误分类,便于前端区分处理。
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("网络错误: {0}")]
    Network(String),
    #[error("鉴权失败: {0}")]
    Auth(String),
    /// 限流(429)。`retry_after` 为响应头 Retry-After 解析出的秒数(未提供为 None)。
    #[error("限流: {message}")]
    RateLimited {
        message: String,
        retry_after: Option<u64>,
    },
    #[error("服务端错误: {0}")]
    Server(String),
    #[error("请求参数错误: {0}")]
    BadRequest(String),
    #[error("该 Provider 不支持此能力: {0}")]
    NotSupported(String),
    #[error("协议解析失败: {0}")]
    Parse(String),
    #[error("其他: {0}")]
    Other(String),
}

/// Provider 统一抽象。
#[async_trait]
pub trait Provider: Send + Sync {
    /// 展示名
    fn display_name(&self) -> &str;

    /// 流式对话。所有 Provider 必须实现。
    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError>;

    /// 图像生成(预留)。默认 NotSupported。
    async fn generate_image(&self, _req: ImageRequest) -> Result<ImageResponse, ProviderError> {
        Err(ProviderError::NotSupported(format!(
            "{} 不支持图像生成",
            self.display_name()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::translate_system_prompt;

    #[test]
    fn translate_prompt_with_source() {
        let p = translate_system_prompt(Some("Chinese"), "English");
        assert!(p.contains("from Chinese into English"));
        assert!(p.contains("ONLY the translated text"));
    }

    #[test]
    fn translate_prompt_auto_detect() {
        let p = translate_system_prompt(None, "Japanese");
        assert!(p.contains("auto-detected language"));
        assert!(p.contains("into Japanese"));
        // 空白源语言等同自动检测
        let p2 = translate_system_prompt(Some("  "), "French");
        assert!(p2.contains("auto-detected language"));
    }
}
