//! provider 相关 Tauri commands。

use std::time::Instant;

use serde::Deserialize;
use tauri::ipc::Channel;

use crate::db::{
    self,
    usage::{self, UsageEntry, UsageGroup, UsageHour, UsageRecord, UsageSummary},
};
use crate::provider::{
    keychain,
    model_cards,
    model_config::{self, ProviderModelConfig},
    openai::OpenAiCompat,
    reasoning,
    rotation::{self, KeyCandidate, KeySlotMeta, RotationConfig, RotationState},
    templates::{self, ProviderTemplate},
    ChatMessage, ChatRequest, EffortLevel, Provider, ProviderError, ReasoningSpec, StreamChunk,
};

// ---------- 流式请求取消(M-2)----------

/// 单个流式请求的取消信号:oneshot 单发,run_cancellable 的 select 另一端在等。
pub(crate) struct StreamCancel {
    tx: std::sync::Mutex<Option<futures::channel::oneshot::Sender<()>>>,
}

impl StreamCancel {
    fn fire(&self) {
        if let Ok(mut g) = self.tx.lock() {
            if let Some(tx) = g.take() {
                let _ = tx.send(());
            }
        }
    }
}

/// 进程级流式取消注册表:cancel_id(前端生成) → 取消信号,任务结束自行注销。
#[derive(Clone, Default)]
pub struct StreamCancelState {
    map: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<StreamCancel>>>,
    >,
}

impl StreamCancelState {
    /// 注册并返回(信号句柄, 取消接收端)。接收端交给 run_cancellable 等待。
    fn register(
        &self,
        id: &str,
    ) -> (
        std::sync::Arc<StreamCancel>,
        futures::channel::oneshot::Receiver<()>,
    ) {
        let (tx, rx) = futures::channel::oneshot::channel();
        let token = std::sync::Arc::new(StreamCancel {
            tx: std::sync::Mutex::new(Some(tx)),
        });
        match self.map.lock() {
            Ok(mut m) => {
                // N-3:同 id 重复注册时先取消旧任务,避免旧任务永远无法被取消
                if let Some(old) = m.insert(id.to_string(), token.clone()) {
                    old.fire();
                }
            }
            // N-3:Mutex 中毒不 panic( unwrap 会让后续所有流式命令崩溃);
            // 退化为本次不可取消,流本身仍可用
            Err(e) => log::warn!("stream cancel registry poisoned: {e}"),
        }
        (token, rx)
    }

    fn remove(&self, id: &str) {
        if let Ok(mut m) = self.map.lock() {
            m.remove(id);
        }
    }
}

/// 前端「停止生成/停止翻译」入口:触发取消信号。被 select 掉的工作 future
/// drop 时,内部 reqwest 响应流随之 drop、HTTP 连接中止,后端立即停止烧 token。
#[tauri::command]
pub fn cancel_stream(state: tauri::State<'_, StreamCancelState>, cancel_id: String) {
    let token = state
        .map
        .lock()
        .ok()
        .and_then(|m| m.get(&cancel_id).cloned());
    if let Some(t) = token {
        t.fire();
    }
}

/// 跑到完成或收到取消;无论结果如何都注销 cancel_id。返回 None 表示被取消。
/// 注意:work 整个 future 被 drop 即级联中止其内部的 HTTP 请求(连接关闭)。
async fn run_cancellable<T>(
    state: StreamCancelState,
    cancel_id: String,
    cancel_rx: futures::channel::oneshot::Receiver<()>,
    work: impl std::future::Future<Output = T>,
) -> Option<T> {
    use futures::FutureExt;
    let work = work.fuse();
    let cancel = cancel_rx.map(|_| ()).fuse();
    futures::pin_mut!(work);
    futures::pin_mut!(cancel);
    let result = futures::select! {
        v = work => Some(v),
        _ = cancel => None,
    };
    state.remove(&cancel_id);
    result
}


/// 列出 Provider 模板(不含用户已填的 api_key)。以用户持久化列表为准(可增删改)。
/// Q2=B:某 Provider 一旦成功拉取并缓存模型列表(data/provider_models.json),
/// 其 models 以厂家返回为唯一来源(预置列表作废)。
/// 最后统一应用单模型配置(启用过滤 + 排序覆盖)。
#[tauri::command]
pub fn list_provider_templates() -> Vec<ProviderTemplate> {
    let mut tpls = templates::load_all();
    let cache = load_model_cache();
    let model_cfg = model_config::load();
    for t in &mut tpls {
        // Q2=B:该 provider 一旦成功拉取并缓存,模型列表以厂家返回为唯一来源
        let base = match cache.get(&t.id) {
            Some(f) if !f.is_empty() => f.iter().map(|m| m.id.clone()).collect(),
            _ => std::mem::take(&mut t.models),
        };
        let all = model_config::sort_models(base, &model_cfg, &t.id);
        // models = 已启用(供所有模型选择器使用);all_models = 全量(仅设置页展示/开关)
        t.models = model_config::filter_enabled(all.clone(), &model_cfg, &t.id);
        t.all_models = all;
    }
    tpls
}

// ---------- 模板增删改(用户拥有完全权限)----------

/// 新增或更新一个 Provider 模板(id 为空时自动生成),返回落盘后的模板。
#[tauri::command]
pub fn save_provider_template(
    app: tauri::AppHandle,
    mut template: ProviderTemplate,
) -> Result<ProviderTemplate, String> {
    template.display_name = template.display_name.trim().to_string();
    if template.display_name.is_empty() {
        return Err("显示名不能为空".into());
    }
    template.base_url = template.base_url.trim().trim_end_matches('/').to_string();
    validate_base_url_scheme(&template.base_url)?;
    let mut seen = std::collections::HashSet::new();
    template.models = template
        .models
        .iter()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty() && seen.insert(m.clone()))
        .collect();
    // all_models 为派生字段,不落盘
    template.all_models = vec![];
    if template.id.trim().is_empty() {
        template.id = format!("u{}", chrono::Utc::now().timestamp_millis());
    }

    let mut list = templates::load_all();
    match list.iter_mut().find(|t| t.id == template.id) {
        Some(slot) => *slot = template.clone(),
        None => list.push(template.clone()),
    }
    templates::save_all(&list)?;
    notify_providers_changed(&app);
    Ok(template)
}

/// 删除一个 Provider 模板(内置/用户新增一视同仁),并清理其 Key、单模型配置与厂家模型缓存。
#[tauri::command]
pub fn delete_provider_template(
    app: tauri::AppHandle,
    state: tauri::State<'_, RotationState>,
    provider_id: String,
) -> Result<(), String> {
    let mut list = templates::load_all();
    // 编组命名空间基于 base_url,故删除前先记下,以便清理该组的进程内冷却状态
    let group = list
        .iter()
        .find(|t| t.id == provider_id)
        .map(|t| rotation_group(&provider_id, &t.base_url))
        .unwrap_or_else(|| provider_id.clone());
    list.retain(|t| t.id != provider_id);
    templates::save_all(&list)?;

    let _ = keychain::remove(&provider_id);
    let mut cfg = model_config::load();
    if cfg.remove(&provider_id).is_some() {
        model_config::save(&cfg)?;
    }
    let mut cache = load_model_cache();
    if cache.remove(&provider_id).is_some() {
        save_model_cache(&cache)?;
    }
    // 删除模板时同步清理该 Provider 的轮换配置(多 Key 槽位随 keychain::remove 一并删除)
    let mut rot = rotation::load_configs();
    if rot.remove(&provider_id).is_some() {
        rotation::save_configs(&rot)?;
    }
    // 清空进程内冷却状态,避免残留的限流冷却影响后续同名 Provider
    state.clear(&group);
    notify_providers_changed(&app);
    Ok(())
}

