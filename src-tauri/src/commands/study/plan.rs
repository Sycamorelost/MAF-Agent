//! 意图澄清 + 学习规划启动/暂停/恢复/终止。

use std::sync::{Arc, Mutex, atomic::Ordering};

use serde::Deserialize;
use tauri::State;
use tauri::ipc::Channel;

use crate::memory;
use crate::study::{self, Intent, StudyEvent, store};

use super::registry::{StudyCtl, StudyEntry, StudyRegistry};

// ---------- 意图澄清(preflight) ----------

#[derive(Debug, Deserialize)]
pub struct StudyPreflightInput {
    pub topic: String,
    pub lang: String,
    pub provider_id: String,
    pub model: String,
    pub custom_base_url: Option<String>,
    pub custom_display_name: Option<String>,
    /// 面板已确认的字段:已填就不追问
    pub level: Option<String>,
    pub daily_minutes: Option<u32>,
    pub deadline: Option<String>,
}

/// 启动前单独跑一次意图抽取:缺字段回规划面板内联追问(不中断流水线)。
#[tauri::command]
pub async fn study_preflight(input: StudyPreflightInput) -> Result<Intent, String> {
    let topic = input.topic.trim().to_string();
    if topic.is_empty() {
        return Err("学习主题不能为空".into());
    }
    let provider = crate::commands::provider::build_provider_with_stream_options(
        &input.provider_id,
        &input.model,
        input.custom_base_url.as_deref(),
        input.custom_display_name.as_deref(),
        true,
    )?;
    study::preflight(
        &topic,
        &input.lang,
        study::IntentHints {
            level: input.level,
            daily_minutes: input.daily_minutes,
            deadline: input.deadline,
        },
        provider,
        &input.model,
    )
    .await
}

// ---------- 启动 / 控制 ----------

#[derive(Debug, Deserialize)]
pub struct StudyStartInput {
    pub topic: String,
    /// preflight 产出的意图 JSON(缺字段已带用户答案;None = 直接开始,意图兜底)
    pub intent_json: Option<String>,
    /// 面板兜底:每天可投入分钟
    pub daily_minutes: Option<u32>,
    /// 面板兜底:目标完成日期
    pub deadline: Option<String>,
    pub lang: String,
    pub provider_id: String,
    pub model: String,
    pub custom_base_url: Option<String>,
    pub custom_display_name: Option<String>,
    /// P2:是否生成每日任务(None = 默认开)
    pub daily_tasks: Option<bool>,
    /// P3:是否预生成每个知识点的完整教学内容(None = 默认关,成本较高)
    pub full_content: Option<bool>,
}

/// 启动一次学习规划。立即返回计划 id;过程事件经 channel 推送。
#[tauri::command]
pub async fn study_start(
    input: StudyStartInput,
    channel: Channel<StudyEvent>,
    registry: State<'_, StudyRegistry>,
) -> Result<String, String> {
    let topic = input.topic.trim().to_string();
    if topic.is_empty() {
        return Err("学习主题不能为空".into());
    }
    let lang = if input.lang == "en" {
        "en".to_string()
    } else {
        "zh".to_string()
    };
    let daily_minutes = input.daily_minutes.unwrap_or(30).clamp(5, 600);

    // 早失败:Provider 先构造
    let provider = crate::commands::provider::build_provider_with_stream_options(
        &input.provider_id,
        &input.model,
        input.custom_base_url.as_deref(),
        input.custom_display_name.as_deref(),
        true,
    )?;

    let id = memory::new_session_id();
    let now = chrono::Local::now().to_rfc3339();
    // 若前端已带 preflight 意图,学科三列先预填(列表可即时展示);流水线结束 finish_plan 会覆写为终值
    let (pre_subject, pre_band, pre_exam) = input
        .intent_json
        .as_deref()
        .and_then(|t| serde_json::from_str::<Intent>(t).ok())
        .map(|i| {
            let s = i.subject.trim().to_string();
            let key = if s.is_empty() {
                None
            } else {
                Some(
                    crate::study::subject::normalize_subject(&s)
                        .as_str()
                        .to_string(),
                )
            };
            let b = crate::study::subject::normalize_grade_band(&i.grade_band);
            (key, (!b.is_empty()).then_some(b), i.exam_target)
        })
        .unwrap_or((None, None, None));
    store::insert_plan(&store::PlanRow {
        id: id.clone(),
        topic: topic.clone(),
        status: "generating".into(),
        level: String::new(),
        goal: String::new(),
        daily_minutes: daily_minutes as i64,
        deadline: input.deadline.clone(),
        lang: lang.clone(),
        model_id: Some(input.model.clone()),
        error: None,
        repaired: false,
        subject: pre_subject,
        grade_band: pre_band,
        exam_target: pre_exam,
        preferences: None,
        created: now.clone(),
        updated: now,
    })?;

    let ctl = Arc::new(StudyCtl::default());
    let entry = Arc::new(StudyEntry {
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
        study::run(study::StudyParams {
            id: id2.clone(),
            topic,
            intent_json: input.intent_json,
            daily_minutes,
            deadline: input.deadline,
            lang,
            provider,
            model: input.model.clone(),
            cancel: ctl.cancel.clone(),
            paused: ctl.paused.clone(),
            channel,
            rebuild: Some(crate::provider::rebuild::ProviderRebuild {
                provider_id: input.provider_id,
                model: input.model,
                custom_base_url: input.custom_base_url,
                custom_display_name: input.custom_display_name,
            }),
            fallback: std::sync::Mutex::new(None),
            daily_tasks: input.daily_tasks.unwrap_or(true),
            full_content: input.full_content.unwrap_or(false),
        })
        .await;
        if let Ok(mut m) = reg.lock() {
            m.remove(&id2);
        }
    });
    if let Ok(mut g) = entry.handle.lock() {
        *g = Some(handle);
    }
    Ok(id)
}

/// 终止正在运行的规划(引擎在下一个检查点停下,计划标记 cancelled)。
#[tauri::command]
pub async fn study_cancel(id: String, registry: State<'_, StudyRegistry>) -> Result<(), String> {
    memory::validate_id(&id)?;
    let m = registry.0.lock().map_err(|e| e.to_string())?;
    if let Some(entry) = m.get(&id) {
        entry.ctl.cancel.store(true, Ordering::Relaxed);
        entry.ctl.paused.store(false, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn study_pause(id: String, registry: State<'_, StudyRegistry>) -> Result<(), String> {
    memory::validate_id(&id)?;
    let m = registry.0.lock().map_err(|e| e.to_string())?;
    if let Some(entry) = m.get(&id) {
        entry.ctl.paused.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn study_resume(id: String, registry: State<'_, StudyRegistry>) -> Result<(), String> {
    memory::validate_id(&id)?;
    let m = registry.0.lock().map_err(|e| e.to_string())?;
    if let Some(entry) = m.get(&id) {
        entry.ctl.paused.store(false, Ordering::Relaxed);
    }
    Ok(())
}
