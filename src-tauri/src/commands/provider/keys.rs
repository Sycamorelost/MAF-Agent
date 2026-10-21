//! API Key 槽位 / 轮换配置命令,以及 Provider 规格组装辅助。

use crate::db::usage;
use crate::provider::{
    keychain,
    openai::OpenAiCompat,
    rotation::{self, KeyCandidate, KeySlotMeta, RotationConfig, RotationState},
    templates,
    Provider,
};

use super::assembly::validate_base_url_scheme;
use super::templates::notify_providers_changed;

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
pub(crate) fn build_openai(spec: &ChatSpec, api_key: &str, stream_options: bool) -> Box<dyn Provider> {
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
pub(crate) fn order_slots(existing: &[u32], cfg: &RotationConfig) -> Vec<u32> {
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

pub(crate) fn slot_label(cfg: &RotationConfig, slot: u32) -> Option<String> {
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
pub(crate) fn rotation_group(provider_id: &str, base_url: &str) -> String {
    let n = norm_base_url(base_url);
    if n.is_empty() {
        provider_id.to_string()
    } else {
        n
    }
}

/// 单 Provider 的候选 Key(按用户配置顺序 + 权重 / 优先级)。
pub(crate) fn own_candidates(provider_id: &str, cfg: &RotationConfig) -> Vec<KeyCandidate> {
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
pub(crate) fn key_candidates(provider_id: &str, base_url: &str, model: &str, cfg: &RotationConfig) -> Vec<KeyCandidate> {
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
}