/// 补回被删除的内置模板(仅补缺失项,不改动用户已编辑/新增的模板)。
#[tauri::command]
pub fn restore_builtin_templates(app: tauri::AppHandle) -> Result<(), String> {
    let mut list = templates::load_all();
    for b in templates::builtin_templates() {
        if !list.iter().any(|t| t.id == b.id) {
            list.push(b);
        }
    }
    templates::save_all(&list)?;
    notify_providers_changed(&app);
    Ok(())
}

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

pub use model_cards::{load_model_cache, save_model_cache};

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

/// 密钥 / 模板 / 单模型配置变更后广播,各窗口据此刷新全局 providers store。
/// 修复「设置页保存 Key 后同生命周期内对话页仍显示未配置」的状态不同步问题(H-1)。
fn notify_providers_changed(app: &tauri::AppHandle) {
    use tauri::Emitter;
    let _ = app.emit("providers_changed", ());
}

/// 保存某 Provider 的 API Key(加密落盘)。
#[tauri::command]
pub fn save_api_key(
    app: tauri::AppHandle,
    provider_id: String,
    api_key: String,
) -> Result<(), String> {
    keychain::store(&provider_id, &api_key)?;
    notify_providers_changed(&app);
    Ok(())
}

/// 删除某 Provider 的 API Key。
#[tauri::command]
pub fn delete_api_key(app: tauri::AppHandle, provider_id: String) -> Result<(), String> {
    keychain::remove(&provider_id)?;
    notify_providers_changed(&app);
    Ok(())
}

/// 列出已存 API Key 的 provider_id 列表。
#[tauri::command]
pub fn list_stored_provider_ids() -> Result<Vec<String>, String> {
    keychain::list_stored()
}

/// 检测某 Provider 是否已配 Key(多槽位:任一槽位有 Key 即为已配置)。
#[tauri::command]
pub fn has_api_key(provider_id: String) -> Result<bool, String> {
    Ok(!keychain::list_slots(&provider_id)?.is_empty())
}

// ---------- API 流转:多 Key 槽位 ----------

/// 前端可见的槽位视图(不含明文 Key,仅掩码)。
#[derive(Debug, serde::Serialize)]
pub struct KeySlotView {
    pub slot: u32,
    pub label: String,
    pub enabled: bool,
    /// 权重(>=1)
    pub weight: u32,
    /// 优先级(越大越先使用)
    pub priority: u32,
    /// 掩码(如 sk-a****1234);仅用于展示
    pub masked: Option<String>,
    /// 剩余冷却秒数(0 表示可用)
    pub cooldown_remaining: u64,
}

/// 列出某 Provider 的全部 Key 槽位(含标签 / 启用状态 / 掩码 / 冷却剩余)。
#[tauri::command]
pub fn list_provider_keys(
    state: tauri::State<'_, RotationState>,
    provider_id: String,
) -> Result<Vec<KeySlotView>, String> {
    crate::memory::validate_id(&provider_id)?;
    let cfg = rotation::config_for(&provider_id);
    // 冷却表按编组命名空间存放(同 base_url 组共享),查询时须用同一个 group
    let group = rotation_group(
        &provider_id,
        &templates::find_template(&provider_id)
            .map(|t| t.base_url)
            .unwrap_or_default(),
    );
    let slots = keychain::list_slots(&provider_id)?;
    let out = slots
        .iter()
        .map(|slot| {
            let meta = cfg.keys.iter().find(|k| k.slot == *slot);
            let masked = keychain::load_slot(&provider_id, *slot)
                .ok()
                .flatten()
                .map(|k| usage::mask_key(&k));
            KeySlotView {
                slot: *slot,
                label: meta.map(|m| m.label.clone()).unwrap_or_default(),
                enabled: meta.map(|m| m.enabled).unwrap_or(true),
                weight: meta.map(|m| m.weight).unwrap_or(1).clamp(1, 50),
                priority: meta.map(|m| m.priority).unwrap_or(0),
                masked,
                cooldown_remaining: state.cooldown_remaining(&group, &provider_id, *slot),
            }
        })
        .collect();
    Ok(out)
}

/// 新增 / 覆盖某槽位的 API Key。slot 为 None 时自动分配到下一个空槽位。
/// 返回实际写入的槽位号。
#[tauri::command]
pub fn save_provider_key(
    app: tauri::AppHandle,
    provider_id: String,
    slot: Option<u32>,
    api_key: String,
    label: Option<String>,
) -> Result<u32, String> {
    crate::memory::validate_id(&provider_id)?;
    let key = api_key.trim();
    if key.is_empty() {
        return Err("API Key 不能为空".into());
    }
    let mut map = rotation::load_configs();
    let mut cfg = map.remove(&provider_id).unwrap_or_default();
    let existing = keychain::list_slots(&provider_id)?;
    let s = match slot {
        Some(s) => s,
        None => existing.iter().max().map(|m| m + 1).unwrap_or(0),
    };
    keychain::store_slot(&provider_id, s, key)?;
    let label = label.unwrap_or_default().trim().to_string();
    match cfg.keys.iter_mut().find(|k| k.slot == s) {
        Some(k) => {
            if !label.is_empty() {
                k.label = label;
            }
            k.enabled = true;
        }
        None => cfg.keys.push(KeySlotMeta {
            slot: s,
            label,
            enabled: true,
            weight: 1,
            priority: 0,
        }),
    }
    map.insert(provider_id, cfg);
    rotation::save_configs(&map)?;
    notify_providers_changed(&app);
    Ok(s)
}

/// 更新槽位元数据(标签 / 启用开关 / 权重 / 优先级)。Key 本身不变。
#[tauri::command]
pub fn update_key_slot(
    provider_id: String,
    slot: u32,
    label: String,
    enabled: bool,
    weight: Option<u32>,
    priority: Option<u32>,
) -> Result<(), String> {
    crate::memory::validate_id(&provider_id)?;
    let mut map = rotation::load_configs();
    let mut cfg = map.remove(&provider_id).unwrap_or_default();
    match cfg.keys.iter_mut().find(|k| k.slot == slot) {
        Some(k) => {
            k.label = label.trim().to_string();
            k.enabled = enabled;
            if let Some(w) = weight {
                k.weight = w.clamp(1, 50);
            }
            if let Some(p) = priority {
                k.priority = p;
            }
        }
        None => cfg.keys.push(KeySlotMeta {
            slot,
            label: label.trim().to_string(),
            enabled,
            weight: weight.unwrap_or(1).clamp(1, 50),
            priority: priority.unwrap_or(0),
        }),
    }
    map.insert(provider_id, cfg);
    rotation::save_configs(&map)
}

