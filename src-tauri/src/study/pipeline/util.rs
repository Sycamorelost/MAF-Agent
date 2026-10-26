//! 学习流水线的 LLM 调用辅助:流式收口与消息构造。

use std::sync::Arc;

use futures::StreamExt;

use crate::provider::{ChatMessage, ChatRequest, Provider, StreamChunk};

use super::{RunError, StudyParams, check_cancel};

/// 流式收全量文本(逐 chunk 检查取消;暂停在 chunk 间不挂起,步骤间统一挂起)。
pub(crate) async fn llm_collect(
    p: &StudyParams,
    messages: Vec<ChatMessage>,
) -> Result<String, RunError> {
    let req = ChatRequest {
        model: p.model.clone(),
        messages,
        temperature: Some(0.4),
        max_tokens: None,
        top_p: None,
        reasoning: None,
        stream: true,
        ..Default::default()
    };
    // H3 同款:已回退过则直接复用回退实例;stream_options 被拒时回退重建一次
    let cached = p.fallback.lock().ok().and_then(|g| g.clone());
    let mut stream = if let Some(prov) = cached {
        prov.chat_stream(req)
            .await
            .map_err(|e| RunError::Fatal(e.to_string()))?
    } else {
        match p.provider.chat_stream(req.clone()).await {
            Ok(s) => s,
            Err(e) => {
                let rejected = crate::commands::provider::is_stream_options_rejected(&e);
                let has_rb = p.rebuild.is_some();
                if !rejected || !has_rb {
                    return Err(RunError::Fatal(e.to_string()));
                }
                let rb = p.rebuild.as_ref().unwrap();
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
                    .map_err(|e| RunError::Fatal(e.to_string()))?
            }
        }
    };

    let mut out = String::new();
    while let Some(chunk) = stream.next().await {
        check_cancel(p)?;
        match chunk {
            StreamChunk::Delta { content } => out.push_str(&content),
            StreamChunk::Reasoning { .. } => {}
            StreamChunk::Error { message } => return Err(RunError::Fatal(message)),
            StreamChunk::Usage { .. } => {}
            StreamChunk::Done { .. } => break,
            _ => {}
        }
    }
    Ok(out)
}

pub(crate) fn sys_msg(content: String) -> ChatMessage {
    ChatMessage {
        role: "system".into(),
        content,
        images: Vec::new(),
        ..Default::default()
    }
}

pub(crate) fn user_msg(content: String) -> ChatMessage {
    ChatMessage {
        role: "user".into(),
        content,
        images: Vec::new(),
        ..Default::default()
    }
}
