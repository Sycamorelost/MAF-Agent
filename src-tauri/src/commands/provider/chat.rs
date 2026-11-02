//! 流式对话:chat_stream 命令与带故障转移的后台流任务。

use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::Emitter;
use tauri::ipc::Channel;

use crate::db::usage::{self, UsageRecord};
use crate::provider::{
    ChatMessage, ChatRequest, EffortLevel, Provider, ProviderError, ReasoningSpec, StreamChunk,
    ToolCall, keychain, model_config, reasoning,
    rotation::{self, KeyCandidate, RotationConfig, RotationState},
    templates,
};

use super::context_budget::ContextBudget;
use super::failover::{
    cooldown_for, error_kind, is_reasoning_rejected, is_retryable, is_stream_options_rejected,
    next_request_id,
};
use super::keys::{
    ChatSpec, build_openai, key_candidates, order_slots, resolve_spec, rotation_group, slot_label,
};
use super::pricing::{PricingCtx, agent_of_session, cost_nano_of, pricing_for};
use super::stream::{StreamCancelState, run_cancellable};

// ---------- chat_stream ----------

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

/// 解析统一思考档位字符串 → ReasoningSpec;非法值返回 None(静默不发送,保守)。
fn parse_reasoning(s: &str) -> Option<ReasoningSpec> {
    match s {
        "off" => Some(ReasoningSpec::Off),
        "light" => Some(ReasoningSpec::Effort {
            level: EffortLevel::Light,
        }),
        "medium" => Some(ReasoningSpec::Effort {
            level: EffortLevel::Medium,
        }),
        "high" => Some(ReasoningSpec::Effort {
            level: EffortLevel::High,
        }),
        "ultra" => Some(ReasoningSpec::Effort {
            level: EffortLevel::Ultra,
        }),
        _ => None,
    }
}

/// 合并思考档位(优先级:请求显式值 > 单模型默认档 > 全局默认 medium)。
/// 前提:该模型具备思考能力(reasoning::resolve),否则一律 None(不发送,避免 400)。
fn merge_reasoning(
    input_level: Option<&str>,
    ov_default: Option<&str>,
    provider_id: &str,
    model: &str,
) -> Option<ReasoningSpec> {
    let cap = reasoning::resolve(provider_id, model);
    if !cap.enabled {
        return None;
    }
    input_level
        .and_then(parse_reasoning)
        .or_else(|| ov_default.and_then(parse_reasoning))
        .or_else(|| parse_reasoning("medium"))
}