/// 删除某槽位的 Key 与元数据。
#[tauri::command]
pub fn delete_provider_key(
    app: tauri::AppHandle,
    state: tauri::State<'_, RotationState>,
    provider_id: String,
    slot: u32,
) -> Result<(), String> {
    crate::memory::validate_id(&provider_id)?;
    keychain::delete_slot(&provider_id, slot)?;
    let mut map = rotation::load_configs();
    if let Some(mut cfg) = map.remove(&provider_id) {
        cfg.keys.retain(|k| k.slot != slot);
        map.insert(provider_id.clone(), cfg);
        rotation::save_configs(&map)?;
    }
    state.clear_cooldown(
        &rotation_group(
            &provider_id,
            &templates::find_template(&provider_id)
                .map(|t| t.base_url)
                .unwrap_or_default(),
        ),
        &provider_id,
        slot,
    );
    notify_providers_changed(&app);
    Ok(())
}

/// 读取全部 Provider 的轮换配置(不含任何密钥,可安全下发前端)。
#[tauri::command]
pub fn get_rotation_configs() -> Result<std::collections::HashMap<String, RotationConfig>, String> {
    Ok(rotation::load_configs())
}

/// 保存某 Provider 的轮换开关 / 冷却秒数 / stream_options 支持开关。
/// 槽位元数据(keys)保持不变,避免覆盖标签与顺序。
#[tauri::command]
pub fn set_provider_rotation(
    provider_id: String,
    enabled: bool,
    cooldown_secs: u64,
    supports_stream_options: bool,
) -> Result<(), String> {
    crate::memory::validate_id(&provider_id)?;
    let mut map = rotation::load_configs();
    let mut cfg = map.remove(&provider_id).unwrap_or_default();
    cfg.enabled = enabled;
    cfg.cooldown_secs = cooldown_secs.clamp(1, 3600);
    cfg.supports_stream_options = supports_stream_options;
    map.insert(provider_id, cfg);
    rotation::save_configs(&map)
}

// ---------- 用量统计 ----------

/// 按维度聚合用量(dimension: model | provider | key | agent;days: 统计窗口天数)。
#[tauri::command]
pub fn usage_summary(dimension: String, days: u32) -> Result<UsageSummary, String> {
    let dim = match dimension.as_str() {
        "provider" | "key" | "agent" => dimension.as_str(),
        _ => "model",
    };
    usage::summary(dim, days)
}

/// 最近请求明细(倒序,最多 500 条)。
#[tauri::command]
pub fn usage_entries(limit: u32) -> Result<Vec<UsageEntry>, String> {
    usage::entries(limit)
}

/// 某 Provider 各槽位的健康度(成功 / 限流 / 失败 / token)。
#[tauri::command]
pub fn usage_key_health(provider_id: String, days: u32) -> Result<Vec<UsageGroup>, String> {
    crate::memory::validate_id(&provider_id)?;
    usage::key_health(&provider_id, days)
}

/// 热力图数据:近 days 天内按「星期 × 小时」分桶的请求数 / Token 数。
#[tauri::command]
pub fn usage_hourly(days: u32) -> Result<Vec<UsageHour>, String> {
    usage::hourly(days)
}

// ---------- generate_image ----------

use crate::provider::{ImageRequest, ImageResponse};
use base64::Engine as _;

#[derive(Debug, Deserialize)]
pub struct GenerateImageInput {
    pub provider_id: String,
    pub model: String,
    pub prompt: String,
    /// 负面提示(合并进 prompt:`. Avoid: ...`)
    pub negative: Option<String>,
    /// 风格词(由前端风格 chip 给出英文关键词,合并进 prompt)
    pub style_prompt: Option<String>,
    /// 风格标签名(仅记录 meta)
    pub style: Option<String>,
    /// 预留:种子(当前内置 Provider 均不支持,仅记录 meta)
    pub seed: Option<i64>,
    /// 预留:Guidance(同上)
    pub guidance: Option<f32>,
    pub size: Option<String>,
    pub n: Option<u32>,
    pub custom_base_url: Option<String>,
    pub custom_display_name: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct ImageResultItem {
    /// 落盘绝对路径(项目 data/images/ 下)
    pub path: String,
    /// base64 data URI,前端直接渲染(未启用 asset protocol,避免路径协议配置)
    pub data_url: String,
    pub revised_prompt: Option<String>,
}

/// 图片落盘目录:`<data_dir>/images`
fn images_dir() -> std::path::PathBuf {
    crate::paths::data_dir().join("images")
}

/// 生成参数留档(D5.4):与 png 同名同目录的 meta.json。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageMeta {
    pub prompt: String,
    pub negative: Option<String>,
    pub style: Option<String>,
    pub size: String,
    pub model: String,
    pub provider_id: String,
    pub seed: Option<i64>,
    pub guidance: Option<f32>,
    pub created: String,
}

/// 生成图像。n 张图逐张调用 Provider(单张生成,职责单一),全部落盘后返回。
/// Phase 3:多 Key 时按轮换顺序尝试,遇 429/5xx 冷却该 Key 并换下一把(每张图独立选 Key)。
/// 兼容 b64_json(解码写入)与 url(下载后写入)两种 Provider 返回。
#[tauri::command]
pub async fn generate_image(
    state: tauri::State<'_, RotationState>,
    input: GenerateImageInput,
) -> Result<Vec<ImageResultItem>, String> {
    let spec = resolve_spec(
        &input.provider_id,
        &input.model,
        input.custom_base_url.as_deref(),
        input.custom_display_name.as_deref(),
    )?;

    let cfg = rotation::config_for(&input.provider_id);
    let group = rotation_group(&input.provider_id, &spec.base_url);
    let cands = own_candidates(&input.provider_id, &cfg);
    if cands.is_empty() {
        return Err(format!("provider {} 未配置 API Key", input.provider_id));
    }

    let n = input.n.unwrap_or(1).clamp(1, 4);
    let size = input.size.clone().unwrap_or_else(|| "1024x1024".into());

    // 风格词与负面提示合并进 prompt(对所有 OpenAI 兼容 Provider 生效)
    let mut final_prompt = input.prompt.clone();
    if let Some(sp) = input.style_prompt.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        final_prompt = format!("{final_prompt}, {sp}");
    }
    if let Some(neg) = input.negative.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        final_prompt = format!("{final_prompt}. Avoid: {neg}");
    }

    let meta = ImageMeta {
        prompt: input.prompt.clone(),
        negative: input.negative.clone(),
        style: input.style.clone(),
        size: size.clone(),
        model: input.model.clone(),
        provider_id: input.provider_id.clone(),
        seed: input.seed,
        guidance: input.guidance,
        created: chrono::Local::now().to_rfc3339(),
    };

    let mut results = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let resp = with_key_rotation(&group, cands.clone(), &state, &cfg, |_cand, key| {
            let provider = build_openai(&spec, &key, false);
            let req = ImageRequest {
                model: input.model.clone(),
                prompt: final_prompt.clone(),
                size: Some(size.clone()),
                n: Some(1),
                style: None,
            };
            async move { provider.generate_image(req).await }
        })
        .await?;

        results.push(save_image_response(resp, Some(&meta)).await?);
    }
    Ok(results)
}

