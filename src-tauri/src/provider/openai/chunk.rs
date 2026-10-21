//! OpenAI 兼容流式响应解析:单 chunk JSON → StreamChunk,以及 SSE 字节流组装。

use futures::StreamExt;

use crate::provider::{ChatStream, StreamChunk};

use super::OpenAiCompat;

impl OpenAiCompat {
    /// 把 OpenAI 单次响应(JSON)解析成 StreamChunk。
    pub(super) fn parse_chunk(data: &str) -> Option<StreamChunk> {
        if data == "[DONE]" {
            return Some(StreamChunk::Done {
                finish_reason: Some("stop".into()),
                prompt_tokens: None,
                completion_tokens: None,
                reasoning_tokens: None,
                cached_tokens: None,
            });
        }
        let v: serde_json::Value = serde_json::from_str(data).ok()?;

        let usage_of = |u: &serde_json::Value| {
            let input = u
                .get("prompt_tokens")
                .and_then(|t| t.as_u64())
                .map(|t| t as u32);
            let mut output = u
                .get("completion_tokens")
                .and_then(|t| t.as_u64())
                .map(|t| t as u32);
            let total = u
                .get("total_tokens")
                .and_then(|t| t.as_u64())
                .map(|t| t as u32);
            // 思考 token:OpenAI 官方放在 completion_tokens_details.reasoning_tokens;
            // 部分国产厂商直接平铺 reasoning_tokens,两处都尝试
            let reasoning = u
                .get("completion_tokens_details")
                .and_then(|d| d.get("reasoning_tokens"))
                .and_then(|t| t.as_u64())
                .or_else(|| u.get("reasoning_tokens").and_then(|t| t.as_u64()))
                .map(|t| t as u32);
            // 缓存命中输入 token:OpenAI 官方 prompt_tokens_details.cached_tokens;
            // 兼容平铺 cached_tokens 与 Anthropic 风格 cache_read_input_tokens
            let cached = u
                .get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(|t| t.as_u64())
                .or_else(|| u.get("cached_tokens").and_then(|t| t.as_u64()))
                .or_else(|| u.get("cache_read_input_tokens").and_then(|t| t.as_u64()))
                .map(|t| t as u32)
                // 命中的一定是输入的一部分,超过输入时按输入封顶
                .map(|c| input.map(|i| c.min(i)).unwrap_or(c));
            // 口径完整性:reasoning 已含在 completion_tokens 内(OpenAI 语义)。
            // 部分厂商只报 reasoning 不报 output,或 output < reasoning,则按 reasoning 兜底,
            // 保证「其中思考」不超过总输出,避免前端误读为额外 token。
            match (output, reasoning) {
                (Some(o), Some(r)) if r > o => output = Some(r),
                (None, Some(r)) => output = Some(r),
                _ => {}
            }
            (input, output, total, reasoning, cached)
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
                let (input_tokens, output_tokens, _, reasoning_tokens, cached_tokens) =
                    match v.get("usage") {
                        Some(u) => usage_of(u),
                        None => (None, None, None, None, None),
                    };
                return Some(StreamChunk::Done {
                    finish_reason: Some(fr.to_string()),
                    prompt_tokens: input_tokens,
                    completion_tokens: output_tokens,
                    reasoning_tokens,
                    cached_tokens,
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
                let (input_tokens, output_tokens, total_tokens, reasoning_tokens, cached_tokens) =
                    usage_of(usage);
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
                        cached_tokens,
                    });
                }
            }
        }
        None
    }
}

/// 把 SSE 响应体组装成 StreamChunk 流:按行读取,提取 data: 行,喂给 parse_chunk。
///
/// 用字节缓冲拼行:多字节 UTF-8 字符(如 4 字节 emoji)可能被 HTTP chunk 切断,
/// 直接对单个 chunk from_utf8_lossy 会产生 U+FFFD 乱码;先在字节层找 '\n'
/// (0x0A 不可能出现在多字节序列内部),只对完整行解码即可避免。
pub(super) fn sse_stream(resp: reqwest::Response) -> ChatStream {
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
                        if let Some(chunk) = OpenAiCompat::parse_chunk(data) {
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

    Box::pin(s)
}
