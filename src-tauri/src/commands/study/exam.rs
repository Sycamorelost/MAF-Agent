//! P4:学习中心(试题库 / 试卷 / 批改)。

use serde::Deserialize;

use crate::study::learn;

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