/// 把 ImageResponse 落盘到 data/images/,同时写同名 meta.json;返回路径 + data URI。
async fn save_image_response(
    resp: ImageResponse,
    meta: Option<&ImageMeta>,
) -> Result<ImageResultItem, String> {
    let dir = images_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 images 目录失败: {e}"))?;

    // 统一拿到图片字节
    let bytes: Vec<u8> = if let Some(b64) = resp.url.strip_prefix("data:image/png;base64,") {
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("base64 解码失败: {e}"))?
    } else if resp.url.starts_with("http://") || resp.url.starts_with("https://") {
        download_image_checked(&resp.url).await?
    } else {
        return Err(format!("未知的图片返回格式: {}", &resp.url[..resp.url.len().min(64)]));
    };

    // 毫秒时间戳,避免同秒多图撞名
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S%3f");
    let path = dir.join(format!("{ts}.png"));
    std::fs::write(&path, &bytes).map_err(|e| format!("写入图片失败: {e}"))?;

    // 参数留档 meta.json(失败不影响出图)
    if let Some(m) = meta {
        if let Ok(json) = serde_json::to_string_pretty(m) {
            let _ = std::fs::write(dir.join(format!("{ts}.meta.json")), json);
        }
    }

    let data_url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    );
    Ok(ImageResultItem {
        path: path.to_string_lossy().to_string(),
        data_url,
        revised_prompt: resp.revised_prompt,
    })
}

// ---------- 图片 URL 下载安全校验(M2,SSRF 防护) ----------

/// 远程图片大小上限:20MB。
const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

/// 校验 IP 是否为公网可达地址(白名单制)。
/// 覆盖回环/私网/链路本地/ULA/未指定/保留/文档/CGNAT/组播等全部非公网段;
/// IPv4-mapped/compatible IPv6 还原为 v4 再判,防止映射形式绕过(CWE-918,M-8)。
/// 不使用 std 的 is_global(在当前工具链对 IpAddr 仍为 unstable)。
fn assert_ip_allowed(ip: std::net::IpAddr) -> Result<(), String> {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let o = v4.octets();
            let non_public = v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                // 保留段 240.0.0.0/4(is_reserved 在当前工具链为 unstable,手动判定)
                || o[0] >= 240
                // 0.0.0.0/8「本网段」保留地址(也兜住 IPv4-compatible ::a.b.c.d 还原后绕过)
                || o[0] == 0
                // CGNAT 100.64.0.0/10(is_shared 在部分工具链版本不可用,手动判定)
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                // 基准测试 198.18.0.0/15
                || (o[0] == 198 && (18..=19).contains(&o[1]));
            if non_public {
                return Err(format!("图片 URL 指向非公网地址 {v4},已拒绝"));
            }
        }
        std::net::IpAddr::V6(v6) => {
            // 注意顺序:::1 / :: 也会被 to_ipv4() 解释成 0.0.0.1 / 0.0.0.0,
            // 必须先挡回环与未指定,再做内嵌 v4 还原。
            if v6.is_loopback() || v6.is_unspecified() {
                return Err(format!("图片 URL 指向非公网地址 {v6},已拒绝"));
            }
            // ::ffff:a.b.c.d(mapped)与 ::a.b.c.d(compatible)按内嵌 v4 规则判定
            if let Some(v4) = v6.to_ipv4() {
                return assert_ip_allowed(std::net::IpAddr::V4(v4));
            }
            let s = v6.segments();
            let non_public = (s[0] & 0xffc0 == 0xfe80) // 链路本地 fe80::/10
                || (s[0] & 0xfe00 == 0xfc00) // 唯一本地 ULA fc00::/7
                || s[0] == 0xff00 // 组播 ff00::/8
                || (s[0] == 0x2001 && s[1] == 0x0db8) // 文档 2001:db8::/32
                || s[0] == 0x0100; // 黑洞 100::/64(覆盖 100::/64 等丢弃前缀)
            if non_public {
                return Err(format!("图片 URL 指向非公网地址 {v6},已拒绝"));
            }
        }
    }
    Ok(())
}

/// 校验图片 URL:仅 http/https;host 为 IP 字面量时要求是公网地址,
/// 防止生成结果里的 URL 被用作内网探测跳板(CWE-918)。
/// 域名形式的解析后校验在下载前由 `assert_domain_resolves_public` 完成。
fn validate_image_url(url: &str) -> Result<(), String> {
    let u = reqwest::Url::parse(url).map_err(|e| format!("图片 URL 无法解析: {e}"))?;
    match u.scheme() {
        "http" | "https" => {}
        s => return Err(format!("图片 URL 协议不支持: {s}")),
    }
    let host = u.host_str().ok_or_else(|| "图片 URL 缺少 host".to_string())?;
    // url crate 对 IPv6 字面量返回带方括号形式([::1]),统一去括号后再判定
    let h = host.trim_start_matches('[').trim_end_matches(']');
    if h.eq_ignore_ascii_case("localhost") {
        return Err("图片 URL 指向本机回环,已拒绝".into());
    }
    if let Ok(ip) = h.parse::<std::net::IpAddr>() {
        assert_ip_allowed(ip)?;
    }
    Ok(())
}

/// 域名形式的 host:DNS 解析后逐一校验所有结果地址均为公网(M-8)。
/// 若域名同时返回公网与内网 A 记录(如 DNS rebinding 预备态),只要有一个内网地址即拒绝。
/// 注:校验与 reqwest 实际连接之间仍存在极小的解析-连接 TOCTOU 窗口,
/// 彻底消除需自定义连接器做 IP pinning;当前在可用性与安全间取平衡。
async fn assert_domain_resolves_public(url: &str) -> Result<(), String> {
    let u = reqwest::Url::parse(url).map_err(|e| format!("图片 URL 无法解析: {e}"))?;
    let host = u.host_str().ok_or_else(|| "图片 URL 缺少 host".to_string())?;
    // 去 IPv6 方括号;能 parse 成 IP 的已由 validate_image_url 同步校验,此处只处理域名
    let h = host.trim_start_matches('[').trim_end_matches(']');
    if h.parse::<std::net::IpAddr>().is_ok() || h.eq_ignore_ascii_case("localhost") {
        return Ok(());
    }
    let port = u.port_or_known_default().unwrap_or(443);
    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((h, port))
        .await
        .map_err(|e| format!("图片域名解析失败 {h}: {e}"))?
        .collect();
    if addrs.is_empty() {
        return Err(format!("图片域名 {h} 无可用解析地址"));
    }
    for sa in &addrs {
        assert_ip_allowed(sa.ip())?;
    }
    Ok(())
}

