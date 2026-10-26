//! OpenAI 兼容流式响应解析:单 chunk JSON → StreamChunk,以及 SSE 字节流组装。

use std::collections::BTreeMap;

use futures::StreamExt;

use crate::provider::{ChatStream, StreamChunk, ToolCall, ToolCallFunction};

use super::OpenAiCompat;

/// tool_calls delta 跨 chunk 累积器 —— OpenAI streaming 按 index 分块传递 tool_calls,
/// arguments 是 JSON 字符串分片,需要拼完才能给前端/MCP 用。
#[derive(Default)]
struct ToolAcc {
    calls: BTreeMap<usize, ToolCallAcc>,
}

#[derive(Default)]
struct ToolCallAcc {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl ToolAcc {
    fn feed(&mut self, data: &str) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
            return;
        };
        let Some(choice) = v.get("choices").and_then(|c| c.get(0)) else {
            return;
        };
        let Some(deltas) = choice
            .get("delta")
            .and_then(|d| d.get("tool_calls"))
            .and_then(|t| t.as_array())
        else {
            return;
        };
        for d in deltas {
            let idx = d
                .get("index")
                .and_then(|i| i.as_u64())
                .map(|i| i as usize)
                .unwrap_or(0);
            let acc = self.calls.entry(idx).or_default();
            if let Some(id) = d.get("id").and_then(|s| s.as_str())
                && !id.is_empty()
            {
                acc.id = Some(id.to_string());
            }
            if let Some(func) = d.get("function") {
                if let Some(name) = func.get("name").and_then(|s| s.as_str())
                    && !name.is_empty()
                {
                    acc.name = Some(name.to_string());
                }
                if let Some(args) = func.get("arguments").and_then(|s| s.as_str()) {
                    acc.arguments.push_str(args);
                }
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    fn take(&mut self) -> Vec<ToolCall> {
        // 取走即清空:避免同一批 tool_calls 在后续 Done chunk 上被重复派发
        std::mem::take(&mut self.calls)
            .iter()
            .map(|(i, a)| ToolCall {
                id: a.id.clone().unwrap_or_else(|| format!("call_{i}")),
                r#type: "function".into(),
                function: ToolCallFunction {
                    name: a.name.clone().unwrap_or_default(),
                    arguments: a.arguments.clone(),
                },
            })
            .collect()
    }
}

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
                && !rc.is_empty()
            {
                return Some(StreamChunk::Reasoning {
                    content: rc.to_string(),
                });
            }
            // delta.content
            if let Some(content) = choice
                .get("delta")
                .and_then(|d| d.get("content"))
                .and_then(|c| c.as_str())
                && !content.is_empty()
            {
                return Some(StreamChunk::Delta {
                    content: content.to_string(),
                });
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
            && let Some(usage) = v.get("usage").filter(|u| !u.is_null())
        {
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
        (
            byte_stream,
            Vec::<u8>::new(),
            false,
            ToolAcc::default(),
            None,
        ),
        |(mut bs, mut buf, aborted, mut tool_acc, mut pending)| async move {
            // 先吐 pending 里的 chunk(如果有的话 —— ToolCalls 先于 Done)
            if let Some(p) = pending.take() {
                return Some((p, (bs, buf, aborted, tool_acc, None)));
            }
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
                        // 先喂 tool_calls 累积器(静默忽略非 tool_calls 的 chunk)
                        tool_acc.feed(data);
                        if let Some(chunk) = OpenAiCompat::parse_chunk(data) {
                            // Done(tool_calls) + acc 非空 → 先吐 ToolCalls,再吐 Done
                            if let StreamChunk::Done {
                                ref finish_reason, ..
                            } = chunk
                                && finish_reason.as_deref() == Some("tool_calls")
                                && !tool_acc.is_empty()
                            {
                                let calls = tool_acc.take();
                                let tool_calls_chunk = StreamChunk::ToolCalls { calls };
                                pending = Some(chunk);
                                return Some((
                                    tool_calls_chunk,
                                    (bs, buf, aborted, tool_acc, pending),
                                ));
                            }
                            // Done(stop/...) 时重置 tool_acc
                            if matches!(chunk, StreamChunk::Done { .. }) {
                                tool_acc = ToolAcc::default();
                            }
                            return Some((chunk, (bs, buf, aborted, tool_acc, pending)));
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
                                (bs, buf, true, tool_acc, pending),
                            ));
                        }
                    }
                    Some(Err(e)) => {
                        return Some((
                            StreamChunk::Error {
                                message: format!("stream read error: {e}"),
                            },
                            (bs, buf, false, tool_acc, pending),
                        ));
                    }
                    None => return None,
                }
            }
        },
    );

    Box::pin(s)
}

#[cfg(test)]
mod tool_acc_tests {
    use super::ToolAcc;

    /// OpenAI streaming 把 tool_calls 拆成多个 chunk,arguments 是 JSON 分片:
    /// 必须按 index 分桶拼接,才能还原出可执行的完整调用。
    #[test]
    fn accumulates_parallel_tool_calls_by_index() {
        let mut acc = ToolAcc::default();
        acc.feed(r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_a","function":{"name":"read_file","arguments":""}}]}}]}"#);
        acc.feed(r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"id":"call_b","function":{"name":"list_dir","arguments":""}}]}}]}"#);
        acc.feed(r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\":"}}]}}]}"#);
        acc.feed(r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"a.txt\"}"}}]}}]}"#);
        acc.feed(r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"arguments":"{\"dir\":\".\"}"}}]}}]}"#);

        let calls = acc.take();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "call_a");
        assert_eq!(calls[0].function.name, "read_file");
        assert_eq!(calls[0].function.arguments, r#"{"path":"a.txt"}"#);
        assert_eq!(calls[1].id, "call_b");
        assert_eq!(calls[1].function.arguments, r#"{"dir":"."}"#);
        // take 后累积器清空
        assert!(acc.is_empty());
    }

    /// 普通正文 chunk 不含 tool_calls,累积器必须保持为空(不误触发工具循环)。
    #[test]
    fn ignores_plain_content_chunks() {
        let mut acc = ToolAcc::default();
        acc.feed(r#"{"choices":[{"delta":{"content":"你好"}}]}"#);
        acc.feed("不是 JSON");
        acc.feed(r#"{"choices":[]}"#);
        assert!(acc.is_empty());
    }
}
