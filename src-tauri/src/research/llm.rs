//! 研究链路的 LLM 调用与 token 估算。

use std::sync::Arc;

use crate::provider::{ChatMessage, ChatRequest, ChatStream, Provider, StreamChunk};

use super::EngineParams;
use super::pipeline::{RunError, check_cancel};

/// 粗略 token 估算:中英文混合按 3 字符 ≈ 1 tok。
pub(crate) fn est_tok(s: &str) -> u64 {
    (s.chars().count() / 3) as u64
}

/// H3:发起一次流式对话请求(研究链路统一入口)。
/// stream_options 被中转站拒绝(400 含 stream_option)时,自动用无该字段重建的
/// Provider 重试一次并记忆回退实例,后续调用直接走回退,与对话链路回退语义一致。
pub(crate) async fn chat_stream_compat(
    p: &EngineParams,
    req: ChatRequest,
) -> Result<ChatStream, RunError> {
    // 已发生过回退:直接复用回退实例(不持锁跨 await,先取出 Arc)
    let fb = p.fallback.lock().ok().and_then(|g| g.clone());
    if let Some(prov) = fb {
        return prov
            .chat_stream(req)
            .await
            .map_err(|e| RunError::Fatal(e.to_string()));
    }
    match p.provider.chat_stream(req.clone()).await {
        Ok(s) => Ok(s),
        Err(e) => {
            if !crate::commands::provider::is_stream_options_rejected(&e) {
                return Err(RunError::Fatal(e.to_string()));
            }
            let Some(rb) = &p.rebuild else {
                return Err(RunError::Fatal(e.to_string()));
            };
            // 中转站不认 stream_options:关闭该字段重建 Provider,重试一次
            let prov = crate::commands::provider::build_provider_with_stream_options(
                &rb.provider_id,
                &rb.model,
                rb.custom_base_url.as_deref(),
                rb.custom_display_name.as_deref(),
                false,
            )
            .map_err(RunError::Fatal)?;
            let prov: Arc<dyn Provider> = Arc::from(prov);
            if let Ok(mut g) = p.fallback.lock() {
                *g = Some(prov.clone());
            }
            prov.chat_stream(req)
                .await
                .map_err(|e| RunError::Fatal(e.to_string()))
        }
    }
}

/// 流式收全量文本(逐 chunk 检查取消)。
/// 返回 (正文, 真实 total_tokens):H3 优先取 stream_options 收尾 chunk 的 usage,
/// 其次 Done 的 prompt+completion;均不可用为 None(调用方回退字符估算)。
pub(crate) async fn llm_collect(
    p: &EngineParams,
    messages: Vec<ChatMessage>,
) -> Result<(String, Option<u64>), RunError> {
    use futures::StreamExt;

    let req = ChatRequest {
        model: p.model.clone(),
        messages,
        temperature: Some(0.4),
        max_tokens: None,
        top_p: None,
        reasoning: None, // 研究链路档位透传为 Phase 2,暂不启用
        stream: true,
        ..Default::default()
    };
    let mut stream = chat_stream_compat(p, req).await?;
    let mut out = String::new();
    let mut usage: Option<u64> = None;
    while let Some(chunk) = stream.next().await {
        check_cancel(p)?;
        match chunk {
            StreamChunk::Delta { content } => out.push_str(&content),
            StreamChunk::Reasoning { .. } => {} // 研究链路只取正文,忽略思考流
            StreamChunk::Error { message } => return Err(RunError::Fatal(message)),
            StreamChunk::Usage {
                total_tokens: Some(t),
                ..
            } => {
                usage = Some(t as u64);
            }
            StreamChunk::Usage { .. } => {}
            StreamChunk::Done {
                prompt_tokens,
                completion_tokens,
                ..
            } => {
                if usage.is_none() {
                    usage = match (prompt_tokens, completion_tokens) {
                        (Some(a), Some(b)) => Some((a + b) as u64),
                        (Some(a), None) => Some(a as u64),
                        (None, Some(b)) => Some(b as u64),
                        (None, None) => None,
                    };
                }
                break;
            }
            _ => {}
        }
    }
    Ok((out, usage))
}