/// 带校验的图片下载:禁用自动重定向后手动逐跳校验(防 SSRF),再验 Content-Type,全程 20MB 上限。
pub(crate) async fn download_image_checked(url: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("构造图片下载客户端失败: {e}"))?;

    let mut current = url.to_string();
    // 最多跟随 5 跳重定向,每跳都过 SSRF 校验
    for _ in 0..5 {
        validate_image_url(&current)?;
        assert_domain_resolves_public(&current).await?;
        let mut resp = client
            .get(&current)
            .send()
            .await
            .map_err(|e| format!("下载图片失败: {e}"))?;

        // 3xx 重定向:取 Location 手动跟随
        if resp.status().is_redirection() {
            if let Some(loc) = resp.headers().get(reqwest::header::LOCATION) {
                let loc_str = loc.to_str().unwrap_or("").to_string();
                let new_url = if loc_str.starts_with("http://") || loc_str.starts_with("https://") {
                    loc_str
                } else {
                    let base = reqwest::Url::parse(&current)
                        .map_err(|e| format!("重定向基准解析失败: {e}"))?;
                    base.join(&loc_str)
                        .map_err(|e| format!("重定向相对路径拼接失败: {e}"))?
                        .to_string()
                };
                current = new_url;
                continue;
            }
            return Err("重定向响应缺少 Location,已拒绝".into());
        }

        // 最终响应:校验 Content-Type + 尺寸上限
        let ct = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_lowercase();
        if !ct.starts_with("image/") {
            return Err(format!("图片响应 Content-Type 非法: {ct}"));
        }
        if let Some(len) = resp.content_length() {
            if len as usize > MAX_IMAGE_BYTES {
                return Err(format!("图片超过 20MB 上限({len} 字节)"));
            }
        }

        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| format!("读取图片 body 失败: {e}"))?
        {
            if buf.len() + chunk.len() > MAX_IMAGE_BYTES {
                return Err("图片超过 20MB 上限".into());
            }
            buf.extend_from_slice(&chunk);
        }
        return Ok(buf);
    }
    Err("图片下载重定向超过 5 跳,已拒绝".into())
}

// ---------- 绘画历史(D5.4) ----------
/// 历史列表条目:缩略图 data URL + 参数 meta。
#[derive(Debug, serde::Serialize)]
pub struct ImageListItem {
    pub path: String,
    /// 256px JPEG 缩略图 data URL
    pub thumb: String,
    pub meta: Option<ImageMeta>,
}

/// 校验路径必须位于 images 目录内(防越权读盘)。
fn ensure_in_images_dir(p: &std::path::Path) -> Result<(), String> {
    let dir = images_dir().canonicalize().map_err(|e| e.to_string())?;
    let canon = p.canonicalize().map_err(|e| format!("路径无效: {e}"))?;
    if canon.starts_with(&dir) {
        Ok(())
    } else {
        Err("路径不在 images 目录内".into())
    }
}

/// 列出全部历史作品(按文件名时间戳倒序),附缩略图与 meta。
#[tauri::command]
pub fn list_images() -> Result<Vec<ImageListItem>, String> {
    let dir = images_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names: Vec<String> = Vec::new();
    let rd = std::fs::read_dir(&dir).map_err(|e| format!("读取 images 目录失败: {e}"))?;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.ends_with(".png") {
            names.push(name);
        }
    }
    names.sort_by(|a, b| b.cmp(a)); // 时间戳命名,倒序即最新在前

    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let path = dir.join(&name);
        // 缩略图:按字节内容嗅探格式解码(部分渠道实际返回 JPEG 但沿用 .png 命名,
        // image::open 会按扩展名误选 PNG 解码器导致整张被跳过)→ 256 等比缩 → jpeg q70 → data URL
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(img) = image::load_from_memory(&bytes) else { continue };
        let thumb_img = img.thumbnail(256, 256);
        let mut buf = std::io::Cursor::new(Vec::new());
        if thumb_img
            .write_to(&mut buf, image::ImageFormat::Jpeg)
            .is_err()
        {
            continue;
        }
        let thumb = format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
        );
        let meta_path = dir.join(name.replace(".png", ".meta.json"));
        let meta = std::fs::read_to_string(&meta_path)
            .ok()
            .and_then(|t| serde_json::from_str::<ImageMeta>(&t).ok());
        out.push(ImageListItem {
            path: path.to_string_lossy().to_string(),
            thumb,
            meta,
        });
    }
    Ok(out)
}

