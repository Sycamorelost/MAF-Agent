//! `chat_stream` Tauri 命令:组装请求并启动带故障转移的后台流任务。

use tauri::ipc::Channel;

use crate::provider::{
    ChatRequest, StreamChunk, keychain, model_config,
    rotation::{self, RotationState},
};

use super::super::keys::{order_slots, resolve_spec};
use super::super::pricing::{agent_of_session, pricing_for};
use super::super::stream::StreamCancelState;
use super::ChatStreamInput;
use super::failover::{Attribution, spawn_chat_with_failover};
use super::reasoning::merge_reasoning;

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
        super::super::event_log::record_rules(
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
    super::super::event_log::record_request(
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
