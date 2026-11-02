//! P1:学习执行(自学材料 / 测验 / 掌握度 / 复习 / 每日任务 / 专注 / 统计 / 记忆注入)。

use serde::Deserialize;

use crate::memory;
use crate::study::learn;

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
