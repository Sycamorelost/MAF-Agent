//! 深度研究相关 Tauri commands(Phase 2)。

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::memory;
use crate::commands::provider::validate_base_url_scheme;
use crate::research::{
    self, blueprint, preset, search, store, template, EngineParams, ResearchEvent, ResearchParams,
    SourceItem,
};

/// 一次运行中研究的控制标记(Arc 共享给引擎任务)。
#[derive(Clone, Default)]
pub struct ResearchCtl {
    pub cancel: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
}

/// 注册表条目:控制标记 + 引擎任务句柄(删除研究时凭句柄等待任务真正退出,M-4)。
pub struct ResearchEntry {
    pub ctl: Arc<ResearchCtl>,
    /// 任务启动后回填;删除时 take 走并 await。任务自身结束时会从注册表移除整个条目。
    handle: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

/// 正在运行的研究任务的控制表(id → 控制标记 + 句柄)。
#[derive(Clone, Default)]
pub struct ResearchRegistry(pub Arc<Mutex<HashMap<String, Arc<ResearchEntry>>>>);

// ---------- 搜索引擎配置 ----------

#[derive(Debug, Serialize)]
pub struct SearchConfigDto {
    pub engine: String,
    pub base_url: String,
    pub has_api_key: bool,
}

/// 读取搜索引擎配置(不回传 key,只给 has_api_key)。
#[tauri::command]
pub fn get_search_config() -> Result<SearchConfigDto, String> {
    let cfg = search::load_config()?.unwrap_or_default();
    let has_key = if search::engine_needs_key(&cfg.engine) {
        search::has_engine_key(&cfg.engine)?
    } else {
        false
    };
    Ok(SearchConfigDto {
        engine: cfg.engine,
        base_url: cfg.base_url,
        has_api_key: has_key,
    })
}

#[derive(Debug, Deserialize)]
pub struct SaveSearchConfigInput {
    pub engine: String,
    pub base_url: String,
    /// key 类引擎用;非空才覆盖保存(searxng 忽略)
    pub api_key: Option<String>,
}

/// 保存搜索引擎配置(api_key 走 keychain 加密落盘,按引擎分别存储)。
#[tauri::command]
pub fn save_search_config(input: SaveSearchConfigInput) -> Result<(), String> {
    // key 类引擎(bocha/zhipu/tavily)留空 base_url 时使用官方默认地址,仅非空时校验。
    // searxng 必填,留空会在 build_engine 处给出可读错误。
    let base_url = input.base_url.trim();
    if !base_url.is_empty() {
        validate_base_url_scheme(base_url)?;
    }
    search::save_config(&search::SearchConfig {
        engine: input.engine.clone(),
        base_url: input.base_url,
    })?;
    if let Some(k) = input.api_key {
        let k = k.trim();
        if !k.is_empty() && search::engine_needs_key(&input.engine) {
            search::save_engine_key(&input.engine, k)?;
        }
    }
    Ok(())
}

/// 用当前配置跑一次测试查询,返回编号后的结果(验证配置可用)。
#[tauri::command]
pub async fn search_test(query: String) -> Result<Vec<SourceItem>, String> {
    let engine = search::build_engine()?;
    let raws = engine.search(&query, 5).await?;
    Ok(raws
        .into_iter()
        .enumerate()
        .map(|(i, r)| SourceItem {
            id: (i + 1) as u32,
            title: r.title,
            url: r.url,
            snippet: r.snippet,
            engine: engine.name().to_string(),
            published: r.published,
        })
        .collect())
}

// ---------- 研究任务 ----------

#[derive(Debug, Deserialize)]
pub struct ResearchStartInput {
    pub topic: String,
    pub provider_id: String,
    pub model: String,
    pub custom_base_url: Option<String>,
    pub custom_display_name: Option<String>,
    /// 迭代 D4:完整研究参数(缺省字段由 serde default 补齐)
    #[serde(default)]
    pub params: ResearchParams,
    /// 来源研究方案名(步骤 4 方案库;直接启动为 None)
    pub blueprint_name: Option<String>,
}

/// 启动一次深度研究。立即返回研究 id;过程事件经 channel 推送。
#[tauri::command]
pub async fn research_start(
    input: ResearchStartInput,
    channel: Channel<ResearchEvent>,
    registry: State<'_, ResearchRegistry>,
) -> Result<String, String> {
    let topic = input.topic.trim().to_string();
    if topic.is_empty() {
        return Err("研究主题不能为空".into());
    }
    let mut params = input.params;
    params.max_rounds = params.max_rounds.clamp(1, 25);
    params.queries_per_round = params.queries_per_round.clamp(1, 10);
    // 全局约束校验:并发 1-5;失败策略/去重范围非法值回退默认
    params.concurrent_queries = params.concurrent_queries.clamp(1, 5);
    if !matches!(
        params.fail_policy.as_str(),
        "skip" | "abort_stage" | "abort_research"
    ) {
        params.fail_policy = "skip".into();
    }
    if !matches!(params.dedup_scope.as_str(), "global" | "stage") {
        params.dedup_scope = "global".into();
    }
    if params.source_mode == "whitelist" && params.whitelist_domains.is_empty() {
        return Err("域名白名单模式下请至少填写一个域名".into());
    }
    // 专项模式校验:至少一个启用的阶段;无名阶段给默认名,撰写阶段强制 0 轮
    if params.mode == "staged" {
        for (i, s) in params.stages.iter_mut().enumerate() {
            if s.name.trim().is_empty() {
                s.name = format!("阶段 {}", i + 1);
            }
            if s.kind != "synthesis" {
                s.kind = "research".into();
            }
        }
        if !params.stages.iter().any(|s| s.enabled) {
            return Err("专项研究至少需要一个启用的阶段".into());
        }
        if params
            .stages
            .iter()
            .any(|s| s.enabled && s.kind == "research" && s.constraints.min_sources.unwrap_or(0) > 0 && s.max_rounds == Some(0))
        {
            return Err("检索阶段轮次不能为 0".into());
        }
    } else {
        params.mode = "legacy".into();
    }

    // 早失败:搜索引擎与 Provider 先构造,避免起了任务才报错
    let engine = search::build_engine()?;
    // H3:研究链路开启 stream_options.include_usage 采集真实 Token 用量;
    // 中转站不兼容时引擎内自动重建关闭该字段的实例回退
    let provider = crate::commands::provider::build_provider_with_stream_options(
        &input.provider_id,
        &input.model,
        input.custom_base_url.as_deref(),
        input.custom_display_name.as_deref(),
        true,
    )?;

    let id = memory::new_session_id();
    let now = chrono::Local::now().to_rfc3339();
    // M4:启用阶段总数启动即落库,列表页直接读列
    let stages_total = if params.mode == "staged" {
        params.stages.iter().filter(|s| s.enabled).count() as i64
    } else {
        0
    };
    store::insert(&store::ResearchRow {
        id: id.clone(),
        topic: topic.clone(),
        status: "running".into(),
        rounds: 0,
        max_rounds: params.max_rounds as i64,
        model_id: Some(input.model.clone()),
        error: None,
        created: now.clone(),
        updated: now,
        mode: params.mode.clone(),
        current_stage: 0,
        blueprint_name: input
            .blueprint_name
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        stages_done: 0,
        stages_total,
    })?;

    let ctl = Arc::new(ResearchCtl::default());
    let entry = Arc::new(ResearchEntry {
        ctl: ctl.clone(),
        handle: Mutex::new(None),
    });
    registry
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .insert(id.clone(), entry.clone());

    let reg = registry.0.clone();
    let id2 = id.clone();
    let handle = tauri::async_runtime::spawn(async move {
        research::run(EngineParams {
            id: id2.clone(),
            topic,
            provider,
            model: input.model.clone(),
            engine,
            params,
            cancel: ctl.cancel.clone(),
            paused: ctl.paused.clone(),
            channel,
            rebuild: Some(research::ProviderRebuild {
                provider_id: input.provider_id,
                model: input.model,
                custom_base_url: input.custom_base_url,
                custom_display_name: input.custom_display_name,
            }),
            fallback: std::sync::Mutex::new(None),
        })
        .await;
        if let Ok(mut m) = reg.lock() {
            m.remove(&id2);
        }
    });
    // 回填句柄供 research_delete 等待(M-4)。极端情况下任务已自行退出并从表中移除,
    // 此 entry 也会随之失效,回填无副作用。
    if let Ok(mut g) = entry.handle.lock() {
        *g = Some(handle);
    }

    Ok(id)
}

/// 请求取消一次正在运行的研究(置取消标记,引擎在下一个检查点停下)。
#[tauri::command]
pub fn research_cancel(id: String, registry: State<'_, ResearchRegistry>) -> Result<(), String> {
    memory::validate_id(&id)?;
    let m = registry.0.lock().map_err(|e| e.to_string())?;
    if let Some(entry) = m.get(&id) {
        entry.ctl.cancel.store(true, Ordering::Relaxed);
        Ok(())
    } else {
        Err("研究任务不存在或已结束".into())
    }
}

/// 暂停一次正在运行的研究(引擎在下一个检查点挂起)。
#[tauri::command]
pub fn research_pause(id: String, registry: State<'_, ResearchRegistry>) -> Result<(), String> {
    memory::validate_id(&id)?;
    let m = registry.0.lock().map_err(|e| e.to_string())?;
    if let Some(entry) = m.get(&id) {
        entry.ctl.paused.store(true, Ordering::Relaxed);
        Ok(())
    } else {
        Err("研究任务不存在或已结束".into())
    }
}

/// 继续一次已暂停的研究。
#[tauri::command]
pub fn research_resume(id: String, registry: State<'_, ResearchRegistry>) -> Result<(), String> {
    memory::validate_id(&id)?;
    let m = registry.0.lock().map_err(|e| e.to_string())?;
    if let Some(entry) = m.get(&id) {
        entry.ctl.paused.store(false, Ordering::Relaxed);
        Ok(())
    } else {
        Err("研究任务不存在或已结束".into())
    }
}

/// 检查点确认继续(专项研究):语义等同 resume,供阶段间检查点操作条调用。
#[tauri::command]
pub fn research_stage_continue(
    id: String,
    registry: State<'_, ResearchRegistry>,
) -> Result<(), String> {
    memory::validate_id(&id)?;
    let m = registry.0.lock().map_err(|e| e.to_string())?;
    if let Some(entry) = m.get(&id) {
        entry.ctl.paused.store(false, Ordering::Relaxed);
        Ok(())
    } else {
        Err("研究任务不存在或已结束".into())
    }
}

/// H1:检查点「调整下阶段」— 写回 stages.json 的最新阶段编排。
/// 引擎在每个阶段起点热读 configs 使调整生效;运行期状态按 id 保留。
#[tauri::command]
pub fn research_update_stages(
    id: String,
    mut stages: Vec<research::stage::StageConfig>,
) -> Result<(), String> {
    memory::validate_id(&id)?;
    if stages.is_empty() {
        return Err("阶段编排不能为空".into());
    }
    // 与 research_start 相同的归一化:无名阶段给默认名,kind 白名单
    for (i, s) in stages.iter_mut().enumerate() {
        if s.name.trim().is_empty() {
            s.name = format!("阶段 {}", i + 1);
        }
        if s.kind != "synthesis" {
            s.kind = "research".into();
        }
    }
    let row = store::get(&id)?.ok_or_else(|| format!("研究 {id} 不存在"))?;
    if row.mode != "staged" {
        return Err("仅专项研究支持调整阶段".into());
    }
    let mut file = store::read_stages(&id)
        .ok_or_else(|| "阶段快照 stages.json 缺失,无法调整".to_string())?;
    file.configs = stages;
    // 状态按 id 保留(含已完成阶段的摘要);新增阶段补 pending / skipped
    let mut states = Vec::with_capacity(file.configs.len());
    for c in &file.configs {
        let prev = file.stages.iter().find(|s| s.id == c.id).cloned();
        states.push(match prev {
            Some(mut s) => {
                s.name = c.name.clone();
                s.kind = c.kind.clone();
                s.enabled = c.enabled;
                // 停用尚未开始的阶段直接标 skipped;反向操作只发生在未开始时,回到 pending
                if !c.enabled && s.status == "pending" {
                    s.status = "skipped".into();
                } else if c.enabled && s.status == "skipped" && s.summary.is_empty() {
                    s.status = "pending".into();
                }
                s
            }
            None => research::stage::StageState {
                id: c.id.clone(),
                name: c.name.clone(),
                kind: c.kind.clone(),
                enabled: c.enabled,
                status: if c.enabled { "pending".into() } else { "skipped".into() },
                summary: String::new(),
                sources_count: 0,
                rounds: 0,
                partial: false,
            },
        });
    }
    file.stages = states;
    file.updated = chrono::Local::now().to_rfc3339();
    store::save_stages(&id, &file)
}

// ---------- 研究预设(迭代 D4) ----------

/// 列出全部研究预设。
#[tauri::command]
pub fn research_preset_list() -> Vec<preset::ResearchPreset> {
    preset::load_presets()
}

/// 保存研究预设(同名覆盖),返回最新列表。
#[tauri::command]
pub fn research_preset_save(
    name: String,
    params: ResearchParams,
) -> Result<Vec<preset::ResearchPreset>, String> {
    preset::save_preset(&name, params)
}

/// 删除研究预设,返回最新列表。
#[tauri::command]
pub fn research_preset_delete(name: String) -> Result<Vec<preset::ResearchPreset>, String> {
    preset::delete_preset(&name)
}

// ---------- 研究方案 Blueprint(专项研究步骤 4) ----------

/// 列出全部研究方案(内置 + 用户;旧预设已自动迁移)。
#[tauri::command]
pub fn blueprint_list() -> Vec<blueprint::Blueprint> {
    blueprint::list()
}

/// 保存研究方案(同名覆盖;内置名保护),返回最新列表。
#[tauri::command]
pub fn blueprint_save(
    name: String,
    desc: String,
    params: ResearchParams,
) -> Result<Vec<blueprint::Blueprint>, String> {
    blueprint::save(&name, &desc, params)
}

/// 删除用户方案(内置名保护),返回最新列表。
#[tauri::command]
pub fn blueprint_delete(name: String) -> Result<Vec<blueprint::Blueprint>, String> {
    blueprint::delete(&name)
}

// ---------- 提示词模板(专项研究步骤 1) ----------

/// 列出全部研究提示词模板(全局决策提示 / 终稿报告模板,自定义覆盖优先)。
#[tauri::command]
pub fn research_template_list() -> Vec<template::TemplateDto> {
    template::list()
}

/// 保存模板自定义覆盖,返回最新列表。
#[tauri::command]
pub fn research_template_save(
    key: String,
    content: String,
) -> Result<Vec<template::TemplateDto>, String> {
    template::save(&key, &content)
}

/// 恢复内置模板(删除自定义覆盖),返回最新列表。
#[tauri::command]
pub fn research_template_reset(key: String) -> Result<Vec<template::TemplateDto>, String> {
    template::reset(&key)
}

/// 列出全部研究会话(按创建时间倒序;阶段进度直接读 M4 回写列,零文件 IO)。
#[tauri::command]
pub fn research_list() -> Result<Vec<store::ResearchRow>, String> {
    store::list()
}

#[derive(Debug, Serialize)]
pub struct ResearchDetail {
    pub session: store::ResearchRow,
    pub trail: Vec<ResearchEvent>,
    pub report: Option<String>,
    /// 专项模式阶段编排快照 + 运行期状态(legacy 为 None)
    pub stages: Option<research::stage::StagesFile>,
}

/// 获取研究详情(元数据 + 过程事件回放 + 报告正文 + 阶段快照)。
#[tauri::command]
pub fn research_get(id: String) -> Result<ResearchDetail, String> {
    memory::validate_id(&id)?;
    let session = store::get(&id)?.ok_or_else(|| format!("研究 {id} 不存在"))?;
    let trail = store::read_trail(&id)?;
    let report = store::read_report(&id)?;
    let stages = store::read_stages(&id);
    Ok(ResearchDetail {
        session,
        trail,
        report,
        stages,
    })
}

/// 删除研究(若在运行先取消并等待任务真正退出,再删 db 行 + 磁盘目录)。
///
/// M-4:旧实现只置取消标志就立刻删目录,引擎的下一次落盘会重新创建目录与文件,
/// 造成「删了又生」。这里取走任务句柄等待退出(上限 10 秒),确保引擎不再写盘。
#[tauri::command]
pub async fn research_delete(
    id: String,
    registry: State<'_, ResearchRegistry>,
) -> Result<(), String> {
    memory::validate_id(&id)?;
    let entry = {
        let m = registry.0.lock().map_err(|e| e.to_string())?;
        m.get(&id).cloned()
    };
    let mut engine_stuck = false;
    if let Some(entry) = entry {
        // 同时解除暂停,避免引擎停在 wait_gate 里不响应取消
        entry.ctl.paused.store(false, Ordering::Relaxed);
        entry.ctl.cancel.store(true, Ordering::Relaxed);
        if let Some(handle) = entry.handle.lock().ok().and_then(|mut g| g.take()) {
            // 引擎在检查点轮询间隔为 200ms,正常应在秒级退出;10 秒兜底防卡死阻塞前端
            engine_stuck = tokio::time::timeout(std::time::Duration::from_secs(10), handle)
                .await
                .is_err();
        }
        // 双保险:任务句柄缺失(如启动竞态)时再尝试从表中剔除
        if let Ok(mut m) = registry.0.lock() {
            m.remove(&id);
        }
    }
    // N-7:引擎卡死时仅删数据库行并保留目录,避免引擎随后重建目录「删而又生」
    if engine_stuck {
        log::warn!("research {id} 引擎 10s 内未退出,仅删除数据库记录,目录保留待手动清理");
        store::delete_row(&id)
    } else {
        store::delete(&id)
    }
}

// ---------- 科研增强:技能资源健康检查(v1.9.0 工作块 II) ----------

/// 列出内置科研方法论技能文档(健康检查;资源随包只读分发)。
#[tauri::command]
pub fn research_list_skills() -> Result<Vec<research::skills::SkillDoc>, String> {
    Ok(research::skills::list_docs())
}

// ---------- 报告导出(v1.9.0:仅 Markdown 与自包含 HTML,不做 PDF/DOCX/PPTX) ----------

/// 目标已存在但未获覆盖确认时返回的错误码;前端据此弹二次确认后带 overwrite=true 重试。
pub const EXPORT_TARGET_EXISTS: &str = "TARGET_EXISTS";

/// 导出路径令牌表(N-6):token → 用户经原生「另存为」对话框选定的路径。
/// 导出命令只认令牌,渲染层不能凭空指定任意路径/任意覆盖——彻底收敛 H-2 残余。
#[derive(Default)]
pub struct ExportPathTokens(std::sync::Mutex<std::collections::HashMap<String, std::path::PathBuf>>);

impl ExportPathTokens {
    /// 登记对话框选定路径,返回一次性语义的令牌(表有界,满 64 清空重建)。
    fn issue(&self, path: std::path::PathBuf) -> String {
        use rand::RngCore;
        let mut b = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut b);
        let token = b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        if let Ok(mut m) = self.0.lock() {
            if m.len() >= 64 {
                m.clear();
            }
            m.insert(token.clone(), path);
        }
        token
    }

