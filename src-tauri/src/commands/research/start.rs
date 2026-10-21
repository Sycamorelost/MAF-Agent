//! 搜索引擎配置与研究任务启动。

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::commands::provider::validate_base_url_scheme;
use crate::memory;
use crate::research::{
    self, search, store, EngineParams, ProviderRebuild, ResearchEvent, ResearchParams, SourceItem,
};

use super::{ResearchCtl, ResearchEntry, ResearchRegistry};

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
    } else if params.mode == "concise" {
        // 简洁模式:轮次 clamp 1-10(每轮 = 1 次整稿输出 + 1 组轻量检索,防失控)
        params.max_rounds = params.max_rounds.clamp(1, 10);
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

    // 参考资料一次性装配:读盘 + 文本提取只在启动时做一次(优先读导入时的缓存),不进入流式路径
    let references = params.references_block();
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
            rebuild: Some(ProviderRebuild {
                provider_id: input.provider_id,
                model: input.model,
                custom_base_url: input.custom_base_url,
                custom_display_name: input.custom_display_name,
            }),
            fallback: std::sync::Mutex::new(None),
            references,
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
