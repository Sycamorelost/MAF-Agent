//! 学习规划相关 Tauri commands(P0 规划 + P1 学习执行)。
//!
//! 命令族:study_preflight(意图澄清)/ study_start / study_cancel / study_pause /
//! study_resume / study_list / study_get / study_export_markdown / study_delete /
//! study_deep_dig(深挖研究联动);
//! P1:study_nodes / study_unit_generate / study_quiz_new / study_quiz_submit /
//! study_mastery_set / study_node_done / study_review_queue / study_review_done / study_today。

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::memory;
use crate::study::{self, learn, store, Intent, PlanPayload, StudyEvent};

/// 一次运行中规划的控制标记(Arc 共享给引擎任务)。
#[derive(Clone, Default)]
pub struct StudyCtl {
    pub cancel: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
}

/// 注册表条目:控制标记 + 任务句柄。
pub struct StudyEntry {
    pub ctl: Arc<StudyCtl>,
    handle: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

/// 正在运行的规划任务控制表(id → 控制 + 句柄)。
#[derive(Clone, Default)]
pub struct StudyRegistry(pub Arc<Mutex<HashMap<String, Arc<StudyEntry>>>>);

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
    let lang = if input.lang == "en" { "en".to_string() } else { "zh".to_string() };
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
        created: now.clone(),
        updated: now,
    })?;

    let ctl = Arc::new(StudyCtl::default());
    let entry = Arc::new(StudyEntry { ctl: ctl.clone(), handle: Mutex::new(None) });
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
            rebuild: Some(crate::research::ProviderRebuild {
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
    if let Ok(mut g) = entry.handle.lock() {
        *g = Some(handle);
    }
    Ok(id)
}

/// 终止正在运行的规划(引擎在下一个检查点停下,计划标记 cancelled)。
#[tauri::command]
pub fn study_cancel(id: String, registry: State<'_, StudyRegistry>) -> Result<(), String> {
    memory::validate_id(&id)?;
    let m = registry.0.lock().map_err(|e| e.to_string())?;
    if let Some(entry) = m.get(&id) {
        entry.ctl.cancel.store(true, Ordering::Relaxed);
        entry.ctl.paused.store(false, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub fn study_pause(id: String, registry: State<'_, StudyRegistry>) -> Result<(), String> {
    memory::validate_id(&id)?;
    let m = registry.0.lock().map_err(|e| e.to_string())?;
    if let Some(entry) = m.get(&id) {
        entry.ctl.paused.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub fn study_resume(id: String, registry: State<'_, StudyRegistry>) -> Result<(), String> {
    memory::validate_id(&id)?;
    let m = registry.0.lock().map_err(|e| e.to_string())?;
    if let Some(entry) = m.get(&id) {
        entry.ctl.paused.store(false, Ordering::Relaxed);
    }
    Ok(())
}

// ---------- 查询 / 删除 ----------

#[derive(Debug, Serialize)]
pub struct StudyDetail {
    pub plan: store::PlanRow,
    /// 生成的计划载荷(尚未生成/失败为 None)
    pub payload: Option<PlanPayload>,
    pub trail: Vec<StudyEvent>,
}

#[tauri::command]
pub fn study_list() -> Result<Vec<store::PlanRow>, String> {
    store::list_plans()
}

#[tauri::command]
pub fn study_get(id: String) -> Result<StudyDetail, String> {
    memory::validate_id(&id)?;
    let plan = store::get_plan(&id)?.ok_or("学习计划不存在")?;
    let trail = store::read_trail(&id)?;
    let payload = store::read_payload(&id);
    Ok(StudyDetail { plan, payload, trail })
}

/// 删除学习计划:运行中先终止,等引擎自行退出(≤5s)后删行删目录。
#[tauri::command]
pub fn study_delete(id: String, registry: State<'_, StudyRegistry>) -> Result<(), String> {
    memory::validate_id(&id)?;
    // 引擎退出时会把自己从注册表移除;以此判断是否仍在运行
    let running = |reg: &StudyRegistry| {
        reg.0.lock().map(|m| m.contains_key(&id)).unwrap_or(false)
    };
    if running(&registry) {
        if let Ok(m) = registry.0.lock() {
            if let Some(entry) = m.get(&id) {
                entry.ctl.cancel.store(true, Ordering::Relaxed);
                entry.ctl.paused.store(false, Ordering::Relaxed);
            }
        }
        // 等任务真正退出,避免删后引擎又写盘「删而又生」
        for _ in 0..50 {
            if !running(&registry) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    store::delete(&id)
}

// ---------- 深挖研究联动 ----------

/// 记录一次「深挖研究」:study_logs 落 kind=research 行,返回预填给研究页的主题。
#[tauri::command]
pub fn study_deep_dig(id: String, node_name: String) -> Result<String, String> {
    memory::validate_id(&id)?;
    let name = node_name.trim().to_string();
    if name.is_empty() {
        return Err("知识点名称不能为空".into());
    }
    let plan = store::get_plan(&id)?.ok_or("学习计划不存在")?;
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    let node_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM knowledge_nodes WHERE plan_id=?1 AND name=?2",
            rusqlite::params![id, name],
            |r| r.get(0),
        )
        .ok();
    store::add_log(&conn, &id, node_id, "research", 0)?;
    Ok(format!("深入学习:{}(源自学习计划「{}」)", name, plan.topic))
}

// ---------- 导出 ----------

/// 导出学习计划 Markdown,返回实际写入路径。
/// token 为 pick_save_path 签发的路径令牌(复用研究导出的 N-6 机制)。
#[tauri::command]
pub fn study_export_markdown(
    id: String,
    token: String,
    overwrite: bool,
    tokens: State<'_, crate::commands::research::ExportPathTokens>,
) -> Result<String, String> {
    memory::validate_id(&id)?;
    let target = tokens.resolve(&token)?;
    if target.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) != Some("md".into()) {
        return Err("导出文件扩展名必须为 .md".into());
    }
    let payload = store::read_payload(&id).ok_or("学习计划尚未生成,无法导出")?;
    let md = study::export_markdown(&payload);
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("mkdir failed: {e}"))?;
    }
    if target.exists() && !overwrite {
        return Err("TARGET_EXISTS".into());
    }
    std::fs::write(&target, md).map_err(|e| format!("write failed: {e}"))?;
    Ok(target.to_string_lossy().to_string())
}

// ---------- P1:学习执行(自学材料 / 测验 / 掌握度 / 复习 / 今日待办) ----------

/// 知识点落库状态(阶段树合并 payload)。
#[tauri::command]
pub fn study_nodes(id: String) -> Result<Vec<learn::NodeState>, String> {
    memory::validate_id(&id)?;
    learn::nodes_state(&id)
}

/// 单元/测验生成入参。
#[derive(Debug, Deserialize)]
pub struct StudyGenInput {
    pub id: String,
    pub node_id: i64,
    /// 自学材料不套固定小节结构
    pub free_form: Option<bool>,
    /// 重新生成(替换已缓存单元)
    pub force: Option<bool>,
    pub provider_id: String,
    pub model: String,
    pub custom_base_url: Option<String>,
    pub custom_display_name: Option<String>,
}

fn gen_provider(input: &StudyGenInput) -> Result<Box<dyn crate::provider::Provider>, String> {
    crate::commands::provider::build_provider_with_stream_options(
        &input.provider_id,
        &input.model,
        input.custom_base_url.as_deref(),
        input.custom_display_name.as_deref(),
        true,
    )
}

/// 生成(或命中缓存)某知识点的自学材料。
#[tauri::command]
pub async fn study_unit_generate(input: StudyGenInput) -> Result<learn::UnitRow, String> {
    memory::validate_id(&input.id)?;
    let provider = gen_provider(&input)?;
    learn::generate_unit(
        provider,
        &input.model,
        &input.id,
        input.node_id,
        input.free_form.unwrap_or(false),
        input.force.unwrap_or(false),
    )
    .await
}

/// 为某知识点出一卷新测验(每次新卷)。
#[tauri::command]
pub async fn study_quiz_new(input: StudyGenInput) -> Result<learn::QuizRow, String> {
    memory::validate_id(&input.id)?;
    let provider = gen_provider(&input)?;
    learn::new_quiz(provider, &input.model, &input.id, input.node_id).await
}

/// 提交测验:客观题确定性判分,简答题自评;返回逐题结果与重算后的掌握度。
#[tauri::command]
pub fn study_quiz_submit(
    id: String,
    quiz_id: i64,
    answers: Vec<learn::QuizAnswerIn>,
) -> Result<learn::QuizResult, String> {
    memory::validate_id(&id)?;
    learn::quiz_submit(&id, quiz_id, &answers)
}

/// 自评掌握(0 / 0.5 / 1 三档);评「掌握」同时置 done_at。返回重算后的 mastery。
#[tauri::command]
pub fn study_mastery_set(id: String, node_id: i64, rating: f64) -> Result<f64, String> {
    memory::validate_id(&id)?;
    learn::set_mastery(&id, node_id, rating)
}

/// 标记知识点完成/取消完成。
#[tauri::command]
pub fn study_node_done(id: String, node_id: i64, done: bool) -> Result<(), String> {
    memory::validate_id(&id)?;
    learn::node_done(&id, node_id, done)
}

/// 到期复习队列(due <= 今天)。
#[tauri::command]
pub fn study_review_queue(id: String) -> Result<Vec<learn::ReviewItem>, String> {
    memory::validate_id(&id)?;
    learn::review_queue(&id)
}

/// 复习打卡:答对晋级一档,答错回退 1 天。返回更新后的条目。
#[tauri::command]
pub fn study_review_done(id: String, node_id: i64, correct: bool) -> Result<learn::ReviewItem, String> {
    memory::validate_id(&id)?;
    learn::review_done(&id, node_id, correct)
}

/// 今日待办:当前排期周内未完成知识点 + 到期复习。
#[tauri::command]
pub fn study_today(id: String) -> Result<learn::TodayPlan, String> {
    memory::validate_id(&id)?;
    learn::today(&id)
}

// ---------- P2:专注计时 / 统计 / 记忆注入 ----------

/// 专注计时结束:分钟数落 study_logs(kind='focus'),构成当日打卡。
#[tauri::command]
pub fn study_focus_log(id: String, node_id: Option<i64>, minutes: i64) -> Result<(), String> {
    learn::focus_log(&id, node_id, minutes)
}

/// 计划级统计:连续天数 / 累计时长 / 完成度 / 掌握度均值 / 最近 35 天打卡网格。
#[tauri::command]
pub fn study_stats(id: String) -> Result<learn::StudyStats, String> {
    memory::validate_id(&id)?;
    learn::stats(&id)
}

/// 学习进度记忆摘要(会话 system 注入用);无进行中计划返回 None。
#[tauri::command]
pub fn study_memory_digest() -> Result<Option<String>, String> {
    learn::memory_digest()
}
