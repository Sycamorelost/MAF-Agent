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
    app: tauri::AppHandle,
    mut input: ChatStreamInput,
    channel: Channel<StreamChunk>,
) -> Result<(), String> {
    // P1-8:项目级模型覆盖作为「未显式指定模型」时的回退(格式 provider_id::model_id)
    if input.model.trim().is_empty()
        && let Some(sid) = input.session_id.as_deref()
        && let Ok(Some((pid, model))) = crate::db::projects::model_override_for_session(sid)
    {
        input.provider_id = pid;
        input.model = model;
    }
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
    if let Some(rules) = crate::project_rules::inject(&mut messages, input.session_id.as_deref()) {
        // D7:留下「本轮注入了哪个约定文件」的痕迹
        super::super::event_log::record_rules(
            input.session_id.as_deref(),
            &rules.source,
            rules.content.chars().count(),
        );
    }
    // P1-8:项目共享上下文按请求实时构建 —— 先剥掉绑定时写入的旧快照,再拼当前项目 guard,
    // 这样编辑项目 shared_context 后,已绑定会话下一轮立即生效。
    if let Some(sid) = input.session_id.as_deref() {
        for m in messages.iter_mut().filter(|m| m.role == "system") {
            if m.content.contains(crate::db::projects::GUARD_MARKER) {
                m.content = crate::db::projects::strip_guard(&m.content);
            }
        }
        if let Some(guard) = crate::db::projects::guard_for_session(sid) {
            match messages.iter_mut().find(|m| m.role == "system") {
                Some(sys) => sys.content.push_str(&guard),
                None => messages.insert(
                    0,
                    crate::provider::ChatMessage {
                        role: "system".into(),
                        content: guard.trim_start().to_string(),
                        ..Default::default()
                    },
                ),
            }
        }
    }
    // P0:跨轮上下文治理(后端接管历史窗):
    //   1) 应用已落库的「前情摘要」,丢弃水位线之前的历史(不再每轮重复计费/重复摘要);
    //   2) 会话显式设置了 max_context_messages>0 时按最近 N 条裁剪;0 = 全部,仅按预算裁。
    if let Some(sid) = input.session_id.as_deref() {
        if let Ok(Some(c)) = crate::db::compaction::get(sid) {
            super::context::apply_stored_compaction(&mut messages, c.through_msg_id, &c.summary);
        }
        let max_msgs = crate::db::sessions::get(sid)
            .ok()
            .flatten()
            .map(|s| s.max_context_messages)
            .unwrap_or(0);
        if max_msgs > 0 {
            super::context::apply_session_window(&mut messages, max_msgs as usize);
        }
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
    // P7:完整三层估算(L1 官方 BPE × L2 历史校准 - L3 Segmented Cache 预估)
    super::super::event_log::record_request(
        input.session_id.as_deref(),
        &spec.provider_id,
        &req.model,
        &req.messages,
        req.tools.as_deref(),
        None, // prev_messages:Layer 3 cache 预估暂不传,后续从 event_log 取上轮指纹
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
    );
    Ok(())
}