    /// 按令牌取回路径(不消费:用户可能对同一目标重复导出)。
    fn resolve(&self, token: &str) -> Result<std::path::PathBuf, String> {
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .get(token)
            .cloned()
            .ok_or_else(|| "导出路径令牌无效或已过期,请重新选择保存位置".to_string())
    }
}

/// 另存为对话框结果(H-2/N-6):附带选择时目标是否已存在 + 后端签发的路径令牌。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePathPick {
    pub path: String,
    pub existed: bool,
    pub token: String,
}

/// 导出路径规范(H-2):必须绝对路径、扩展名与白名单一致、父目录已存在(不替调用方创建任意目录)。
fn ensure_export_path(raw: &str, allowed_ext: &str) -> Result<std::path::PathBuf, String> {
    let target = std::path::PathBuf::from(raw.trim());
    if !target.is_absolute() {
        return Err("导出路径必须为绝对路径".into());
    }
    let want = allowed_ext.trim_start_matches('.');
    let ext_ok = target
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(want))
        .unwrap_or(false);
    if !ext_ok {
        return Err(format!("导出文件扩展名必须为 .{want}"));
    }
    let parent = target.parent().ok_or("导出路径缺少父目录")?;
    if !parent.is_dir() {
        return Err("导出目录不存在,请通过另存为对话框选择有效位置".into());
    }
    Ok(target)
}

