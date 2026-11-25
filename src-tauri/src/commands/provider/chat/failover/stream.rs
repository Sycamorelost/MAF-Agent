//! 单轮流式响应的消费:采集正文 / 思考 / token,决定是否转发 chunk。

use crate::provider::{StreamChunk, ToolCall};
use std::time::{Duration, Instant};
use tauri::ipc::Channel;

use super::Attribution;
use super::guards::{awaiting_user_decision, remaining_budget, todos_sync_needed};

/// 一轮流式消费的结果(供主循环判定下一步)。
pub(super) struct StreamOutcome {
    pub(super) assistant_text: String,
    pub(super) reasoning_text: String,
    pub(super) pending_calls: Option<Vec<ToolCall>>,
    pub(super) toks: (Option<u32>, Option<u32>, Option<u32>),
    pub(super) reasoning_toks: Option<u32>,
    pub(super) cached_toks: Option<u32>,
    pub(super) stream_err: Option<String>,
    pub(super) saw_done: bool,
    pub(super) timed_out: bool,
    pub(super) first_token_ms: Option<i64>,
    pub(super) todo_sync_pending: bool,
}

/// 消费一条流直到结束。
///
/// - Usage / ToolCalls 只后端采集,不转发前端;
/// - 本轮请求了工具调用时,流内的 Done 只代表「本轮生成结束」,吞掉不发;
/// - 任务清单收尾同步:最终答复已就绪但面板仍有未完成项 → 吞掉 Done,由主循环补一轮提醒。
#[allow(clippy::too_many_arguments)]
pub(super) async fn consume_stream<S>(
    stream: &mut S,
    channel: &Channel<StreamChunk>,
    started: &Instant,
    total_timeout: Duration,
    attr: &Attribution,
    todo_nudged: bool,
) -> StreamOutcome
where
    S: futures::Stream<Item = StreamChunk> + Unpin,
{
    use futures::StreamExt;

    let mut out = StreamOutcome {
        assistant_text: String::new(),
        reasoning_text: String::new(),
        pending_calls: None,
        toks: (None, None, None),
        reasoning_toks: None,
        cached_toks: None,
        stream_err: None,
        saw_done: false,
        timed_out: false,
        first_token_ms: None,
        todo_sync_pending: false,
    };

    // P2:每个 chunk 只受「剩余总预算」约束(不按空闲时长打断长思考)
    loop {
        let chunk =
            match tokio::time::timeout(remaining_budget(started, total_timeout), stream.next())
                .await
            {
                Ok(Some(c)) => c,
                Ok(None) => break,
                Err(_) => {
                    out.timed_out = true;
                    break;
                }
            };
        match &chunk {
            StreamChunk::Usage {
                input_tokens,
                output_tokens,
                total_tokens,
                reasoning_tokens,
                cached_tokens,
            } => {
                out.toks = (*input_tokens, *output_tokens, *total_tokens);
                if out.reasoning_toks.is_none() {
                    out.reasoning_toks = *reasoning_tokens;
                }
                if out.cached_toks.is_none() {
                    out.cached_toks = *cached_tokens;
                }
                continue; // 仅后端采集,不转发前端
            }
            // 思考流也算首响(用户感知的第一字来自思考阶段)
            StreamChunk::Delta { content } => {
                out.assistant_text.push_str(content);
                if out.first_token_ms.is_none() {
                    out.first_token_ms = Some(started.elapsed().as_millis() as i64);
                }
            }
            StreamChunk::Reasoning { content } => {
                out.reasoning_text.push_str(content);
                if out.first_token_ms.is_none() {
                    out.first_token_ms = Some(started.elapsed().as_millis() as i64);
                }
            }
            // 工具调用请求:仅后端消费,由 dispatcher 审批+执行后以 ToolCallResult 推前端
            StreamChunk::ToolCalls { calls } => {
                out.pending_calls = Some(calls.clone());
                continue;
            }
            StreamChunk::Done {
                prompt_tokens,
                completion_tokens,
                reasoning_tokens,
                cached_tokens,
                ..
            } => {
                if out.toks.2.is_none() {
                    let i = out.toks.0.or(*prompt_tokens);
                    let o = out.toks.1.or(*completion_tokens);
                    out.toks = (i, o, i.zip(o).map(|(a, b)| a + b));
                }
                if out.reasoning_toks.is_none() {
                    out.reasoning_toks = *reasoning_tokens;
                }
                if out.cached_toks.is_none() {
                    out.cached_toks = *cached_tokens;
                }
                // 本轮请求了工具调用:这个 done 只代表"本轮生成结束",
                // 整条流还没结束(后面还有工具结果 + 下一轮正文)。
                // 若现在转发,前端会立即收流,后续 delta 会挂到已结束的流上,
                // 表现为最后一条回复一直显示"闪烁光标"且不落库。
                out.saw_done = true;
                if out.pending_calls.is_some() {
                    continue;
                }
                // 任务清单收尾同步:最终答复已就绪但面板仍有未完成项 → 吞掉这个 Done,
                // 由主循环补一轮「先同步清单」提醒(至多一轮),保证进度与答复一致;
                // 答复在等用户决策(疑问句收尾)时不补轮 —— 此时清单停滞是正确状态
                if !todo_nudged
                    && out.stream_err.is_none()
                    && !awaiting_user_decision(&out.assistant_text)
                    && todos_sync_needed(&attr.session_id)
                {
                    out.todo_sync_pending = true;
                    continue;
                }
            }
            StreamChunk::Error { message } => out.stream_err = Some(message.clone()),
            // Phase 5e 新增的两个变体:不影响 token 采集,直接转发前端
            _ => {}
        }
        if channel.send(chunk).is_err() {
            break; // 前端已关闭 channel
        }
    }
    out
}
