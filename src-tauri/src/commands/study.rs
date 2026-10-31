//! 学习规划相关 Tauri commands(P0 规划 + P1 学习执行)。
//!
//! 命令族:study_preflight(意图澄清)/ study_start / study_cancel / study_pause /
//! study_resume / study_list / study_get / study_export_markdown / study_delete /
//! study_deep_dig(深挖研究联动);
//! P1:study_nodes / study_unit_generate / study_quiz_new / study_quiz_submit /
//! study_mastery_set / study_node_done / study_review_queue / study_review_done / study_today。

use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};
use tauri::State;
use tauri::ipc::Channel;

use crate::memory;
use crate::study::{self, Intent, PlanPayload, StudyEvent, learn, store};

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

// ---------- 查询 / 删除 ----------

#[derive(Debug, Serialize)]
pub struct StudyDetail {
    pub plan: store::PlanRow,
    /// 生成的计划载荷(尚未生成/失败为 None)
    pub payload: Option<PlanPayload>,
    pub trail: Vec<StudyEvent>,
}

#[tauri::command]
pub async fn study_list() -> Result<Vec<store::PlanRow>, String> {
    store::list_plans()
}

#[tauri::command]
pub async fn study_get(id: String) -> Result<StudyDetail, String> {
    memory::validate_id(&id)?;
    let plan = store::get_plan(&id)?.ok_or("学习计划不存在")?;
    let trail = store::read_trail(&id)?;
    let payload = store::read_payload(&id);
    Ok(StudyDetail {
        plan,
        payload,
        trail,
    })
}