/// 覆盖校验(H-2):目标已是文件且未显式 overwrite 时拒绝(返回 TARGET_EXISTS);
/// 同名目录一律拒绝,避免目录被文件覆盖的怪异状态。
fn ensure_writable(target: &std::path::Path, overwrite: bool) -> Result<(), String> {
    match target.metadata() {
        Ok(meta) if meta.is_file() => {
            if overwrite {
                Ok(())
            } else {
                Err(EXPORT_TARGET_EXISTS.into())
            }
        }
        Ok(_) => Err("导出目标已存在且是目录,无法写入".into()),
        Err(_) => Ok(()),
    }
}

/// 弹出原生「另存为」对话框(报告导出入口);用户取消返回 None。
/// 与 pick_data_dir 同款实现:阻塞调用放入 blocking 线程池,避免卡住运行时。
/// 选定路径登记为令牌(N-6),后续导出命令只认令牌。
#[tauri::command]
pub async fn pick_save_path(
    app: tauri::AppHandle,
    tokens: State<'_, ExportPathTokens>,
    title: String,
    default_name: String,
    filter_name: String,
    filter_ext: String,
) -> Result<Option<SavePathPick>, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title(title)
            .set_file_name(default_name)
            .add_filter(filter_name, &[filter_ext.as_str()])
            .blocking_save_file()
            .and_then(|p| p.into_path().ok())
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    // 此时应用尚未写入:存在即代表覆盖了用户的旧文件,系统对话框已征得确认
    let existed = picked.is_file();
    let token = tokens.issue(picked.clone());
    Ok(Some(SavePathPick {
        path: picked.display().to_string(),
        existed,
        token,
    }))
}

