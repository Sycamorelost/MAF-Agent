//! 流式对话:chat_stream 命令与带故障转移的后台流任务(模块门面)。
//!
//! 职责拆分:
//! - `command.rs`:`chat_stream` Tauri 命令入口(组装请求 + 启动后台任务);
//! - `reasoning.rs`:思考档位解析与合并;
//! - `failover.rs`:多 Key 轮换/故障转移的流式主循环;
//! - `context.rs`:上下文预算裁剪;

// 命令子模块需对外可见:命令注册表使用子模块原始路径(Tauri 宏要求真实模块路径)。
pub(crate) mod command;
mod context;
mod failover;
mod reasoning;

#[cfg(test)]
mod tests;

use serde::Deserialize;

use crate::provider::ChatMessage;

#[derive(Debug, Deserialize)]
pub struct ChatStreamInput {
    pub provider_id: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    /// 思考档位(v1.9.0 工作块 I):off / light / medium / high / ultra。
    /// None = 前端能力判定不支持,不发送任何思考参数(后端兜底合并保持同口径)。
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// 自定义 Provider 时必传(base_url)
    pub custom_base_url: Option<String>,
    /// 自定义 Provider 时必传(模型列表只用于 UI 展示,这里仅校验非空)
    pub custom_display_name: Option<String>,
    /// 会话 id(用于按助手归因;可选,缺失时记为未绑定助手)
    pub session_id: Option<String>,
    /// 前端生成的取消句柄;「停止生成」时凭它调 cancel_stream(M-2)
    pub cancel_id: String,
    /// Phase 5e:启用 MCP 时前端下发的工具定义(OpenAI function-calling 格式)。
    /// 为空 = 不启用工具调用,行为与既有版本完全一致。
    #[serde(default)]
    pub tools: Option<Vec<crate::provider::ToolDef>>,
}

#[allow(unused_imports)]
pub use command::chat_stream;