/// 删除学习计划:运行中先终止,等引擎自行退出(≤5s)后删行删目录。
#[tauri::command]
pub async fn study_delete(id: String, registry: State<'_, StudyRegistry>) -> Result<(), String> {
    memory::validate_id(&id)?;
    // 引擎退出时会把自己从注册表移除;以此判断是否仍在运行
    let running = |reg: &StudyRegistry| reg.0.lock().map(|m| m.contains_key(&id)).unwrap_or(false);
    if running(&registry) {
        if let Ok(m) = registry.0.lock()
            && let Some(entry) = m.get(&id)
        {
            entry.ctl.cancel.store(true, Ordering::Relaxed);
            entry.ctl.paused.store(false, Ordering::Relaxed);
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

/// 记录一次「深入研究」:study_logs 落 kind=research 行,返回预填给新会话的主题。
#[tauri::command]
pub async fn study_deep_dig(id: String, node_name: String) -> Result<String, String> {
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
/// token 为 pick_save_path 签发的路径令牌(复用通用导出的 N-6 机制)。
#[tauri::command]
pub async fn study_export_markdown(
    id: String,
    token: String,
    overwrite: bool,
    tokens: State<'_, crate::export::ExportPathTokens>,
) -> Result<String, String> {
    memory::validate_id(&id)?;
    let target = tokens.resolve(&token)?;
    if target
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        != Some("md".into())
    {
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
pub async fn study_nodes(id: String) -> Result<Vec<learn::NodeState>, String> {
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
    /// 错题变式再练(P3):上一轮答错题的题干与答案汇总
    pub wrong_context: Option<String>,
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
        provider.as_ref(),
        &input.model,
        &input.id,
        input.node_id,
        input.free_form.unwrap_or(false),
        input.force.unwrap_or(false),
    )
    .await
}

/// 为某知识点出一卷新测验(每次新卷;带 wrong_context 时出错题变式)。
#[tauri::command]
pub async fn study_quiz_new(input: StudyGenInput) -> Result<learn::QuizRow, String> {
    memory::validate_id(&input.id)?;
    let provider = gen_provider(&input)?;
    learn::new_quiz(
        provider,
        &input.model,
        &input.id,
        input.node_id,
        input.wrong_context.as_deref(),
    )
    .await
}

/// 提交测验:客观题确定性判分,简答题自评;返回逐题结果与重算后的掌握度。
#[tauri::command]
pub async fn study_quiz_submit(
    id: String,
    quiz_id: i64,
    answers: Vec<learn::QuizAnswerIn>,
) -> Result<learn::QuizResult, String> {
    memory::validate_id(&id)?;
    learn::quiz_submit(&id, quiz_id, &answers)
}

/// 自评掌握(0 / 0.5 / 1 三档);评「掌握」同时置 done_at。返回重算后的 mastery。
#[tauri::command]
pub async fn study_mastery_set(id: String, node_id: i64, rating: f64) -> Result<f64, String> {
    memory::validate_id(&id)?;
    learn::set_mastery(&id, node_id, rating)
}

/// 标记知识点完成/取消完成。
#[tauri::command]
pub async fn study_node_done(id: String, node_id: i64, done: bool) -> Result<(), String> {
    memory::validate_id(&id)?;
    learn::node_done(&id, node_id, done)
}

/// 到期复习队列(due <= 今天)。
#[tauri::command]
pub async fn study_review_queue(id: String) -> Result<Vec<learn::ReviewItem>, String> {
    memory::validate_id(&id)?;
    learn::review_queue(&id)
}

/// 复习打卡:答对晋级一档,答错回退 1 天。返回更新后的条目。
#[tauri::command]
pub async fn study_review_done(
    id: String,
    node_id: i64,
    correct: bool,
) -> Result<learn::ReviewItem, String> {
    memory::validate_id(&id)?;
    learn::review_done(&id, node_id, correct)
}

/// 今日待办:当前排期周内未完成知识点 + 到期复习。
#[tauri::command]
pub async fn study_today(id: String) -> Result<learn::TodayPlan, String> {
    memory::validate_id(&id)?;
    learn::today(&id)
}

// ---------- P2:每日任务 ----------

/// 今日任务:未完成的到期任务(过期浮到今天)+ 今天已完成的。
#[tauri::command]
pub async fn study_tasks_today(id: String) -> Result<Vec<learn::StudyTask>, String> {
    memory::validate_id(&id)?;
    learn::tasks_today(&id)
}

/// 勾选/取消勾选一条每日任务(完成写学习日志,分钟计入统计)。
#[tauri::command]
pub async fn study_task_done(id: String, task_id: i64, done: bool) -> Result<(), String> {
    memory::validate_id(&id)?;
    learn::task_done(&id, task_id, done)
}

// ---------- P2:专注计时 / 统计 / 记忆注入 ----------

/// 专注计时结束:分钟数落 study_logs(kind='focus'),构成当日打卡。
#[tauri::command]
pub async fn study_focus_log(id: String, node_id: Option<i64>, minutes: i64) -> Result<(), String> {
    learn::focus_log(&id, node_id, minutes)
}

/// 计划级统计:连续天数 / 累计时长 / 完成度 / 掌握度均值 / 最近 35 天打卡网格。
#[tauri::command]
pub async fn study_stats(id: String) -> Result<learn::StudyStats, String> {
    memory::validate_id(&id)?;
    learn::stats(&id)
}

/// 学习进度记忆摘要(会话 system 注入用);无进行中计划返回 None。
#[tauri::command]
pub async fn study_memory_digest() -> Result<Option<String>, String> {
    learn::memory_digest()
}

// ---------- P4:学习中心(试题库 / 试卷 / 批改) ----------

/// 题库筛选入参。
#[derive(Debug, Deserialize)]
pub struct BankListInput {
    pub plan_id: Option<String>,
    pub qtype: Option<String>,
    pub keyword: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[tauri::command]
pub async fn study_bank_list(input: BankListInput) -> Result<learn::BankListResult, String> {
    learn::bank::list(
        input.plan_id.as_deref(),
        input.qtype.as_deref(),
        input.keyword.as_deref(),
        input.limit.unwrap_or(50),
        input.offset.unwrap_or(0),
    )
}

/// 手动加题(同计划同题干去重)。
#[tauri::command]
pub async fn study_bank_add(input: learn::BankAddInput) -> Result<learn::BankQuestion, String> {
    learn::bank::add(&input)
}

/// 编辑题库题目内容。
#[tauri::command]
pub async fn study_bank_update(
    id: i64,
    input: learn::BankUpdateInput,
) -> Result<learn::BankQuestion, String> {
    learn::bank::update(id, &input)
}

/// 删除题库题目(已出试卷保存快照,不受影响)。
#[tauri::command]
pub async fn study_bank_delete(id: i64) -> Result<(), String> {
    learn::bank::delete(id)
}

/// 试卷列表(plan_id 为空 = 全部)。
#[tauri::command]
pub async fn study_paper_list(plan_id: Option<String>) -> Result<Vec<learn::PaperRow>, String> {
    learn::paper::list(plan_id.as_deref())
}

/// 试卷详情(含题目快照、作答与批改结果)。
#[tauri::command]
pub async fn study_paper_get(id: String) -> Result<learn::PaperDetail, String> {
    learn::paper::detail(&id)
}

/// 自选题组卷。
#[derive(Debug, Deserialize)]
pub struct PaperCreateInput {
    pub plan_id: Option<String>,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub question_ids: Vec<i64>,
}

#[tauri::command]
pub async fn study_paper_create(
    input: PaperCreateInput,
) -> Result<learn::PaperDetail, String> {
    learn::paper::create_manual(
        input.plan_id.as_deref(),
        input.title.as_deref(),
        input.kind.as_deref(),
        &input.question_ids,
    )
}

/// 随机组卷入参(按计划 / 知识点筛选,不足时明确报错)。
#[derive(Debug, Deserialize)]
pub struct PaperAutoInput {
    pub plan_id: Option<String>,
    #[serde(default)]
    pub node_ids: Vec<i64>,
    pub count: u32,
    pub title: Option<String>,
    pub kind: Option<String>,
}

#[tauri::command]
pub async fn study_paper_auto(input: PaperAutoInput) -> Result<learn::PaperDetail, String> {
    learn::paper::create_auto(
        input.plan_id.as_deref(),
        &input.node_ids,
        input.count,
        input.title.as_deref(),
        input.kind.as_deref(),
    )
}

/// 整卷提交批改入参。
#[derive(Debug, Deserialize)]
pub struct PaperSubmitInput {
    pub paper_id: String,
    pub answers: Vec<learn::PaperAnswerIn>,
    pub provider_id: String,
    pub model: String,
    pub custom_base_url: Option<String>,
    pub custom_display_name: Option<String>,
}

/// 整卷批改:先落作答再逐题判分(客观题本地、主观题按 rubric),返回汇总。
#[tauri::command]
pub async fn study_paper_submit(input: PaperSubmitInput) -> Result<learn::PaperResult, String> {
    let provider = crate::commands::provider::build_provider_with_stream_options(
        &input.provider_id,
        &input.model,
        input.custom_base_url.as_deref(),
        input.custom_display_name.as_deref(),
        true,
    )?;
    learn::paper::submit(
        provider.as_ref(),
        &input.model,
        &input.paper_id,
        &input.answers,
    )
    .await
}

/// 删除试卷。
#[tauri::command]
pub async fn study_paper_delete(id: String) -> Result<(), String> {
    learn::paper::delete(&id)
}

/// 单题批改入参(主观题由用户点「批改此题」触发)。
#[derive(Debug, Deserialize)]
pub struct QuestionGradeInput {
    /// 题库题目 id(优先);为空时用下方题目字段
    pub question_id: Option<i64>,
    pub qtype: Option<String>,
    pub stem: Option<String>,
    pub answer: Option<String>,
    pub explanation: Option<String>,
    pub user_answer: String,
    pub provider_id: String,
    pub model: String,
    pub custom_base_url: Option<String>,
    pub custom_display_name: Option<String>,
}

#[tauri::command]
pub async fn study_question_grade(
    input: QuestionGradeInput,
) -> Result<learn::GradeOutcome, String> {
    let (qtype, stem, answer, explanation) = match input.question_id {
        Some(id) => {
            let q = learn::bank::get(id)?;
            (q.qtype, q.stem, q.answer, q.explanation)
        }
        None => {
            let qtype = input.qtype.unwrap_or_else(|| "short".into());
            let stem = input.stem.unwrap_or_default();
            let answer = input.answer.unwrap_or_default();
            if stem.trim().is_empty() || answer.trim().is_empty() {
                return Err("题干与参考答案不能为空".into());
            }
            (
                qtype,
                stem,
                answer,
                input.explanation.unwrap_or_default(),
            )
        }
    };
    let provider = crate::commands::provider::build_provider_with_stream_options(
        &input.provider_id,
        &input.model,
        input.custom_base_url.as_deref(),
        input.custom_display_name.as_deref(),
        true,
    )?;
    learn::paper::question_grade(
        provider.as_ref(),
        &input.model,
        &qtype,
        &stem,
        &answer,
        &explanation,
        &input.user_answer,
    )
    .await
}
