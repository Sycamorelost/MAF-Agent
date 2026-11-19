//! 带多 Key 轮换与故障转移的流式对话主循环。

mod guards;
mod runner;
mod stream;
mod types;
mod usage;

use std::time::{Duration, Instant};

use tauri::ipc::Channel;

use crate::db::usage as db_usage;
use crate::provider::{
    ChatMessage, ChatRequest, StreamChunk, ToolCall, keychain,
    rotation::{self, KeyCandidate, RotationConfig, RotationState},
    templates,
};

use super::super::context_budget::ContextBudget;
use super::super::failover::{
    cooldown_for, error_kind, is_reasoning_rejected, is_retryable, is_stream_options_rejected,
    next_request_id,
};
use super::super::keys::{ChatSpec, build_openai, key_candidates, rotation_group, slot_label};
use super::super::stream::StreamCancelState;
use super::context::{context_over_budget, prune_context};
use guards::{remaining_budget, todos_sync_needed};
use stream::{StreamOutcome, consume_stream};
use usage::{record_failure, record_success};

pub(crate) use types::Attribution;

/// 带轮换的流式对话:按候选 Key 逐个尝试,遇限流/5xx/网络错误则冷却并换下一把。
/// 候选池可能包含同 base_url 编组内其他 Provider 的 Key(Phase 3)。
/// 全部候选耗尽时**只推一次** Error。
// 参数为流式链路必需的独立上下文(请求/通道/取消/轮换/运行时等),收敛为结构体只会改名不换药
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_chat_with_failover(
    spec: ChatSpec,
    mut req: ChatRequest,
    channel: Channel<StreamChunk>,
    state: RotationState,
    cfg: RotationConfig,
    attr: Attribution,
    cancel_state: StreamCancelState,
    cancel_id: String,
    cancel_rx: futures::channel::oneshot::Receiver<()>,
    app: tauri::AppHandle,
    mcp_runtime: crate::mcp::McpRuntime,
    approval_store: crate::mcp::ApprovalStore,
    capability_store: crate::mcp::CapabilityStore,
) {
    // panic 兜底用的 channel 副本(必须在使用 work 之前克隆)
    let panic_channel = channel.clone();
    let work = async move {
        let group = rotation_group(&spec.provider_id, &spec.base_url);
        let request_id = next_request_id();
        let started = Instant::now();
        // 每次请求开始时读一次设置(默认 10 分钟,已夹到 1..=1440):改完设置对下一次请求生效
        let total_minutes = crate::settings::chat_timeout_minutes();
        let total_timeout = Duration::from_secs(u64::from(total_minutes) * 60);
        // D5:按模型上下文窗口算预算(未知窗口回落 48k 字符),整轮对话只算一次
        let budget = ContextBudget::for_model(&spec.provider_id, &spec.model);
        let mut stream_options = cfg.supports_stream_options;
        let mut last_error: Option<String> = None;
        // 工具调用轮次(模型每次要求调工具算一轮;上限 200 轮,支持长任务连续执行)
        const MAX_TOOL_ROUNDS: usize = 200;
        let mut round: usize = 0;
        // 任务清单同步:模型连续未回写 todo 的轮数(达阈值随下一轮顺带提醒,不额外耗轮)
        let mut todo_idle_rounds: u32 = 0;
        // 收尾同步:nudged=已补过一轮(至多一轮,防死循环);
        // pending=最终 Done 已就绪、待注入提醒(注入后立即复位,绝不二次触发)
        let mut todo_nudged = false;
        let data_dir = crate::paths::data_dir();
        // harness 子调用通道(上下文压缩 / delegate 委派):取最小槽位 Key 构造一次,全程复用;
        // 无 Key 时为 None,子功能自动降级,主对话不受影响
        let sub_llm = super::super::keys::build_sub_llm(&spec);

        'rounds: loop {
            // P2:总时长到点即收口(工具执行同样计入预算),避免单轮工具卡死绕过上限。
            if remaining_budget(&started, total_timeout).is_zero() {
                let _ = channel.send(StreamChunk::Error {
                    message: format!(
                        "生成超时(总时长超过 {} 分钟),已中断;已生成内容会保留。",
                        total_minutes
                    ),
                });
                return;
            }
            // 200 轮工具调用的前提:每轮都会往 req.messages 追加 assistant + tool 结果。
            // 超预算时先用 LLM 摘要压缩早期轮次(保住需求与结论);压缩失败/不可用回落硬裁剪。
            if context_over_budget(&req.messages, &budget)
                && let Some(llm) = sub_llm.as_ref()
            {
                let before = req.messages.len();
                if let Ok(n) =
                    super::super::compress::compress_context(&mut req.messages, llm, &budget).await
                    && n > 0
                {
                    // D7:记录压缩前后的条数与预算口径,便于事后核对「为什么被压缩」
                    super::super::event_log::record_compress(
                        attr.session_id.as_deref(),
                        n,
                        before,
                        req.messages.len(),
                        &budget,
                    );
                    if let Some(sid) = attr.session_id.as_deref() {
                        crate::diagnostics::safe_emit(
                            &app,
                            &format!("chat-notice-{sid}"),
                            format!("上下文过长,已自动把早期 {n} 条对话压缩为摘要"),
                        );
                    }
                }
            }
            prune_context(&mut req.messages, &budget);
            let mut cands = key_candidates(&spec.provider_id, &spec.base_url, &spec.model, &cfg);
            if cands.is_empty() {
                let _ = channel.send(StreamChunk::Error {
                    message: format!("provider {} 未配置 API Key", spec.provider_id),
                });
                return;
            }

            // 每候选至多一次 + 1 次 stream_options 回退重试 + 1 次 reasoning 回退重试
            let max_attempts = cands.len() + 2;
            // stream_options 被拒后,记住要重试的候选
            let mut fallback_cand: Option<KeyCandidate> = None;

            for _ in 0..max_attempts {
                if cands.is_empty() {
                    break;
                }
                let cand = match fallback_cand.take() {
                    Some(c) => c,
                    None => match state.pick(&group, &cands) {
                        Some(i) => cands[i].clone(),
                        None => break,
                    },
                };
                // 编组内对端 Provider 的槽位元数据取自其自身配置
                let own = cand.provider_id == spec.provider_id;
                let cand_cfg = if own {
                    cfg.clone()
                } else {
                    rotation::config_for(&cand.provider_id)
                };
                let label = slot_label(&cand_cfg, cand.slot);
                let provider_name = if own {
                    Some(spec.display_name.clone())
                } else {
                    templates::find_template(&cand.provider_id).map(|t| t.display_name)
                };
                let key = match keychain::load_slot(&cand.provider_id, cand.slot) {
                    Ok(Some(k)) => k,
                    Ok(None) => {
                        // 密钥文件缺失:剔除该候选继续
                        cands.retain(|c| c.cooldown_key() != cand.cooldown_key());
                        continue;
                    }
                    Err(e) => {
                        last_error = Some(e);
                        break;
                    }
                };
                let masked = db_usage::mask_key(&key);
                let provider = build_openai(&spec, &key, stream_options);

                // P1-4:即将发起 HTTPS 连接(TLS 握手 + 排队等 2xx),知会前端推进占位文案
                let _ = channel.send(StreamChunk::Phase {
                    phase: "connecting".into(),
                });

                // P2:建立连接 + 等到 2xx 响应的剩余总预算(不再单独设首字超时)
                let connect = tokio::time::timeout(
                    remaining_budget(&started, total_timeout),
                    provider.chat_stream(req.clone()),
                )
                .await;
                match connect {
                    Ok(Ok(mut stream)) => {
                        // 首次 2xx 后即锁定该 Key:stream 期间绝不换 Key
                        let StreamOutcome {
                            mut assistant_text,
                            mut reasoning_text,
                            mut pending_calls,
                            toks,
                            reasoning_toks,
                            cached_toks,
                            stream_err,
                            saw_done,
                            timed_out,
                            first_token_ms,
                            todo_sync_pending,
                        } = consume_stream(&mut stream, &channel, &started, total_timeout, &attr, todo_nudged)
                            .await;
                        // P2:超时收口 —— 补一个明确的 Error,前端据此保留已生成内容(P1)。
                        // 不执行已请求的工具:结果不完整时执行写入类工具风险更高。
                        if timed_out {
                            let msg = format!(
                                "生成超时(总时长超过 {} 分钟),已中断;已生成内容会保留。",
                                total_minutes
                            );
                            let _ = channel.send(StreamChunk::Error { message: msg });
                            return;
                        }
                        record_success(
                            &request_id,
                            &started,
                            &spec,
                            &cand,
                            provider_name.clone(),
                            label.clone(),
                            masked.clone(),
                            &attr,
                            toks,
                            reasoning_toks,
                            cached_toks,
                            first_token_ms,
                            stream_err.as_ref(),
                        );

                        // 任务清单收尾同步:正文已就绪 → 回填 assistant 正文与同步提醒,
                        // 补一轮让模型先调用 todo_write 对齐面板进度,再真正结束
                        if todo_sync_pending && pending_calls.is_none() {
                            if stream_err.is_none() && round < MAX_TOOL_ROUNDS {
                                todo_nudged = true;
                                round += 1;
                                req.messages.push(ChatMessage {
                                    role: "assistant".into(),
                                    content: std::mem::take(&mut assistant_text),
                                    reasoning_content: if reasoning_text.is_empty() {
                                        None
                                    } else {
                                        Some(std::mem::take(&mut reasoning_text))
                                    },
                                    ..Default::default()
                                });
                                req.messages.push(ChatMessage {
                                    role: "system".into(),
                                    content: "结束前必须先用 todo_write 同步任务清单:完成项置为 completed,未完成项如实保留 pending/in_progress;若全部完成则全部置为 completed。只调用工具同步,不要重复正文。"
                                        .into(),
                                    ..Default::default()
                                });
                                continue 'rounds;
                            }
                            // 无法补轮(已达工具轮次上限):补发被吞掉的 Done,保证前端正常收流
                            let _ = channel.send(StreamChunk::Done {
                                finish_reason: Some("stop".into()),
                                prompt_tokens: toks.0,
                                completion_tokens: toks.1,
                                reasoning_tokens: reasoning_toks,
                                cached_tokens: cached_toks,
                            });
                            return;
                        }
                        // Phase 5e:模型请求调用工具 → 审批 + 执行 + 回填消息,再发起下一轮
                        if let Some(calls) = pending_calls.take() {
                            if round >= MAX_TOOL_ROUNDS {
                                let _ = channel.send(StreamChunk::Delta {
                                    content: format!(
                                        "\n\n> 已达到 {MAX_TOOL_ROUNDS} 轮工具调用上限,后续工具调用已停止,请基于已有信息作答。\n"
                                    ),
                                });
                                // 收流必须由这里补一个 done,否则前端一轮都等不到结束信号
                                let _ = channel.send(StreamChunk::Done {
                                    finish_reason: Some("tool_round_limit".into()),
                                    prompt_tokens: toks.0,
                                    completion_tokens: toks.1,
                                    reasoning_tokens: reasoning_toks,
                                    cached_tokens: cached_toks,
                                });
                                return;
                            }
                            round += 1;
                            // 任务清单多轮未同步检测:本轮是否回写了 todo(用于下方顺带提醒)
                            let wrote_todos = calls.iter().any(|c| c.function.name == "todo_write");
                            let outcome = crate::mcp::dispatcher::dispatch_tool_calls(
                                &app,
                                calls,
                                std::mem::take(&mut assistant_text),
                                &mcp_runtime,
                                &approval_store,
                                &capability_store,
                                data_dir.clone(),
                                &channel,
                                attr.session_id.as_deref(),
                                sub_llm.as_ref(),
                            )
                            .await;
                            req.messages.extend(outcome.new_messages);
                            // 回填本轮思考内容(DeepSeek 思考模式 + 工具调用要求原样回传)
                            if !reasoning_text.is_empty() {
                                if let Some(first) = req.messages.iter_mut().rev().find(|m| {
                                    m.role == "assistant" && m.reasoning_content.is_none()
                                }) {
                                    first.reasoning_content =
                                        Some(std::mem::take(&mut reasoning_text));
                                }
                            }
                            // 工具全部失败且非用户拒绝(如 server 未启用/执行报错)→ 提醒模型别再重试同一工具
                            if !outcome.all_ok && !outcome.denied {
                                req.messages.push(ChatMessage {
                                    role: "system".into(),
                                    content: "上一轮工具调用未成功。请勿重复调用同一工具,直接基于现有信息回答用户。"
                                        .into(),
                                    ..Default::default()
                                });
                            }
                            // 任务清单多轮未同步:随下一轮请求顺带提醒(不额外消耗轮次)
                            todo_idle_rounds = if wrote_todos { 0 } else { todo_idle_rounds + 1 };
                            if todo_idle_rounds >= 3 && todos_sync_needed(&attr.session_id) {
                                todo_idle_rounds = 0;
                                req.messages.push(ChatMessage {
                                    role: "system".into(),
                                    content: "提醒:任务清单已多轮未更新。每完成一步须立即调用 todo_write(完成项→completed、下一项→in_progress),保持面板进度实时同步。"
                                        .into(),
                                    ..Default::default()
                                });
                            }
                            continue 'rounds;
                        }
                        // 兜底:本轮从未收到过 done(个别中转站在流末尾不发)时补发一个,
                        // 否则前端永远等不到收流信号,流式光标会一直闪。
                        if !saw_done && stream_err.is_none() {
                            let _ = channel.send(StreamChunk::Done {
                                finish_reason: Some("stop".into()),
                                prompt_tokens: toks.0,
                                completion_tokens: toks.1,
                                reasoning_tokens: reasoning_toks,
                                cached_tokens: cached_toks,
                            });
                        }
                        return;
                    }
                    Ok(Err(e)) => {
                        // 兼容回退 1(优先判断):中转站不认思考扩展字段 → 剥离 reasoning,
                        // 同一候选重试一次(与 stream_options 回退同构;剥离后 temperature/top_p
                        // 由 openai.rs 在重建 body 时自动恢复,无需额外处理)
                        if req.reasoning.is_some() && is_reasoning_rejected(&e) {
                            req.reasoning = None;
                            fallback_cand = Some(cand.clone());
                            last_error = Some(e.to_string());
                            continue;
                        }
                        // 兼容回退 2:中转站不认 stream_options → 关掉该字段,同一候选重试一次
                        if stream_options && is_stream_options_rejected(&e) {
                            stream_options = false;
                            fallback_cand = Some(cand.clone());
                            last_error = Some(e.to_string());
                            continue;
                        }

                        let retryable = is_retryable(&e);
                        let kind = error_kind(&e);
                        record_failure(
                            &request_id,
                            &started,
                            &spec,
                            &cand,
                            provider_name,
                            label,
                            masked,
                            &attr,
                            kind,
                        );
                        last_error = Some(e.to_string());
                        if !retryable {
                            break; // 鉴权/参数等致命错误:不再换 Key
                        }
                        // 429 优先采用响应头 Retry-After 给出的建议时长
                        let cool = cooldown_for(&e, cand_cfg.cooldown_secs);
                        state.mark_cooldown_secs(&group, &cand.provider_id, cand.slot, cool);
                        cands.retain(|c| c.cooldown_key() != cand.cooldown_key());
                        continue;
                    }
                    // P2:连接阶段耗尽总预算 —— 换 Key 也救不了「网络/中转站不响应」,
                    // 直接收口并提示用户,避免无限挂起。
                    Err(_elapsed) => {
                        last_error = Some(format!(
                            "生成超时(总时长超过 {} 分钟),已中断;已生成内容会保留。",
                            total_minutes
                        ));
                        break;
                    }
                }
            }

            let _ = channel.send(StreamChunk::Error {
                message: last_error
                    .unwrap_or_else(|| "所有 API Key 均不可用(可能全部被限流)".into()),
            });
            return;
        }
    };
    runner::launch(work, panic_channel, cancel_state, cancel_id, cancel_rx);
}
