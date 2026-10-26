//! 流式翻译命令(OpenAI 兼容 + DeepL 直连)。

use serde::Deserialize;
use tauri::ipc::Channel;

use crate::provider::{ChatMessage, ChatRequest, StreamChunk, keychain, templates};

use super::assembly::{build_provider, validate_base_url_scheme};
use super::chat::spawn_chat_to_channel;
use super::stream::{StreamCancelState, run_cancellable};

// ---------- translate_stream ----------

#[derive(Debug, Deserialize)]
pub struct TranslateStreamInput {
    pub provider_id: String,
    pub model: String,
    pub text: String,
    /// 源语言;None/空 = 自动检测
    pub source_lang: Option<String>,
    pub target_lang: String,
    pub custom_base_url: Option<String>,
    pub custom_display_name: Option<String>,
    /// 前端生成的取消句柄;「停止翻译」时凭它调 cancel_stream(M-2)
    pub cancel_id: String,
}

/// 流式翻译:system prompt 约束只输出译文,chunk 通过 channel 推给前端。
/// - 术语表:启动时加载 data/glossary.json,命中词条注入 system prompt(迭代 D6)。
/// - DeepL:provider_id = "deepl" 时直连官方 API,不走 OpenAI 兼容层(迭代 D6.4)。
#[tauri::command]
pub async fn translate_stream(
    cancel_state: tauri::State<'_, StreamCancelState>,
    input: TranslateStreamInput,
    channel: Channel<StreamChunk>,
) -> Result<(), String> {
    if input.text.trim().is_empty() {
        return Err("待翻译文本不能为空".into());
    }
    let cancel_id = input.cancel_id.clone();
    // token 已存入注册表供 cancel_stream 查找;接收端交给后台任务
    let (_, cancel_rx) = cancel_state.register(&cancel_id);
    let cancels = (cancel_state.inner().clone(), cancel_id, cancel_rx);

    // DeepL 直连分支(模板 models 为空,不参与通用 OpenAI 兼容流程)
    if input.provider_id == "deepl" {
        return deepl_translate(input, channel, cancels).await;
    }

    let provider = match build_provider(
        &input.provider_id,
        &input.model,
        input.custom_base_url.as_deref(),
        input.custom_display_name.as_deref(),
    ) {
        Ok(p) => p,
        // 注册取消句柄后才可能走到这里:失败必须注销,避免注册表残留
        Err(e) => {
            cancels.0.remove(&cancels.1);
            return Err(e);
        }
    };

    // 基础 prompt + 术语表命中注入
    let mut system =
        crate::provider::translate_system_prompt(input.source_lang.as_deref(), &input.target_lang);
    if let Some(hints) = crate::commands::translate::glossary_hints(&input.text) {
        system.push_str(&hints);
    }

    let req = ChatRequest {
        model: input.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: system,
                images: vec![],
                ..Default::default()
            },
            ChatMessage {
                role: "user".into(),
                content: input.text,
                images: vec![],
                ..Default::default()
            },
        ],
        temperature: Some(0.2),
        max_tokens: None,
        top_p: None,
        reasoning: None, // 翻译走自有口径,不启用思考
        stream: true,
        ..Default::default()
    };

    spawn_chat_to_channel(provider, req, channel, cancels.0, cancels.1, cancels.2);
    Ok(())
}

/// 前端语言英文名 → DeepL API 语言码。
/// target 侧英语用 EN-US(DeepL 目标语言需指定变体);source 侧可用通用 EN。
fn deepl_lang(name: &str, is_target: bool) -> Option<&'static str> {
    match name.trim().to_ascii_lowercase().as_str() {
        "chinese" => Some("ZH"),
        "english" => Some(if is_target { "EN-US" } else { "EN" }),
        "japanese" => Some("JA"),
        "korean" => Some("KO"),
        "french" => Some("FR"),
        "german" => Some("DE"),
        "russian" => Some("RU"),
        "spanish" => Some("ES"),
        _ => None,
    }
}