/// 读取单张图片的完整 data URL(历史列表点击放大时用)。
#[tauri::command]
pub fn read_image(path: String) -> Result<String, String> {
    let p = std::path::PathBuf::from(&path);
    ensure_in_images_dir(&p)?;
    let bytes = std::fs::read(&p).map_err(|e| format!("读取图片失败: {e}"))?;
    // 文件统一 .png 命名,但部分渠道返回的是 JPEG;按字节内容标注真实 MIME。
    let mime = match image::guess_format(&bytes) {
        Ok(image::ImageFormat::Jpeg) => "image/jpeg",
        Ok(image::ImageFormat::WebP) => "image/webp",
        Ok(image::ImageFormat::Gif) => "image/gif",
        _ => "image/png",
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// 「上传到对话」:把已生成图片复制为指定会话的图片附件,返回附件元数据。
#[tauri::command]
pub async fn image_to_session(
    image_path: String,
    session_id: String,
) -> Result<super::attachment::AttachmentMeta, String> {
    let src = std::path::PathBuf::from(&image_path);
    ensure_in_images_dir(&src)?;
    let bytes = std::fs::read(&src).map_err(|e| format!("读取图片失败: {e}"))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let name = src
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image.png".into());
    super::attachment::save_attachment(session_id, name, b64).await
}

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

// ---------- 思考能力(v1.9.0 工作块 I) ----------

/// 查询某模型的思考能力(手动修正 > 内置规则表推断 > 不支持)。
/// 前端据此决定是否渲染思考控件、是否允许关闭/选择档位。
#[tauri::command]
pub fn get_reasoning_capability(provider_id: String, model: String) -> reasoning::ReasoningCapability {
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
        .and_then(|m| m.reasoning.clone())
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
            cache.entry(provider_id).or_default().push(model_cards::FetchedModel {
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

/// 用量归因上下文(与 Key 选择无关,仅影响落库内容)。
struct Attribution {
    session_id: Option<String>,
    agent_id: Option<String>,
    agent_name: Option<String>,
    pricing: Option<PricingCtx>,
}

/// 计价上下文:单价(元/百万 tokens)+ 计价快照与来源。
struct PricingCtx {
    input_price: f64,
    output_price: f64,
    snapshot: String,
    source: String,
}

/// 查询会话绑定的助手(agent_id, agent_name);未绑定返回 (None, None)。
fn agent_of_session(session_id: &str) -> (Option<String>, Option<String>) {
    let Some(row) = db::sessions::get(session_id).ok().flatten() else {
        return (None, None);
    };
    let Some(aid) = row.agent_id.filter(|s| !s.trim().is_empty()) else {
        return (None, None);
    };
    let name = crate::agents::find(&aid).map(|a| a.name);
    (Some(aid), name)
}

/// 查模型卡定价并生成快照;无定价信息返回 None(成本记为未知)。
fn pricing_for(provider_id: &str, model: &str) -> Option<PricingCtx> {
    let card = model_cards::list_model_cards()
        .into_iter()
        .find(|c| c.provider == provider_id && c.official_model_id == model)?;
    let p = card.pricing?;
    let input_price = p.input.or(p.input_cache_miss).or(p.input_cache_hit);
    let (Some(i), Some(o)) = (input_price, p.output) else {
        return None;
    };
    let snapshot = serde_json::json!({
        "input": i,
        "output": o,
        "per_image": p.per_image,
        "per_char": p.per_char,
        "currency": "CNY",
        "unit": "per_million_tokens",
        "card_id": card.id,
    })
    .to_string();
    Some(PricingCtx {
        input_price: i,
        output_price: o,
        snapshot,
        source: "model_card".into(),
    })
}

/// 成本(微元,1e-6 元):tokens × 单价(元/百万 tokens) = 微元。
fn cost_micro_of(p: &PricingCtx, input: Option<u32>, output: Option<u32>) -> i64 {
    let i = input.unwrap_or(0) as f64 * p.input_price;
    let o = output.unwrap_or(0) as f64 * p.output_price;
    (i + o).round() as i64
}

/// Provider 规格:构造 Provider 所需的全部非密钥参数。
/// 与 API Key 解耦,便于按槽位反复组装不同 Key 的实例。
pub(crate) struct ChatSpec {
    pub provider_id: String,
    pub model: String,
    pub base_url: String,
    pub display_name: String,
}

/// 从入参解析 Provider 规格(base_url 为空则报错,并做 scheme 安全校验)。
pub(crate) fn resolve_spec(
    provider_id: &str,
    model: &str,
    custom_base_url: Option<&str>,
    custom_display_name: Option<&str>,
) -> Result<ChatSpec, String> {
    let tpl = templates::find_template(provider_id)
        .ok_or_else(|| format!("未知 provider: {}", provider_id))?;
    let base_url = custom_base_url
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| tpl.base_url.clone());
    if base_url.trim().is_empty() {
        return Err(format!("provider {} 未配置 base_url", provider_id));
    }
    validate_base_url_scheme(&base_url)?;
    let display_name = custom_display_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| tpl.display_name.clone());
    Ok(ChatSpec {
        provider_id: tpl.id.clone(),
        model: model.to_string(),
        base_url,
        display_name,
    })
}

/// 用给定 Key 组装 OpenAI 兼容 Provider。
fn build_openai(spec: &ChatSpec, api_key: &str, stream_options: bool) -> Box<dyn Provider> {
    Box::new(
        OpenAiCompat::new(
            spec.provider_id.clone(),
            spec.display_name.clone(),
            spec.base_url.clone(),
            api_key.to_string(),
        )
        .with_stream_options(stream_options),
    )
}

/// 计算本次请求可用的槽位顺序(纯函数,便于单测):
///   - 未启用轮换:仅用最小槽位(slot 0),行为与升级前一致;
///   - 启用轮换:按用户配置顺序 + 生效开关过滤,未登记的槽位默认启用并追加。
/// 全部被停用时降级为最小槽位,避免彻底不可用。
fn order_slots(existing: &[u32], cfg: &RotationConfig) -> Vec<u32> {
    if existing.is_empty() {
        return Vec::new();
    }
    let min_slot = *existing.iter().min().unwrap_or(&0);
    if !cfg.enabled {
        return vec![min_slot];
    }
    let mut ordered: Vec<u32> = Vec::new();
    for k in cfg.keys.iter().filter(|k| k.enabled) {
        if existing.contains(&k.slot) && !ordered.contains(&k.slot) {
            ordered.push(k.slot);
        }
    }
    for s in existing {
        let disabled = cfg.keys.iter().any(|k| k.slot == *s && !k.enabled);
        if !disabled && !ordered.contains(s) {
            ordered.push(*s);
        }
    }
    if ordered.is_empty() {
        ordered.push(min_slot);
    }
    ordered
}

fn slot_label(cfg: &RotationConfig, slot: u32) -> Option<String> {
    cfg.keys
        .iter()
        .find(|k| k.slot == slot)
        .map(|k| k.label.trim().to_string())
        .filter(|l| !l.is_empty())
}

/// 在 `order_slots` 的次序基础上补上每槽位的权重 / 优先级(未登记 = 权重 1 / 优先级 0)。
fn apply_slot_meta(c: &mut KeyCandidate, cfg: &RotationConfig, slot: u32) {
    if let Some(meta) = cfg.keys.iter().find(|k| k.slot == slot) {
        c.weight = meta.weight.clamp(1, 50);
        c.priority = meta.priority;
    }
}

/// 归一化 base_url(去空白与尾部 '/'、统一小写),用于编组判定。
fn norm_base_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// 轮换状态命名空间:同 base_url 的 Provider 共享一个组(游标 + 冷却表)。
fn rotation_group(provider_id: &str, base_url: &str) -> String {
    let n = norm_base_url(base_url);
    if n.is_empty() {
        provider_id.to_string()
    } else {
        n
    }
}

/// 单 Provider 的候选 Key(按用户配置顺序 + 权重 / 优先级)。
fn own_candidates(provider_id: &str, cfg: &RotationConfig) -> Vec<KeyCandidate> {
    let slots = keychain::list_slots(provider_id).unwrap_or_default();
    order_slots(&slots, cfg)
        .into_iter()
        .map(|slot| {
            let mut c = KeyCandidate::new(provider_id, slot);
            apply_slot_meta(&mut c, cfg, slot);
            c
        })
        .collect()
}

/// 构建本次请求的候选 Key 池。
/// Phase 3 编组:当本 Provider 已启用轮换时,同 base_url 且满足下列条件的**对端 Provider**
/// 的 Key 一并并入候选池(跨 Provider 共享轮换):
///   - 对端自身也启用了轮换(双方 opt-in,避免静默借用他人 Key);
///   - 对端模型列表包含本次请求的模型(模型一致性约束,防止跨窗口模型串用)。
fn key_candidates(provider_id: &str, base_url: &str, model: &str, cfg: &RotationConfig) -> Vec<KeyCandidate> {
    let mut out = own_candidates(provider_id, cfg);
    if !cfg.enabled {
        return out;
    }
    let group = norm_base_url(base_url);
    if group.is_empty() {
        return out;
    }
    for t in templates::load_all() {
        if t.id == provider_id || norm_base_url(&t.base_url) != group {
            continue;
        }
        if model.is_empty() || !t.models.iter().any(|m| m == model) {
            continue;
        }
        let peer_cfg = rotation::config_for(&t.id);
        if !peer_cfg.enabled {
            continue;
        }
        out.extend(own_candidates(&t.id, &peer_cfg));
    }
    out
}

/// 非流式辅助请求(模型列表 / 图片生成)的 Key 轮换执行器:
/// 依次尝试各可用候选,遇限流 / 5xx / 网络错误则冷却该槽位并换下一把 Key。
/// 与对话共用同一 group 的冷却表,避免刚被限流的 Key 立刻又被辅助请求撞上。
async fn with_key_rotation<T, F, Fut>(
    group: &str,
    cands: Vec<KeyCandidate>,
    state: &RotationState,
    cfg: &RotationConfig,
    mut attempt: F,
) -> Result<T, String>
where
    F: FnMut(&KeyCandidate, String) -> Fut,
    Fut: std::future::Future<Output = Result<T, ProviderError>>,
{
    if cands.is_empty() {
        return Err("未配置 API Key".into());
    }
    let mut pool = cands;
    let mut last_error: Option<String> = None;
    for _ in 0..pool.len().max(1) {
        if pool.is_empty() {
            break;
        }
        let idx = match state.pick(group, &pool) {
            Some(i) => i,
            None => break,
        };
        let cand = pool[idx].clone();
        let key = match keychain::load_slot(&cand.provider_id, cand.slot) {
            Ok(Some(k)) => k,
            Ok(None) => {
                pool.retain(|c| c.cooldown_key() != cand.cooldown_key());
                continue;
            }
            Err(e) => return Err(e),
        };
        match attempt(&cand, key).await {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_error = Some(e.to_string());
                if !is_retryable(&e) {
                    break;
                }
                let cool = cooldown_for(&e, cfg.cooldown_secs);
                state.mark_cooldown_secs(group, &cand.provider_id, cand.slot, cool);
                pool.retain(|c| c.cooldown_key() != cand.cooldown_key());
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "所有 API Key 均不可用(可能全部被限流)".into()))
}

/// 请求唯一 id(毫秒时间戳 + 进程内自增序号)。
fn next_request_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        chrono::Utc::now().timestamp_millis(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

fn error_kind(e: &ProviderError) -> &'static str {
    match e {
        ProviderError::RateLimited { .. } => "rate_limited",
        ProviderError::Auth(_) => "auth",
        ProviderError::Server(_) => "server",
        ProviderError::Network(_) => "network",
        ProviderError::BadRequest(_) => "bad_request",
        ProviderError::NotSupported(_) => "not_supported",
        ProviderError::Parse(_) => "parse",
        ProviderError::Other(_) => "other",
    }
}

/// 是否值得换槽重试(限流 / 服务端错误 / 网络错误)。
fn is_retryable(e: &ProviderError) -> bool {
    matches!(
        e,
        ProviderError::RateLimited { .. } | ProviderError::Server(_) | ProviderError::Network(_)
    )
}

/// 429 的建议冷却秒数:优先用响应头 Retry-After,缺失时回退到配置值。
fn cooldown_for(e: &ProviderError, cfg_secs: u64) -> u64 {
    match e {
        ProviderError::RateLimited { retry_after, .. } => retry_after.unwrap_or(cfg_secs).max(1),
        _ => 10,
    }
}

/// 该错误是否由「中转站不认 stream_options」引起(据此去掉字段重试一次)。
pub(crate) fn is_stream_options_rejected(e: &ProviderError) -> bool {
    match e {
        ProviderError::BadRequest(m) => {
            let m = m.to_ascii_lowercase();
            m.contains("stream_option")
        }
        _ => false,
    }
}

/// 该错误是否由「端点不认思考扩展参数」引起(据此剥离 reasoning 重试一次,v1.9.0 工作块 I)。
/// 典型报错关键词:reasoning_effort / thinking / enable_thinking(各家扩展字段名)。
pub(crate) fn is_reasoning_rejected(e: &ProviderError) -> bool {
    match e {
        ProviderError::BadRequest(m) => {
            let m = m.to_ascii_lowercase();
            m.contains("reasoning_effort")
                || m.contains("reasoning")
                || m.contains("thinking")
                || m.contains("enable_thinking")
        }
        _ => false,
    }
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
                    let mut stream_err: Option<String> = None;
                    while let Some(chunk) = stream.next().await {
                        match &chunk {
                            StreamChunk::Usage {
                                input_tokens,
                                output_tokens,
                                total_tokens,
                                reasoning_tokens,
                            } => {
                                toks = (*input_tokens, *output_tokens, *total_tokens);
                                if reasoning_toks.is_none() {
                                    reasoning_toks = *reasoning_tokens;
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
                            }
                            StreamChunk::Error { message } => stream_err = Some(message.clone()),
                        }
                        if channel.send(chunk).is_err() {
                            break; // 前端已关闭 channel
                        }
                    }
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
                        first_token_ms,
                        total_ms: Some(started.elapsed().as_millis() as i64),
                        agent_id: attr.agent_id.clone(),
                        agent_name: attr.agent_name.clone(),
                        cost_micro: attr
                            .pricing
                            .as_ref()
                            .map(|p| cost_micro_of(p, toks.0, toks.1)),
                        pricing_snapshot: attr.pricing.as_ref().map(|p| p.snapshot.clone()),
                        cost_source: attr.pricing.as_ref().map(|p| p.source.clone()),
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
fn spawn_chat_to_channel(
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
    let mut system = crate::provider::translate_system_prompt(
        input.source_lang.as_deref(),
        &input.target_lang,
    );
    if let Some(hints) = super::translate::glossary_hints(&input.text) {
        system.push_str(&hints);
    }

    let req = ChatRequest {
        model: input.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: system,
                images: vec![],
            },
            ChatMessage {
                role: "user".into(),
                content: input.text,
                images: vec![],
            },
        ],
        temperature: Some(0.2),
        max_tokens: None,
        top_p: None,
        reasoning: None, // 翻译走自有口径,不启用思考
        stream: true,
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
            )
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
            )
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
    tauri::async_runtime::spawn(run_cancellable(
        cancel_state,
        cancel_id,
        cancel_rx,
        work,
    ));

    Ok(())
}

/// 根据参数构造 Provider(chat_stream / summarize_memory 等命令共用)。
/// base_url / display_name 优先用显式入参,缺省回退到模板(base_url 为空则报错);
/// api_key 一律从 keychain 按 provider_id 取。
pub(crate) fn build_provider(
    provider_id: &str,
    model: &str,
    custom_base_url: Option<&str>,
    custom_display_name: Option<&str>,
) -> Result<Box<dyn Provider>, String> {
    build_provider_with_stream_options(provider_id, model, custom_base_url, custom_display_name, false)
}

/// H3:build_provider 的可变体,允许开启 stream_options.include_usage(研究链路
/// 采集真实 Token 用量;不兼容的中转站由调用方用 false 重建回退)。
pub(crate) fn build_provider_with_stream_options(
    provider_id: &str,
    model: &str,
    custom_base_url: Option<&str>,
    custom_display_name: Option<&str>,
    stream_options: bool,
) -> Result<Box<dyn Provider>, String> {
    // 非对话路径(图片/翻译/研究/记忆提炼)统一使用 slot 0(历史单 Key 语义);
    // slot 0 缺失时(用户把 Key 都配在 1..N 槽位),回退到轮换顺序中的第一把可用 Key,
    // 仅取一把、不做故障轮换,保持非对话路径的单 Key 语义。
    let api_key = match keychain::load_slot(provider_id, 0)? {
        Some(k) => k,
        None => {
            let cfg = rotation::config_for(provider_id);
            own_candidates(provider_id, &cfg)
                .into_iter()
                .find_map(|c| keychain::load_slot(provider_id, c.slot).ok().flatten())
                .ok_or_else(|| format!("provider {} 未配置 API Key", provider_id))?
        }
    };
    let spec = resolve_spec(provider_id, model, custom_base_url, custom_display_name)?;
    Ok(build_openai(&spec, &api_key, stream_options))
}

/// M1 安全校验:base_url 仅允许 https;例外放行本机回环(http://localhost / 127.0.0.1 / [::1]),
/// 防止 API Key 经明文 http 外发到第三方主机(CWE-319)。
pub(crate) fn validate_base_url_scheme(base_url: &str) -> Result<(), String> {
    let u = reqwest::Url::parse(base_url.trim()).map_err(|e| format!("base_url 无法解析: {e}"))?;
    match u.scheme() {
        "https" => Ok(()),
        "http" => {
            // url crate 对 IPv6 字面量返回带方括号形式([::1]),统一去括号后再判定
            let host = u.host_str().unwrap_or_default();
            let h = host.trim_start_matches('[').trim_end_matches(']');
            if h == "localhost" || h == "127.0.0.1" || h == "::1" {
                Ok(())
            } else {
                Err("base_url 仅允许 https;http 仅限本机回环(localhost/127.0.0.1/[::1])".into())
            }
        }
        s => Err(format!("base_url 协议不支持: {s}(仅允许 https)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rc(enabled: bool, keys: Vec<(u32, bool)>) -> RotationConfig {
        RotationConfig {
            enabled,
            keys: keys
                .into_iter()
                .map(|(slot, en)| KeySlotMeta {
                    slot,
                    label: format!("k{slot}"),
                    enabled: en,
                    weight: 1,
                    priority: 0,
                })
                .collect(),
            ..Default::default()
        }
    }

    /// 未启用轮换:仅用最小槽位,行为与升级前一致。
    #[test]
    fn order_slots_disabled_uses_min_slot() {
        assert_eq!(order_slots(&[0, 1, 2], &rc(false, vec![])), vec![0]);
        assert_eq!(order_slots(&[1, 2], &rc(false, vec![])), vec![1]);
        assert!(order_slots(&[], &rc(false, vec![])).is_empty());
    }

    /// 启用轮换:按用户配置顺序排列,并跳过被停用的槽位。
    #[test]
    fn order_slots_respects_meta_order_and_enabled() {
        let cfg = rc(true, vec![(2, true), (1, false), (0, true)]);
        assert_eq!(order_slots(&[0, 1, 2], &cfg), vec![2, 0]);
    }

    /// 未登记的槽位默认启用,并追加在已登记槽位之后。
    #[test]
    fn order_slots_appends_unregistered() {
        let cfg = rc(true, vec![(1, true)]);
        assert_eq!(order_slots(&[0, 1, 2], &cfg), vec![1, 0, 2]);
    }

    /// 全部停用时降级为最小槽位,避免彻底不可用。
    #[test]
    fn order_slots_all_disabled_degrades() {
        let cfg = rc(true, vec![(0, false), (1, false)]);
        assert_eq!(order_slots(&[0, 1], &cfg), vec![0]);
    }

    /// 中转站不认 stream_options 的 400 文案可被识别,用于回退重试。
    #[test]
    fn detect_stream_options_rejection() {
        assert!(is_stream_options_rejected(&ProviderError::BadRequest(
            "Unrecognized request argument supplied: stream_options".into()
        )));
        assert!(!is_stream_options_rejected(&ProviderError::BadRequest(
            "invalid model".into()
        )));
        assert!(!is_stream_options_rejected(&ProviderError::RateLimited {
            message: "429".into(),
            retry_after: None
        }));
    }

    /// 限流 / 5xx / 网络错误可换槽重试;鉴权错误不可。
    #[test]
    fn retryable_classification() {
        assert!(is_retryable(&ProviderError::RateLimited {
            message: "429".into(),
            retry_after: None
        }));
        assert!(is_retryable(&ProviderError::Server("503".into())));
        assert!(is_retryable(&ProviderError::Network("timeout".into())));
        assert!(!is_retryable(&ProviderError::Auth("401".into())));
        assert!(!is_retryable(&ProviderError::BadRequest("400".into())));
    }

    /// Phase 2:429 冷却优先采用 Retry-After,缺失时回退配置值;其他错误固定 10s。
    #[test]
    fn cooldown_prefers_retry_after_header() {
        let with = ProviderError::RateLimited {
            message: "429".into(),
            retry_after: Some(42),
        };
        assert_eq!(cooldown_for(&with, 60), 42);
        let without = ProviderError::RateLimited {
            message: "429".into(),
            retry_after: None,
        };
        assert_eq!(cooldown_for(&without, 60), 60);
        assert_eq!(cooldown_for(&ProviderError::Server("503".into()), 60), 10);
    }

    /// Phase 2:权重 / 优先级随配置带入选槽候选(未登记 = weight 1 / priority 0)。
    #[test]
    fn slot_candidates_carry_weight_and_priority() {
        let mut cfg = rc(true, vec![(1, true), (0, true)]);
        cfg.keys[0].weight = 3;
        cfg.keys[0].priority = 2;
        // 次序:登记的 1、0,未登记的 2 追加末尾
        assert_eq!(order_slots(&[0, 1, 2], &cfg), vec![1, 0, 2]);
        let mut c1 = KeyCandidate::new("p", 1);
        apply_slot_meta(&mut c1, &cfg, 1);
        assert_eq!((c1.weight, c1.priority), (3, 2));
        let mut c0 = KeyCandidate::new("p", 0);
        apply_slot_meta(&mut c0, &cfg, 0);
        assert_eq!((c0.weight, c0.priority), (1, 0));
    }

    /// Phase 3:编组命名空间按归一化 base_url 计算(空则回退 provider_id)。
    #[test]
    fn rotation_group_normalizes_base_url() {
        assert_eq!(
            rotation_group("a", " https://Api.Example.com/v1/ "),
            "https://api.example.com/v1"
        );
        assert_eq!(rotation_group("a", "https://x.com/"), rotation_group("b", "https://X.com"));
        assert_eq!(rotation_group("a", ""), "a");
    }

    /// M1 回归:明文 http 指向第三方主机必须被拒绝。
    #[test]
    fn build_provider_rejects_plain_http() {
        assert!(validate_base_url_scheme("http://evil.com").is_err());
        assert!(validate_base_url_scheme("http://evil.com:8080/v1").is_err());
        assert!(validate_base_url_scheme("ftp://evil.com").is_err());
        assert!(validate_base_url_scheme("not-a-url").is_err());
    }

    /// M1 回归:https 与 本机回环 http 放行。
    #[test]
    fn build_provider_allows_https_and_localhost_http() {
        assert!(validate_base_url_scheme("https://api.openai.com/v1").is_ok());
        assert!(validate_base_url_scheme("http://127.0.0.1:11434/v1").is_ok());
        assert!(validate_base_url_scheme("http://localhost:11434").is_ok());
        assert!(validate_base_url_scheme("http://[::1]:11434").is_ok());
    }

    /// M2 回归:私网/回环图片 URL 必须被拒绝(SSRF)。
    #[test]
    fn image_url_rejects_private_ip() {
        assert!(validate_image_url("http://192.168.1.1/x.png").is_err());
        assert!(validate_image_url("http://10.0.0.5/x.png").is_err());
        assert!(validate_image_url("http://172.16.0.9/x.png").is_err());
        assert!(validate_image_url("http://127.0.0.1/x.png").is_err());
        assert!(validate_image_url("http://169.254.1.1/x.png").is_err());
        assert!(validate_image_url("http://localhost/x.png").is_err());
        assert!(validate_image_url("http://[::1]/x.png").is_err());
        assert!(validate_image_url("file:///C:/x.png").is_err());
    }

    /// M2 回归:公网 http/https 图片 URL 放行。
    #[test]
    fn image_url_allows_public_host() {
        assert!(validate_image_url("https://oaidalleapiprodscus.blob.core.windows.net/x.png").is_ok());
        assert!(validate_image_url("http://8.8.8.8/x.png").is_ok());
    }
}
