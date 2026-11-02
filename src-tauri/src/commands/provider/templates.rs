//! Provider 模板 CRUD 与变更广播。

use crate::provider::model_cards::{load_model_cache, merge_image_models, save_model_cache};
use crate::provider::{
    keychain, model_config,
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
pub async fn list_provider_templates() -> Vec<ProviderTemplate> {
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
        // 图像模型候选:模板预置 ∪ 拉取缓存中声明 image 输出模态的模型
        // (豆包/中转站把 seedream 类模型挂在 /models 下,仅靠预置会让绘图页无可选项)
        if let Some(f) = cache.get(&t.id) {
            t.image_models = merge_image_models(&t.image_models, f);
        }
    }
    tpls
}

// ---------- 模板增删改(用户拥有完全权限)----------

/// 新增或更新一个 Provider 模板(id 为空时自动生成),返回落盘后的模板。
#[tauri::command]
pub async fn save_provider_template(
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

/// 手动设置某 Provider 的模型列表(单个增删 / 批量导入 / 批量删除共用入口)。
///
/// 落点与 `list_provider_templates` 的来源规则一致:
/// - 已拉取过厂家列表(缓存非空)→ 改写缓存,保留既有条目的元数据,新 id 补最小条目;
/// - 未拉取过 → 改写模板预置列表(provider_templates.json);
/// - 传空列表 = 清空该 Provider 的模型(缓存条目与预置一并清掉,避免回落复活旧列表)。
///
/// 返回生效后的模型 id 列表(已 trim + 去重,保持入参顺序)。
#[tauri::command]
pub async fn set_provider_models(
    app: tauri::AppHandle,
    provider_id: String,
    models: Vec<String>,
) -> Result<Vec<String>, String> {
    let mut seen = std::collections::HashSet::new();
    let ids: Vec<String> = models
        .iter()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty() && seen.insert(m.clone()))
        .collect();

    let mut cache = load_model_cache();
    let has_cache = cache.get(&provider_id).is_some_and(|l| !l.is_empty());
    if has_cache {
        let existing = cache.get(&provider_id).cloned().unwrap_or_default();
        if ids.is_empty() {
            cache.remove(&provider_id);
        } else {
            cache.insert(
                provider_id.clone(),
                crate::provider::model_cards::merge_fetched_models(&existing, &ids),
            );
        }
        save_model_cache(&cache)?;
        if ids.is_empty() {
            // 同步清空模板预置,避免缓存清空后回落把旧预置列表带回来
            let mut list = templates::load_all();
            if let Some(t) = list.iter_mut().find(|t| t.id == provider_id) {
                t.models = vec![];
                templates::save_all(&list)?;
            }
        }
    } else {
        let mut list = templates::load_all();
        let slot = list
            .iter_mut()
            .find(|t| t.id == provider_id)
            .ok_or_else(|| format!("未知 provider: {}", provider_id))?;
        slot.models = ids.clone();
        templates::save_all(&list)?;
        // 清掉可能存在的空缓存条目,保持「列表来源单一」
        if cache.remove(&provider_id).is_some() {
            save_model_cache(&cache)?;
        }
    }
    notify_providers_changed(&app);
    Ok(ids)
}

/// 删除一个 Provider 模板(内置/用户新增一视同仁),并清理其 Key、单模型配置与厂家模型缓存。
#[tauri::command]
pub async fn delete_provider_template(
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
pub async fn restore_builtin_templates(app: tauri::AppHandle) -> Result<(), String> {
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
    crate::diagnostics::safe_emit(app, "providers_changed", ());
}