/// DeepL 官方 API 直连翻译(POST {base}/translate)。
/// 结果非流式:一次性发 Delta + Done,与前端流式 UI 兼容。
async fn deepl_translate(
    input: TranslateStreamInput,
    channel: Channel<StreamChunk>,
    cancels: (
        StreamCancelState,
        String,
        futures::channel::oneshot::Receiver<()>,
    ),
) -> Result<(), String> {
    let (cancel_state, cancel_id, cancel_rx) = cancels;
    // 校验阶段任何提前失败都要注销取消句柄,避免注册表残留
    let bail = |state: &StreamCancelState, id: &str, e: String| -> Result<(), String> {
        state.remove(id);
        Err(e)
    };

    let api_key = match keychain::load("deepl") {
        Ok(Some(k)) => k,
        Ok(None) => {
            return bail(
                &cancel_state,
                &cancel_id,
                "DeepL 未配置 API Key(设置页 → 模型服务商)".into(),
            );
        }
        Err(e) => return bail(&cancel_state, &cancel_id, e),
    };

    let target = match deepl_lang(&input.target_lang, true) {
        Some(t) => t,
        None => {
            return bail(
                &cancel_state,
                &cancel_id,
                format!("DeepL 暂不支持目标语言: {}", input.target_lang),
            );
        }
    };
    let source = input
        .source_lang
        .as_deref()
        .and_then(|s| deepl_lang(s, false));

    let tpl = match templates::find_template("deepl") {
        Some(t) => t,
        None => return bail(&cancel_state, &cancel_id, "DeepL 模板缺失".into()),
    };
    if let Err(e) = validate_base_url_scheme(&tpl.base_url) {
        return bail(&cancel_state, &cancel_id, e);
    }
    let url = format!("{}/translate", tpl.base_url.trim_end_matches('/'));

    let mut body = serde_json::json!({
        "text": [input.text],
        "target_lang": target,
    });
    if let Some(src) = source {
        body["source_lang"] = serde_json::Value::String(src.to_string());
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => return bail(&cancel_state, &cancel_id, e.to_string()),
    };

    let work = async move {
        let result = client
            .post(&url)
            .header("Authorization", format!("DeepL-Auth-Key {api_key}"))
            .json(&body)
            .send()
            .await;

        match result {
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    let body_text = resp.text().await.unwrap_or_default();
                    let excerpt = body_text.chars().take(200).collect::<String>();
                    let _ = channel.send(StreamChunk::Error {
                        message: format!("DeepL 返回 HTTP {}: {}", status.as_u16(), excerpt),
                    });
                    return;
                }
                let parsed: Result<serde_json::Value, _> = resp.json().await;
                match parsed {
                    Ok(json) => {
                        let text = json
                            .get("translations")
                            .and_then(|t| t.as_array())
                            .and_then(|arr| arr.first())
                            .and_then(|t| t.get("text"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if text.is_empty() {
                            let _ = channel.send(StreamChunk::Error {
                                message: "DeepL 返回了空译文".into(),
                            });
                            return;
                        }
                        let _ = channel.send(StreamChunk::Delta { content: text });
                        let _ = channel.send(StreamChunk::Done {
                            finish_reason: Some("stop".into()),
                            prompt_tokens: None,
                            completion_tokens: None,
                            reasoning_tokens: None,
                            cached_tokens: None,
                        });
                    }
                    Err(e) => {
                        let _ = channel.send(StreamChunk::Error {
                            message: format!("解析 DeepL 响应失败: {e}"),
                        });
                    }
                }
            }
            Err(e) => {
                let _ = channel.send(StreamChunk::Error {
                    message: format!("请求 DeepL 失败: {e}"),
                });
            }
        }
    };
    tauri::async_runtime::spawn(run_cancellable(cancel_state, cancel_id, cancel_rx, work));

    Ok(())
}