/// 流式对话。结果通过 `channel` 逐 chunk 推给前端,命令本身立即返回。
///
/// API 流转(多 Key 轮换):同一 Provider 的多把 Key 轮流承担请求;
/// 仅当**初始 HTTP 响应非 2xx 且尚未推送任何 chunk** 时轮换重试,
/// stream 一旦吐出 chunk 就不再换 Key(语义安全,不会重复渲染)。
// 参数多为 Tauri State 注入与 IPC 契约字段,无法收敛为结构体
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn chat_stream(
    state: tauri::State<'_, RotationState>,
    cancel_state: tauri::State<'_, StreamCancelState>,
    mcp_runtime: tauri::State<'_, crate::mcp::McpRuntime>,
    approval_store: tauri::State<'_, crate::mcp::ApprovalStore>,
    capability_store: tauri::State<'_, crate::mcp::CapabilityStore>,
    app: tauri::AppHandle,
    input: ChatStreamInput,
    channel: Channel<StreamChunk>,
) -> Result<(), String> {
    // 1. 组装 Provider 规格(base_url / display_name 优先显式入参,回退模板)
    let spec = resolve_spec(
        &input.provider_id,
        &input.model,
        input.custom_base_url.as_deref(),
        input.custom_display_name.as_deref(),
    )?;
    let cfg = rotation::config_for(&spec.provider_id);
    if order_slots(
        &keychain::list_slots(&spec.provider_id).unwrap_or_default(),
        &cfg,
    )
    .is_empty()
    {
        return Err(format!("provider {} 未配置 API Key", spec.provider_id));
    }
    let cancel_id = input.cancel_id.clone();
    // token 已存入注册表供 cancel_stream 查找;接收端交给后台任务
    let (_, cancel_rx) = cancel_state.register(&cancel_id);

    // 2. 构造请求(单模型配置的默认参数在 input 未显式传值时兜底,迭代 D8;
    //    思考档位合并规则见 merge_reasoning,v1.9.0 工作块 I)
    let model_cfg = model_config::load();
    let ov = model_config::get(&model_cfg, &input.provider_id, &input.model);
    let reasoning = merge_reasoning(
        input.reasoning_effort.as_deref(),
        ov.and_then(|o| o.default_reasoning.as_deref()),
        &input.provider_id,
        &input.model,
    );
    // D6:工作区根下的 AGENTS.md / CLAUDE.md 自动注入 system 基线(零配置,无需用户设置)
    let mut messages = input.messages.clone();
    if let Some(rules) =
        crate::project_rules::inject(&mut messages, input.session_id.as_deref())
    {
        // D7:留下「本轮注入了哪个约定文件」的痕迹
        super::event_log::record_rules(
            input.session_id.as_deref(),
            &rules.source,
            rules.content.chars().count(),
        );
    }
    let req = ChatRequest {
        model: input.model.clone(),
        messages,
        temperature: input.temperature.or_else(|| ov.and_then(|o| o.temperature)),
        max_tokens: input.max_tokens.or_else(|| ov.and_then(|o| o.max_tokens)),
        top_p: input.top_p.or_else(|| ov.and_then(|o| o.top_p)),
        reasoning,
        stream: true,
        tools: input.tools.clone(),
        ..Default::default()
    };
    // D7:记录本轮请求的规模与消息指纹(压缩/注入都会让指纹变化,便于事后比对)
    super::event_log::record_request(
        input.session_id.as_deref(),
        &spec.provider_id,
        &req.model,
        &req.messages,
        req.tools.as_ref().map_or(0, |t| t.len()),
    );

    // 3. 归因上下文:会话 → 助手(按助手归因)+ 模型卡定价(成本)
    let agent = input
        .session_id
        .as_deref()
        .map(agent_of_session)
        .unwrap_or_default();
    let pricing = pricing_for(&spec.provider_id, &input.model);

    // P1-4:首个事件必须 <200ms 到达前端 —— spawn 之前先回「准备中」心跳,
    // 否则工具收集/连接建立/模型排队这整段前端完全无感知(空窗)。
    let _ = channel.send(StreamChunk::Phase {
        phase: "preparing".into(),
    });

    spawn_chat_with_failover(
        spec,
        req,
        channel,
        state.inner().clone(),
        cfg,
        Attribution {
            session_id: input.session_id.clone(),
            agent_id: agent.0,
            agent_name: agent.1,
            pricing,
        },
        cancel_state.inner().clone(),
        cancel_id,
        cancel_rx,
        app,
        mcp_runtime.inner().clone(),
        approval_store.inner().clone(),
        capability_store.inner().clone(),
    );
    Ok(())
}

/// P2(选项 B):整体生成的总时长上限 —— 覆盖连接、首字等待、流式输出与各轮工具调用。
/// 时长由设置「生成时长」决定(默认 10 分钟,1..=1440),大工程可调大。
/// 不额外设首字/空闲分段超时(长任务不会被中途的慢思考误杀),只在总时长到点后收口。
/// 超时会向前端补 `Error`(而不是静默关闭 channel),前端据此保留已生成内容(P1)。
fn remaining_budget(started: &Instant, total: Duration) -> Duration {
    total.saturating_sub(started.elapsed())
}

/// 用量归因上下文(与 Key 选择无关,仅影响落库内容)。
struct Attribution {
    session_id: Option<String>,
    agent_id: Option<String>,
    agent_name: Option<String>,
    pricing: Option<PricingCtx>,
}

