//! Provider 模板 CRUD 与变更广播。

use crate::provider::model_cards::{load_model_cache, save_model_cache};
use crate::provider::{
    keychain,
    model_config,
    rotation::{self, RotationState},
    templates::{self, ProviderTemplate},
};

use super::assembly::validate_base_url_scheme;
use super::keys::rotation_group;

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

/// 密钥 / 模板 / 单模型配置变更后广播,各窗口据此刷新全局 providers store。
/// 修复「设置页保存 Key 后同生命周期内对话页仍显示未配置」的状态不同步问题(H-1)。
pub(crate) fn notify_providers_changed(app: &tauri::AppHandle) {
    use tauri::Emitter;
    let _ = app.emit("providers_changed", ());
}