/// 导出报告为 Markdown:写入用户选定路径,同目录创建 figures/ 资源夹并复制图示资产。
#[tauri::command]
pub fn research_export_markdown(
    id: String,
    token: String,
    overwrite: bool,
    tokens: State<'_, ExportPathTokens>,
) -> Result<String, String> {
    export_markdown_inner(id, token, overwrite, &tokens)
}

fn export_markdown_inner(
    id: String,
    token: String,
    overwrite: bool,
    tokens: &ExportPathTokens,
) -> Result<String, String> {
    memory::validate_id(&id)?;
    // 令牌解析先于 DB 读取:非法令牌快速失败,也便于无库单测
    let target = ensure_export_path(&tokens.resolve(&token)?.to_string_lossy(), "md")?;
    let report = store::read_report(&id)?.ok_or("报告不存在或尚未生成")?;
    ensure_writable(&target, overwrite)?;
    std::fs::write(&target, &report).map_err(|e| format!("写入 Markdown 失败: {e}"))?;
    // 图示资产:同目录 figures/ 复制(报告内相对引用 figures/xxx 直接可预览)。
    // 父目录已由 ensure_export_path 确认存在,仅在其下建资产夹,不触碰其他路径。
    let src_figs = store::research_dir(&id).join("figures");
    if src_figs.is_dir() {
        if let Some(parent) = target.parent() {
            let dst = parent.join("figures");
            if std::fs::create_dir_all(&dst).is_ok() {
                if let Ok(entries) = std::fs::read_dir(&src_figs) {
                    for e in entries.flatten() {
                        let p = e.path();
                        if p.is_file() {
                            if let Some(name) = p.file_name() {
                                let _ = std::fs::copy(&p, dst.join(name));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(target.to_string_lossy().into_owned())
}

/// 导出报告为自包含 HTML:前端序列化已渲染 DOM,后端将 figures/ 引用内嵌为 base64 后写盘。
#[tauri::command]
pub fn research_export_html(
    id: String,
    html: String,
    token: String,
    overwrite: bool,
    tokens: State<'_, ExportPathTokens>,
) -> Result<String, String> {
    export_html_inner(id, html, token, overwrite, &tokens)
}

fn export_html_inner(
    id: String,
    html: String,
    token: String,
    overwrite: bool,
    tokens: &ExportPathTokens,
) -> Result<String, String> {
    memory::validate_id(&id)?;
    let target = ensure_export_path(&tokens.resolve(&token)?.to_string_lossy(), "html")?;
    let figures = store::research_dir(&id).join("figures");
    let html = inline_figures(&html, &figures);
    ensure_writable(&target, overwrite)?;
    std::fs::write(&target, html).map_err(|e| format!("写入 HTML 失败: {e}"))?;
    Ok(target.to_string_lossy().into_owned())
}

/// 扫描 HTML 中的 `figures/{name}` 引用并替换为 base64 data URI(离线自包含)。
/// 名称白名单同 research_figure_data;未命中文件的引用原样保留。
fn inline_figures(html: &str, figures_dir: &std::path::Path) -> String {
    const MARKER: &str = "figures/";
    let mut out = String::with_capacity(html.len() + 4096);
    let mut rest = html;
    while let Some(i) = rest.find(MARKER) {
        out.push_str(&rest[..i]);
        let after = &rest[i + MARKER.len()..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
            .collect();
        if is_valid_figure_name(&name) {
            let path = figures_dir.join(&name);
            if path.parent() == Some(figures_dir) {
                if let Ok(bytes) = std::fs::read(&path) {
                    let mime = figure_mime(&name).unwrap_or("application/octet-stream");
                    use base64::Engine as _;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    out.push_str(&format!("data:{mime};base64,{b64}"));
                    rest = &after[name.len()..];
                    continue;
                }
            }
        }
        out.push_str(MARKER);
        rest = after;
    }
    out.push_str(rest);
    out
}

/// 图示文件名白名单:仅字母数字-_ 与单个点,扩展名必须为图片格式,拒绝路径分隔与 .. 序列。
fn is_valid_figure_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.contains("..")
        && !name.contains('/')
        && !name.contains('\\')
        && name.matches('.').count() == 1
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && figure_mime(name).is_some()
}

/// 图示 MIME(按字节嗅探为辅,扩展名为准;仅图片格式)。
fn figure_mime(name: &str) -> Option<&'static str> {
    match name.rsplit('.').next()? {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// 读取研究图示为 data URL(报告内联预览;仅允许 figures 目录内的图片文件)。
#[tauri::command]
pub fn research_figure_data(id: String, name: String) -> Result<String, String> {
    memory::validate_id(&id)?;
    if !is_valid_figure_name(&name) {
        return Err("图示文件名非法".into());
    }
    let mime = figure_mime(&name).ok_or("不支持的图示格式")?;
    let dir = store::research_dir(&id).join("figures");
    let path = dir.join(&name);
    if path.parent() != Some(dir.as_path()) {
        return Err("图示路径非法".into());
    }
    let bytes = std::fs::read(&path).map_err(|_| "图示不存在")?;
    use base64::Engine as _;
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// H1 回归:路径遍历 id 必须在入口被拒绝,且不得触碰磁盘。
    #[test]
    fn research_get_rejects_traversal_id() {
        let r = research_get("../../x".to_string());
        assert!(r.is_err());
        let msg = r.err().unwrap();
        assert!(msg.contains("非法"), "错误文案应指出 id 非法,实际: {msg}");
    }

    #[test]
    fn research_get_rejects_slash_id() {
        assert!(research_get("a/b".to_string()).is_err());
        assert!(research_get("a\\b".to_string()).is_err());
        assert!(research_get(String::new()).is_err());
    }

    /// H1 回归:导出接口同样必须拒绝路径遍历 id(id 校验先于令牌解析)。
    #[test]
    fn export_commands_reject_traversal_id() {
        let tokens = ExportPathTokens::default();
        assert!(export_markdown_inner("../x".into(), "t".into(), false, &tokens).is_err());
        assert!(export_html_inner("../x".into(), "<html/>".into(), "t".into(), false, &tokens).is_err());
        assert!(research_figure_data("../x".into(), "a.png".into()).is_err());
    }

    /// N-6 回归:非法令牌一律拒绝,渲染层无法凭空指定路径。
    #[test]
    fn export_commands_reject_unknown_token() {
        let tokens = ExportPathTokens::default();
        let r = export_markdown_inner("valid_id".into(), "no_such_token".into(), false, &tokens);
        assert!(r.is_err());
    }

    /// 图示文件名白名单:拒绝穿越/分隔/非法扩展。
    #[test]
    fn figure_name_whitelist() {
        assert!(is_valid_figure_name("draft-20260101-000000123.png"));
        assert!(is_valid_figure_name("a-b_c.png"));
        assert!(!is_valid_figure_name("..\\evil.png"));
        assert!(!is_valid_figure_name("a/b.png"));
        assert!(!is_valid_figure_name("a..b.png"));
        assert!(!is_valid_figure_name("a.exe"));
        assert!(!is_valid_figure_name(""));
        assert!(!is_valid_figure_name("a.png.exe"));
    }

    /// HTML 内嵌:figures 引用替换为 data URI,未命中原样保留。
    #[test]
    fn inline_figures_replaces_and_preserves() {
        let tmp = std::env::temp_dir().join("maf-inline-test");
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("t.png"), b"png").unwrap();
        let html = r#"<img src="figures/t.png"><a href="figures/missing.png">x</a>"#;
        let out = inline_figures(html, &tmp);
        assert!(out.starts_with(r#"<img src="data:image/png;base64,"#));
        assert!(out.contains(r#"href="figures/missing.png""#), "未命中引用应原样保留");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
