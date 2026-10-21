//! 流式对话:chat_stream 命令与带故障转移的后台流任务。

use std::time::Instant;

use serde::Deserialize;
use tauri::ipc::Channel;

use crate::db::usage::{self, UsageRecord};
use crate::provider::{
    keychain,
    model_config,
    reasoning,
    rotation::{self, KeyCandidate, RotationConfig, RotationState},
    templates,
    ChatMessage, ChatRequest, EffortLevel, Provider, ProviderError, ReasoningSpec, StreamChunk,
};

use super::failover::{
    cooldown_for, error_kind, is_reasoning_rejected, is_retryable, is_stream_options_rejected,
    next_request_id,
};
use super::keys::{
    build_openai, key_candidates, order_slots, resolve_spec, rotation_group, slot_label, ChatSpec,
};
use super::pricing::{agent_of_session, cost_nano_of, pricing_for, PricingCtx};
use super::stream::{run_cancellable, StreamCancelState};

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
}

/// 解析统一思考档位字符串 → ReasoningSpec;非法值返回 None(静默不发送,保守)。
fn parse_reasoning(s: &str) -> Option<ReasoningSpec> {
    match s {
        "off" => Some(ReasoningSpec::Off),
        "light" => Some(ReasoningSpec::Effort { level: EffortLevel::Light }),
        "medium" => Some(ReasoningSpec::Effort { level: EffortLevel::Medium }),
        "high" => Some(ReasoningSpec::Effort { level: EffortLevel::High }),
        "ultra" => Some(ReasoningSpec::Effort { level: EffortLevel::Ultra }),
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
#[tauri::command]
pub async fn chat_stream(
    state: tauri::State<'_, RotationState>,
    cancel_state: tauri::State<'_, StreamCancelState>,
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
    if order_slots(&keychain::list_slots(&spec.provider_id).unwrap_or_default(), &cfg).is_empty() {
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
    let req = ChatRequest {
        model: input.model.clone(),
        messages: input.messages.clone(),
        temperature: input.temperature.or_else(|| ov.and_then(|o| o.temperature)),
        max_tokens: input.max_tokens.or_else(|| ov.and_then(|o| o.max_tokens)),
        top_p: input.top_p.or_else(|| ov.and_then(|o| o.top_p)),
        reasoning,
        stream: true,
    };

    // 3. 归因上下文:会话 → 助手(按助手归因)+ 模型卡定价(成本)
    let agent = input
        .session_id
        .as_deref()
        .map(|sid| agent_of_session(sid))
        .unwrap_or_default();
    let pricing = pricing_for(&spec.provider_id, &input.model);

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
    );
    Ok(())
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
) {
    let work = async move {
        use futures::StreamExt;

        let group = rotation_group(&spec.provider_id, &spec.base_url);
        let mut cands = key_candidates(&spec.provider_id, &spec.base_url, &spec.model, &cfg);
        if cands.is_empty() {
            let _ = channel.send(StreamChunk::Error {
                message: format!("provider {} 未配置 API Key", spec.provider_id),
            });
            return;
        }

        let request_id = next_request_id();
        let started = Instant::now();
        let mut stream_options = cfg.supports_stream_options;
        let mut last_error: Option<String> = None;
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
            let cand_cfg = if own { cfg.clone() } else { rotation::config_for(&cand.provider_id) };
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

            match provider.chat_stream(req.clone()).await {
                Ok(mut stream) => {
                    // 首次 2xx 后即锁定该 Key:stream 期间绝不换 Key
                    let mut first_token_ms: Option<i64> = None;
                    let mut toks: (Option<u32>, Option<u32>, Option<u32>) = (None, None, None);
                    let mut reasoning_toks: Option<u32> = None;
                    let mut cached_toks: Option<u32> = None;
                    let mut stream_err: Option<String> = None;
                    while let Some(chunk) = stream.next().await {
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
                            StreamChunk::Delta { .. } | StreamChunk::Reasoning { .. } => {
                                if first_token_ms.is_none() {
                                    first_token_ms = Some(started.elapsed().as_millis() as i64);
                                }
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
                            }
                            StreamChunk::Error { message } => stream_err = Some(message.clone()),
                        }
                        if channel.send(chunk).is_err() {
                            break; // 前端已关闭 channel
                        }
                    }
                    // 成本(纳元):有定价但 token 不足无法拆分时记 unpriced,不写 0 元假账
                    let cost_nano = attr
                        .pricing
                        .as_ref()
                        .and_then(|p| cost_nano_of(p, toks.0, toks.1, cached_toks));
                    let (pricing_snapshot, cost_source) = match (&attr.pricing, cost_nano) {
                        (Some(p), Some(_)) => (Some(p.snapshot.clone()), Some(p.source.clone())),
                        (Some(p), None) => (Some(p.snapshot.clone()), Some("unpriced".into())),
                        (None, _) => (None, Some("none".into())),
                    };
                    usage::record(&UsageRecord {
                        request_id,
                        session_id: attr.session_id.clone(),
                        provider_id: cand.provider_id.clone(),
                        provider_name,
                        model_id: spec.model.clone(),
                        slot_id: cand.slot,
                        slot_label: label,
                        key_masked: Some(masked),
                        status: if stream_err.is_none() { "success" } else { "failed" }.into(),
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
                    return;
                }
                Err(e) => {
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
                        status: if kind == "rate_limited" { "rate_limited" } else { "failed" }.into(),
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
            }
        }

        let _ = channel.send(StreamChunk::Error {
            message: last_error
                .unwrap_or_else(|| "所有 API Key 均不可用(可能全部被限流)".into()),
        });
    };
    tauri::async_runtime::spawn(run_cancellable(
        cancel_state,
        cancel_id,
        cancel_rx,
        work,
    ));
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
    tauri::async_runtime::spawn(run_cancellable(
        cancel_state,
        cancel_id,
        cancel_rx,
        work,
    ));
}
