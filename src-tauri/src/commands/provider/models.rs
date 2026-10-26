//! 模型卡 / 厂家模型列表 / 单模型配置 / 思考能力相关命令。

use crate::provider::model_cards::{load_model_cache, save_model_cache};
use crate::provider::{
    ProviderError, model_cards,
    model_config::{self, ProviderModelConfig},
    openai::OpenAiCompat,
    reasoning,
    rotation::{self, RotationState},
    templates,
};

use super::assembly::validate_base_url_scheme;
use super::failover::with_key_rotation;
use super::keys::{own_candidates, rotation_group};
use super::templates::notify_providers_changed;

// ---------- 单模型配置(迭代 D8)----------

/// 读取单模型配置(provider_id -> model_id -> 覆盖项)。
#[tauri::command]
pub fn get_provider_model_config() -> ProviderModelConfig {
    model_config::load()
}

/// 保存单模型配置(整体覆盖写)。
#[tauri::command]
pub fn save_provider_model_config(
    app: tauri::AppHandle,
    config: ProviderModelConfig,
) -> Result<(), String> {
    model_config::save(&config)?;
    notify_providers_changed(&app);
    Ok(())
}

// ---------- 模型卡(迭代 D7.1)----------

/// 列出全部模型卡:内置 13 张 + data/model_cards.json 用户覆盖/追加。
#[tauri::command]
pub fn list_model_cards() -> Vec<crate::provider::model_cards::ModelCard> {
    crate::provider::model_cards::list_model_cards()
}

// ---------- 厂家模型拉取(迭代 D2;元数据+价格补全)----------

/// 手动新增/覆盖一张模型卡(价格/上下文等元数据),写入 data/model_cards.json。
#[tauri::command]
pub fn save_model_card(card: model_cards::ModelCard) -> Result<(), String> {
    model_cards::save_user_card(&card)
}

/// 从厂家拉取模型列表(OpenAI 兼容 `GET {base_url}/models`)。
/// 除 id 外同时收厂家自报的名称/上下文/输出上限/模态/价格,缓存到
/// data/provider_models.json(派生模型卡时再用内置刊例目录补全缺口)。
/// 多 Key 时按轮换顺序尝试,遇 429/5xx 冷却该 Key 并换下一把。
/// 返回值仍为 id 列表(供前端提示数量与刷新模板)。
#[tauri::command]
pub async fn fetch_provider_models(
    app: tauri::AppHandle,
    state: tauri::State<'_, RotationState>,
    provider_id: String,
) -> Result<Vec<String>, String> {
    let base_url = templates::find_template(&provider_id)
        .ok_or_else(|| format!("未知 provider: {}", provider_id))?
        .base_url;
    if base_url.trim().is_empty() {
        return Err(format!("provider {} 未配置 base_url", provider_id));
    }
    validate_base_url_scheme(&base_url)?;
    let url = format!("{}/models", base_url.trim_end_matches('/'));

    let cfg = rotation::config_for(&provider_id);
    let group = rotation_group(&provider_id, &base_url);
    let cands = own_candidates(&provider_id, &cfg);
    if cands.is_empty() {
        return Err(format!("provider {} 未配置 API Key", provider_id));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;

    let fetched = with_key_rotation(&group, cands, &state, &cfg, |_cand, api_key| {
        let client = client.clone();
        let url = url.clone();
        async move {
            let resp = client
                .get(&url)
                .bearer_auth(&api_key)
                .send()
                .await
                .map_err(|e| ProviderError::Network(format!("请求 {} 失败: {e}", url)))?;
            if !resp.status().is_success() {
                return Err(OpenAiCompat::map_http_error(resp).await);
            }
            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| ProviderError::Parse(format!("解析响应失败: {e}")))?;
            let models = model_cards::parse_fetched_models(&json);
            if models.is_empty() {
                return Err(ProviderError::Other("厂家返回的模型列表为空".into()));
            }
            Ok(models)
        }
    })
    .await?;

    let ids = fetched.iter().map(|m| m.id.clone()).collect();
    let mut cache = model_cards::load_model_cache();
    cache.insert(provider_id, fetched);
    model_cards::save_model_cache(&cache)?;
    notify_providers_changed(&app);
    Ok(ids)
}

// ---------- 思考能力(v1.9.0 工作块 I) ----------

/// 查询某模型的思考能力(手动修正 > 内置规则表推断 > 不支持)。
/// 前端据此决定是否渲染思考控件、是否允许关闭/选择档位。
#[tauri::command]
pub fn get_reasoning_capability(
    provider_id: String,
    model: String,
) -> reasoning::ReasoningCapability {
    reasoning::resolve(&provider_id, &model)
}

/// 读取某模型的手动思考能力修正原始值(未修正返回 None)。
/// 与 get_reasoning_capability 的「解析后能力」区分:设置页据此展示「自动推断」还是「手动修正」。
#[tauri::command]
pub fn get_reasoning_override(
    provider_id: String,
    model: String,
) -> Option<reasoning::ReasoningCapability> {
    model_cards::load_model_cache()
        .get(&provider_id)
        .and_then(|list| list.iter().find(|m| m.id == model))
        .and_then(|m| m.reasoning)
}

/// 手动修正模型思考能力(写 provider_models.json 缓存条目的 reasoning 字段)。
/// cap = None 表示清除修正,回到内置推断。
#[tauri::command]
pub fn save_reasoning_override(
    provider_id: String,
    model: String,
    cap: Option<reasoning::ReasoningCapability>,
) -> Result<(), String> {
    let mut cache = load_model_cache();
    let entry = cache
        .entry(provider_id.clone())
        .or_default()
        .iter_mut()
        .find(|m| m.id == model);
    match (entry, cap) {
        (Some(m), Some(c)) => m.reasoning = Some(c),
        (Some(m), None) => m.reasoning = None,
        // 模型不在缓存中(如自定义模板未拉取过列表):补一个最小条目
        (None, Some(c)) => {
            cache
                .entry(provider_id)
                .or_default()
                .push(model_cards::FetchedModel {
                    id: model,
                    name: None,
                    context_window: None,
                    max_output: None,
                    input_modalities: Vec::new(),
                    output_modalities: Vec::new(),
                    pricing: None,
                    reasoning: Some(c),
                });
        }
        (None, None) => {}
    }
    save_model_cache(&cache)
}