/// 带轮换的流式对话:按候选 Key 逐个尝试,遇限流/5xx/网络错误则冷却并换下一把。
/// 候选池可能包含同 base_url 编组内其他 Provider 的 Key(Phase 3)。
/// 全部候选耗尽时**只推一次** Error。
// 参数为流式链路必需的独立上下文(请求/通道/取消/轮换/运行时等),收敛为结构体只会改名不换药
#[allow(clippy::too_many_arguments)]
fn spawn_chat_with_failover(
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
        use futures::StreamExt;

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
        let data_dir = crate::paths::data_dir();
        // harness 子调用通道(上下文压缩 / delegate 委派):取最小槽位 Key 构造一次,全程复用;
        // 无 Key 时为 None,子功能自动降级,主对话不受影响
        let sub_llm = super::keys::build_sub_llm(&spec);

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
                    super::compress::compress_context(&mut req.messages, llm, &budget).await
                    && n > 0
                {
                    // D7:记录压缩前后的条数与预算口径,便于事后核对「为什么被压缩」
                    super::event_log::record_compress(
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
                let masked = usage::mask_key(&key);
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
                        let mut first_token_ms: Option<i64> = None;
                        let mut toks: (Option<u32>, Option<u32>, Option<u32>) = (None, None, None);
                        let mut reasoning_toks: Option<u32> = None;
                        let mut cached_toks: Option<u32> = None;
                        let mut stream_err: Option<String> = None;
                        // Phase 5e:本轮累积的正文(组装 assistant 消息用)+ 模型请求的工具调用
                        let mut assistant_text = String::new();
                        let mut pending_calls: Option<Vec<ToolCall>> = None;
                        // 本轮是否已下发给前端一个 done(部分中转站流末尾不带 done)
                        let mut saw_done = false;
                        // P2:每个 chunk 只受「剩余总预算」约束(不按空闲时长打断长思考)
                        let mut timed_out = false;
                        loop {
                            let chunk =
                                match tokio::time::timeout(
                                    remaining_budget(&started, total_timeout),
                                    stream.next(),
                                )
                                    .await
                                {
                                    Ok(Some(c)) => c,
                                    Ok(None) => break,
                                    Err(_) => {
                                        timed_out = true;
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
                                    toks = (*input_tokens, *output_tokens, *total_tokens);
                                    if reasoning_toks.is_none() {
                                        reasoning_toks = *reasoning_tokens;
                                    }
                                    if cached_toks.is_none() {
                                        cached_toks = *cached_tokens;
                                    }
                                    continue; // 仅后端采集,不转发前端
                                }
                                // 思考流也算首响(用户感知的第一字来自思考阶段)
                                StreamChunk::Delta { content } => {
                                    assistant_text.push_str(content);
                                    if first_token_ms.is_none() {
                                        first_token_ms = Some(started.elapsed().as_millis() as i64);
                                    }
                                }
                                StreamChunk::Reasoning { .. } => {
                                    if first_token_ms.is_none() {
                                        first_token_ms = Some(started.elapsed().as_millis() as i64);
                                    }
                                }
                                // 工具调用请求:仅后端消费,由 dispatcher 审批+执行后以 ToolCallResult 推前端
                                StreamChunk::ToolCalls { calls } => {
                                    pending_calls = Some(calls.clone());
                                    continue;
                                }
                                StreamChunk::Done {
                                    prompt_tokens,
                                    completion_tokens,
                                    reasoning_tokens,
                                    cached_tokens,
                                    ..
                                } => {
                                    if toks.2.is_none() {
                                        let i = toks.0.or(*prompt_tokens);
                                        let o = toks.1.or(*completion_tokens);
                                        toks = (i, o, i.zip(o).map(|(a, b)| a + b));
                                    }
                                    if reasoning_toks.is_none() {
                                        reasoning_toks = *reasoning_tokens;
                                    }
                                    if cached_toks.is_none() {
                                        cached_toks = *cached_tokens;
                                    }
                                    // 本轮请求了工具调用:这个 done 只代表"本轮生成结束",
                                    // 整条流还没结束(后面还有工具结果 + 下一轮正文)。
                                    // 若现在转发,前端会立即收流,后续 delta 会挂到已结束的流上,
                                    // 表现为最后一条回复一直显示"闪烁光标"且不落库。
                                    saw_done = true;
                                    if pending_calls.is_some() {
                                        continue;
                                    }
                                }
                                StreamChunk::Error { message } => {
                                    stream_err = Some(message.clone())
                                }
                                // Phase 5e 新增的两个变体:不影响 token 采集,直接转发前端
                                _ => {}
                            }
                            if channel.send(chunk).is_err() {
                                break; // 前端已关闭 channel
                            }
                        }
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
                        // 成本(纳元):有定价但 token 不足无法拆分时记 unpriced,不写 0 元假账
                        let cost_nano = attr
                            .pricing
                            .as_ref()
                            .and_then(|p| cost_nano_of(p, toks.0, toks.1, cached_toks));
                        let (pricing_snapshot, cost_source) = match (&attr.pricing, cost_nano) {
                            (Some(p), Some(_)) => {
                                (Some(p.snapshot.clone()), Some(p.source.clone()))
                            }
                            (Some(p), None) => (Some(p.snapshot.clone()), Some("unpriced".into())),
                            (None, _) => (None, Some("none".into())),
                        };
                        usage::record(&UsageRecord {
                            request_id: request_id.clone(),
                            session_id: attr.session_id.clone(),
                            provider_id: cand.provider_id.clone(),
                            provider_name,
                            model_id: spec.model.clone(),
                            slot_id: cand.slot,
                            slot_label: label,
                            key_masked: Some(masked),
                            status: if stream_err.is_none() {
                                "success"
                            } else {
                                "failed"
                            }
                            .into(),
                            error_kind: stream_err.as_ref().map(|_| "stream".to_string()),
                            input_tokens: toks.0,
                            output_tokens: toks.1,
                            total_tokens: toks.2,
                            reasoning_tokens: reasoning_toks,
                            cached_tokens: cached_toks,
                            first_token_ms,
                            total_ms: Some(started.elapsed().as_millis() as i64),
                            agent_id: attr.agent_id.clone(),
                            agent_name: attr.agent_name.clone(),
                            cost_nano,
                            pricing_snapshot,
                            cost_source,
                        });

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
                            // 工具全部失败且非用户拒绝(如 server 未启用/执行报错)→ 提醒模型别再重试同一工具
                            if !outcome.all_ok && !outcome.denied {
                                req.messages.push(ChatMessage {
                                role: "system".into(),
                                content: "上一轮工具调用未成功。请勿重复调用同一工具,直接基于现有信息回答用户。"
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
                        usage::record(&UsageRecord {
                            request_id: request_id.clone(),
                            session_id: attr.session_id.clone(),
                            provider_id: cand.provider_id.clone(),
                            provider_name,
                            model_id: spec.model.clone(),
                            slot_id: cand.slot,
                            slot_label: label,
                            key_masked: Some(masked),
                            status: if kind == "rate_limited" {
                                "rate_limited"
                            } else {
                                "failed"
                            }
                            .into(),
                            error_kind: Some(kind.to_string()),
                            total_ms: Some(started.elapsed().as_millis() as i64),
                            agent_id: attr.agent_id.clone(),
                            agent_name: attr.agent_name.clone(),
                            cost_source: Some("none".into()),
                            ..Default::default()
                        });
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
    // 终态兜底:后台任务 panic 时 Tokio 只会吞掉 JoinHandle 的 panic,channel 静默关闭,
    // 前端 `done` 永不 resolve(表现为「思考/工具调用之后再无任何反应」)。
    // 这里 catch_unwind 后补发一个 Error chunk,保证前端一定收口。
    tauri::async_runtime::spawn(async move {
        use futures::FutureExt;
        let fut = run_cancellable(cancel_state, cancel_id, cancel_rx, work);
        if std::panic::AssertUnwindSafe(fut)
            .catch_unwind()
            .await
            .is_err()
        {
            log::error!("chat stream task panicked; emit Error chunk to close the stream");
            let _ = panic_channel.send(StreamChunk::Error {
                message: "对话流异常中断(内部错误),请重试".into(),
            });
        }
    });
}

/// 单次请求最多携带的消息条数(200 轮工具调用时防止上下文无限膨胀)。
const MAX_CONTEXT_MESSAGES: usize = 40;

/// 是否超预算(触发 LLM 压缩;压缩后仍超限由 prune_context 硬裁剪兜底)。
fn context_over_budget(messages: &[ChatMessage], budget: &ContextBudget) -> bool {
    messages.len() > MAX_CONTEXT_MESSAGES || budget.over(messages)
}

/// 裁剪过长的工具调用上下文:
///   - 永远保留 system;
///   - 其余只保留最近 N 条 / 预算内(D5:预算按模型上下文窗口折算为 token);
///   - 裁剪后若开头是孤立的 tool 消息(其 assistant 已被裁掉),一并丢弃,
///     否则 OpenAI function-calling 协议不合法(400)。
fn prune_context(messages: &mut Vec<ChatMessage>, budget: &ContextBudget) {
    let total: usize = messages.iter().map(|m| budget.weight(m)).sum();
    if messages.len() <= MAX_CONTEXT_MESSAGES && total <= budget.limit() {
        return;
    }
    let system: Vec<ChatMessage> = messages
        .iter()
        .filter(|m| m.role == "system")
        .cloned()
        .collect();
    let keep = MAX_CONTEXT_MESSAGES.saturating_sub(system.len()).max(1);
    let mut tail: Vec<ChatMessage> = messages
        .iter()
        .filter(|m| m.role != "system")
        .rev()
        .take(keep)
        .cloned()
        .collect();
    tail.reverse();

    let mut weight: usize = system.iter().chain(tail.iter()).map(|m| budget.weight(m)).sum();
    while weight > budget.limit() && tail.len() > 1 {
        weight = weight.saturating_sub(budget.weight(&tail[0]));
        tail.remove(0);
    }
    // 丢掉开头没有对应 assistant 的 tool 结果
    while tail.first().map(|m| m.role == "tool").unwrap_or(false) {
        tail.remove(0);
    }

    messages.clear();
    messages.extend(system);
    messages.extend(tail);
}

/// 在后台任务中跑流并推到 channel,避免阻塞 IPC 主调用(translate_stream 共用)。
pub(crate) fn spawn_chat_to_channel(
    provider: Box<dyn Provider>,
    req: ChatRequest,
    channel: Channel<StreamChunk>,
    cancel_state: StreamCancelState,
    cancel_id: String,
    cancel_rx: futures::channel::oneshot::Receiver<()>,
) {
    let work = async move {
        match provider.chat_stream(req).await {
            Ok(mut stream) => {
                use futures::StreamExt;
                while let Some(chunk) = stream.next().await {
                    // Usage chunk 仅用于用量归因,不转发前端
                    if matches!(chunk, StreamChunk::Usage { .. }) {
                        continue;
                    }
                    if channel.send(chunk).is_err() {
                        // 前端已关闭 channel,直接退出
                        break;
                    }
                }
            }
            Err(e) => {
                let _ = channel.send(StreamChunk::Error {
                    message: e.to_string(),
                });
            }
        }
    };
    tauri::async_runtime::spawn(run_cancellable(cancel_state, cancel_id, cancel_rx, work));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.into(),
            content: content.into(),
            ..Default::default()
        }
    }

    #[test]
    fn prune_keeps_short_context_untouched() {
        let mut m = vec![msg("system", "s"), msg("user", "u")];
        prune_context(&mut m, &ContextBudget::unknown());
        assert_eq!(m.len(), 2);
    }

    /// P6:预算按 Unicode 字符计 —— 大量中文不应因 UTF-8 字节放大而被过早裁掉。
    #[test]
    fn budget_counts_chars_not_bytes_for_chinese() {
        let zh = "中".repeat(4_000); // 4k 字符 / 12k 字节
        let mut m = vec![msg("system", "s")];
        for _ in 0..10 {
            m.push(msg("user", &zh));
        }
        // 40k 字符(120k 字节)在 48k 字符预算内 → 不裁剪
        let budget = ContextBudget::unknown();
        assert!(!context_over_budget(&m, &budget));
        prune_context(&mut m, &budget);
        assert_eq!(m.len(), 11);
    }

    #[test]
    fn prune_trims_oldest_rounds_and_orphan_tools() {
        let mut m = vec![msg("system", "s"), msg("user", "u0")];
        for i in 0..120 {
            let mut a = msg("assistant", "");
            a.tool_calls = Some(Vec::new());
            m.push(a);
            m.push(msg("tool", &format!("r{i}")));
        }
        let budget = ContextBudget::unknown();
        prune_context(&mut m, &budget);
        assert!(m.len() <= MAX_CONTEXT_MESSAGES);
        assert_eq!(m[0].role, "system");
        assert_ne!(m[1].role, "tool", "裁剪后开头不应是孤立 tool 消息");
        assert_eq!(m.last().unwrap().content, "r119", "必须保留最新一轮结果");
    }
}
