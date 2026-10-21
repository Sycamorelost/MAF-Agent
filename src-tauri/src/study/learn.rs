//! 学习执行闭环(P1):自学材料 / 测验 / 掌握度 / SM-2-lite 复习 / 今日待办。
//!
//! - 自学材料:每知识点一份 Markdown(study_units,UNIQUE 幂等缓存,支持 force 重新生成);
//! - 测验:每知识点 5 题(选择/判断/简答),客观题 Rust 确定性判分,简答题用户自评;
//! - 掌握度:mastery = 自评 ×0.4 + 最新测验正确率 ×0.6(单项缺失时取另一项);
//! - 复习:SM-2-lite 间隔序列 1/3/7/14/30 天,答对晋级、答错回退到 1 天;
//! - 今日待办:按 plans.created 推算当前排期周 → 该周阶段内未完成知识点 + 到期复习。
//!
//! 纯逻辑(判分/加权/间隔/周推算)与 DB 操作(conn 入参)分离,便于单测。

use chrono::{Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};

use super::{llm_plain, level_label, sys_msg, user_msg};
use crate::memory;
use crate::provider::Provider;

/// SM-2-lite 间隔序列(天)。
pub const INTERVALS: [i64; 5] = [1, 3, 7, 14, 30];

fn now_str() -> String {
    Local::now().to_rfc3339()
}

fn today_str() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn plus_days(days: i64) -> String {
    (Local::now().date_naive() + Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

// ---------------- 纯逻辑(可测) ----------------

/// 掌握度加权:自评 ×0.4 + 测验 ×0.6;-1 表示未评,单项缺失时取另一项。
pub fn weighted_mastery(self_rating: f64, quiz_score: f64) -> f64 {
    match (self_rating >= 0.0, quiz_score >= 0.0) {
        (true, true) => self_rating * 0.4 + quiz_score * 0.6,
        (true, false) => self_rating,
        (false, true) => quiz_score,
        (false, false) => 0.0,
    }
}

/// SM-2-lite:答对晋级一档(封顶 30 天),答错回退 1 天;未知当前间隔从 3 天起步。
pub fn next_interval(current: i64, correct: bool) -> i64 {
    if !correct {
        return INTERVALS[0];
    }
    match INTERVALS.iter().position(|&i| i == current) {
        Some(p) if p + 1 < INTERVALS.len() => INTERVALS[p + 1],
        Some(_) => INTERVALS[INTERVALS.len() - 1],
        None => INTERVALS[1],
    }
}

/// 由计划创建日推算当前排期周(1 起),clamp 到 [1, total_weeks];无排期返回 0。
pub fn current_week(created: NaiveDate, today: NaiveDate, total_weeks: usize) -> i64 {
    if total_weeks == 0 {
        return 0;
    }
    let days = (today - created).num_days();
    (days / 7 + 1).clamp(1, total_weeks as i64)
}

/// choice 判分:取首个 ASCII 字母比较(容忍「A. xxx」/「正确答案:B」等写法)。
fn norm_letter(s: &str) -> Option<char> {
    s.trim()
        .chars()
        .find(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
}

/// judge 判分:归一到 对/错 两个布尔。
fn norm_judge(s: &str) -> Option<bool> {
    let t = s.trim().to_lowercase();
    if ["对", "正确", "true", "√", "是", "t", "yes"].iter().any(|k| t == *k)
        || t.starts_with("对")
        || t.starts_with("正确")
    {
        return Some(true);
    }
    if ["错", "错误", "false", "×", "x", "否", "f", "no"].iter().any(|k| t == *k)
        || t.starts_with("错")
    {
        return Some(false);
    }
    None
}

/// 单题判分:choice/judge 客观比对;short 由用户自评(self_correct)。
pub fn grade(qtype: &str, answer: &str, user_answer: &str, self_correct: Option<bool>) -> bool {
    match qtype {
        "choice" => match (norm_letter(answer), norm_letter(user_answer)) {
            (Some(a), Some(u)) => a == u,
            _ => answer.trim() == user_answer.trim(),
        },
        "judge" => match (norm_judge(answer), norm_judge(user_answer)) {
            (Some(a), Some(u)) => a == u,
            _ => answer.trim() == user_answer.trim(),
        },
        _ => self_correct.unwrap_or(false),
    }
}

/// 剥掉包裹全文的最外层代码围栏(不碰正文内的代码块)。
pub fn strip_outer_fence(text: &str) -> String {
    let s = text.trim();
    let first = s.lines().next().unwrap_or("");
    if !first.starts_with("```") || !s.trim_end().ends_with("```") {
        return s.to_string();
    }
    let body = s.strip_prefix(first).unwrap_or(s);
    let body = body.strip_suffix("```").unwrap_or(body);
    body.trim().to_string()
}

/// 解析测验输出:重排 idx、剔除无题干/答案项、judge 布尔答案归一、截到 5 题;有效 <3 题判失败。
pub fn parse_quiz(text: &str) -> Option<Vec<QuizQuestion>> {
    let json = super::extract_json(text)?;
    let v: serde_json::Value = serde_json::from_str(&json).ok()?;
    let arr = v.get("questions")?.as_array()?;
    let mut out: Vec<QuizQuestion> = Vec::new();
    for item in arr {
        let stem = item
            .get("stem")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let answer = match item.get("answer") {
            Some(serde_json::Value::Bool(b)) => {
                if *b {
                    "对".to_string()
                } else {
                    "错".to_string()
                }
            }
            Some(serde_json::Value::String(s)) => s.trim().to_string(),
            _ => String::new(),
        };
        if stem.is_empty() || answer.is_empty() {
            continue;
        }
        let qtype = match item.get("type").and_then(|x| x.as_str()).unwrap_or("") {
            "choice" => "choice",
            "judge" => "judge",
            _ => "short",
        }
        .to_string();
        let options = item
            .get("options")
            .and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let explanation = item
            .get("explanation")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        out.push(QuizQuestion {
            idx: out.len() as u32,
            qtype,
            stem,
            options,
            answer,
            explanation,
        });
        if out.len() >= 5 {
            break;
        }
    }
    if out.len() >= 3 {
        Some(out)
    } else {
        None
    }
}

// ---------------- 行类型 ----------------

/// 自学单元行。
#[derive(Debug, Clone, Serialize)]
pub struct UnitRow {
    pub id: i64,
    pub plan_id: String,
    pub node_id: i64,
    pub node_name: String,
    pub title: String,
    pub est_minutes: i64,
    pub content_md: String,
    pub created: String,
}

/// 测验题目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizQuestion {
    pub idx: u32,
    #[serde(rename = "type")]
    pub qtype: String, // choice / judge / short
    pub stem: String,
    #[serde(default)]
    pub options: Vec<String>,
    pub answer: String,
    #[serde(default)]
    pub explanation: String,
}

/// 测验行(题目载荷已解析)。
#[derive(Debug, Clone, Serialize)]
pub struct QuizRow {
    pub id: i64,
    pub plan_id: String,
    pub node_id: i64,
    pub node_name: String,
    pub questions: Vec<QuizQuestion>,
    pub created: String,
}

/// 复习队列条目。
#[derive(Debug, Clone, Serialize)]
pub struct ReviewItem {
    pub id: i64,
    pub node_id: i64,
    pub node_name: String,
    pub due: String,
    pub interval_days: i64,
    pub last_result: i64,
    /// due 早于今天
    pub overdue: bool,
}

/// 知识点落库状态(前端阶段树合并 payload 用)。
#[derive(Debug, Clone, Serialize)]
pub struct NodeState {
    pub node_id: i64,
    pub stage_index: i64,
    pub name: String,
    pub difficulty: i64,
    pub why: String,
    pub must_learn: bool,
    pub mastery: f64,
    pub done: bool,
    pub has_unit: bool,
}

/// 今日待办 - 应学知识点。
#[derive(Debug, Clone, Serialize)]
pub struct TodayItem {
    pub node_id: i64,
    pub node_name: String,
    pub stage_index: i64,
    pub difficulty: i64,
    pub mastery: f64,
}

/// 今日待办聚合。
#[derive(Debug, Clone, Serialize)]
pub struct TodayPlan {
    /// 当前排期周(1 起;无排期 = 0)
    pub week: i64,
    pub total_weeks: usize,
    pub stage_names: Vec<String>,
    pub nodes: Vec<TodayItem>,
    pub reviews: Vec<ReviewItem>,
}

// ---------------- 判分入参/结果 ----------------

#[derive(Debug, Deserialize)]
pub struct QuizAnswerIn {
    pub question_idx: u32,
    #[serde(default)]
    pub user_answer: String,
    /// short 题自评(true = 记得 / false = 没记住)
    #[serde(default)]
    pub self_correct: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuizGrade {
    pub question_idx: u32,
    pub correct: bool,
    pub expected: String,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuizResult {
    pub score: f64,
    pub mastery: f64,
    pub results: Vec<QuizGrade>,
}

// ---------------- 生成(单元 / 测验) ----------------

fn conn() -> Result<std::sync::MutexGuard<'static, rusqlite::Connection>, String> {
    crate::db::conn().lock().map_err(|e| e.to_string())
}

/// 读取计划与知识点的生成上下文;已缓存单元直接返回。
fn unit_context(
    conn: &rusqlite::Connection,
    plan_id: &str,
    node_id: i64,
) -> Result<(String, String, String, String, i64, String, Option<UnitRow>), String> {
    let (topic, level, lang): (String, String, String) = conn
        .query_row(
            "SELECT topic, level, lang FROM plans WHERE id=?1",
            [plan_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| "学习计划不存在".to_string())?;
    let (name, difficulty, why): (String, i64, String) = conn
        .query_row(
            "SELECT name, difficulty, why FROM knowledge_nodes WHERE plan_id=?1 AND id=?2",
            rusqlite::params![plan_id, node_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| "知识点不存在".to_string())?;
    let existing = unit_get_conn(conn, plan_id, node_id)?;
    Ok((topic, level, lang, name, difficulty, why, existing))
}

fn unit_get_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
    node_id: i64,
) -> Result<Option<UnitRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, plan_id, node_id, node_name, title, est_minutes, content_md, created
             FROM study_units WHERE plan_id=?1 AND node_id=?2",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map(rusqlite::params![plan_id, node_id], |r| {
            Ok(UnitRow {
                id: r.get(0)?,
                plan_id: r.get(1)?,
                node_id: r.get(2)?,
                node_name: r.get(3)?,
                title: r.get(4)?,
                est_minutes: r.get(5)?,
                content_md: r.get(6)?,
                created: r.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    match rows.next() {
        Some(Ok(u)) => Ok(Some(u)),
        Some(Err(e)) => Err(e.to_string()),
        None => Ok(None),
    }
}

/// 生成(或命中缓存)一个知识点的自学材料。free_form = 不套固定小节结构。
pub async fn generate_unit(
    provider: Box<dyn Provider>,
    model: &str,
    plan_id: &str,
    node_id: i64,
    free_form: bool,
    force: bool,
) -> Result<UnitRow, String> {
    let (topic, level, lang, name, difficulty, why, existing) = {
        let c = conn()?;
        unit_context(&c, plan_id, node_id)?
    };
    if let Some(u) = existing {
        if !force {
            return Ok(u);
        }
        let c = conn()?;
        c.execute(
            "DELETE FROM study_units WHERE plan_id=?1 AND node_id=?2",
            rusqlite::params![plan_id, node_id],
        )
        .map_err(|e| e.to_string())?;
    }

    let lang_label = if lang == "en" { "English" } else { "中文" };
    let mut sys = super::template::effective("study_guide");
    if free_form {
        sys.push_str(
            "\n\n补充要求:不要套用固定小节结构,按内容自然组织行文,但仍使用 Markdown 格式与代码围栏规范。",
        );
    }
    let user = format!(
        "学习主题:{}\n当前水平:{}\n知识点:{}(难度 {}/5)\n为什么重要:{}\n输出语言:{}",
        topic,
        level_label(&level),
        name,
        difficulty,
        if why.is_empty() { "未说明" } else { &why },
        lang_label
    );
    let text = llm_plain(provider.as_ref(), model, vec![sys_msg(sys), user_msg(user)]).await?;
    let content = strip_outer_fence(&text);
    if content.trim().is_empty() {
        return Err("模型未返回有效内容".into());
    }

    let c = conn()?;
    let mut row = UnitRow {
        id: 0,
        plan_id: plan_id.to_string(),
        node_id,
        node_name: name.clone(),
        title: name.clone(),
        est_minutes: 20 + difficulty * 10,
        content_md: content,
        created: now_str(),
    };
    c.execute(
        "INSERT INTO study_units(plan_id, node_id, node_name, title, est_minutes, content_md, created)
         VALUES(?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(plan_id, node_id) DO UPDATE SET
           node_name=?3, title=?4, est_minutes=?5, content_md=?6, created=?7",
        rusqlite::params![
            row.plan_id,
            row.node_id,
            row.node_name,
            row.title,
            row.est_minutes,
            row.content_md,
            row.created,
        ],
    )
    .map_err(|e| format!("save unit failed: {e}"))?;
    row.id = c.last_insert_rowid();
    super::store::add_log(&c, plan_id, Some(node_id), "learn", 0)?;
    Ok(row)
}

/// 生成(不缓存,每次新卷)一个知识点的测验;解析失败带反馈重试 1 次。
pub async fn new_quiz(
    provider: Box<dyn Provider>,
    model: &str,
    plan_id: &str,
    node_id: i64,
) -> Result<QuizRow, String> {
    let (topic, level, lang, name, difficulty, why, _) = {
        let c = conn()?;
        unit_context(&c, plan_id, node_id)?
    };
    let lang_label = if lang == "en" { "English" } else { "中文" };
    let sys = super::template::effective("study_quiz");
    let base_user = format!(
        "学习主题:{}\n当前水平:{}\n知识点:{}(难度 {}/5)\n要点:{}\n输出语言:{}",
        topic,
        level_label(&level),
        name,
        difficulty,
        if why.is_empty() { "未说明" } else { &why },
        lang_label
    );

    let mut last_err = String::new();
    let mut questions = None;
    for attempt in 1..=2u32 {
        let user = if attempt == 1 {
            base_user.clone()
        } else {
            format!("{base_user}\n\n上一轮输出存在问题:{last_err}\n必须只输出约定 Schema 的 JSON,恰好 5 题。")
        };
        let text = llm_plain(
            provider.as_ref(),
            model,
            vec![sys_msg(sys.clone()), user_msg(user)],
        )
        .await?;
        match parse_quiz(&text) {
            Some(qs) => {
                questions = Some(qs);
                break;
            }
            None => last_err = "不是合法的 {\"questions\":[...]} JSON 或有效题目不足 3 道".into(),
        }
    }
    let questions = questions.ok_or_else(|| format!("测验生成失败:{last_err}"))?;

    let c = conn()?;
    let payload = serde_json::to_string(&questions).map_err(|e| e.to_string())?;
    c.execute(
        "INSERT INTO quizzes(plan_id, node_id, node_name, payload_json, created) VALUES(?1,?2,?3,?4,?5)",
        rusqlite::params![plan_id, node_id, name, payload, now_str()],
    )
    .map_err(|e| format!("save quiz failed: {e}"))?;
    Ok(QuizRow {
        id: c.last_insert_rowid(),
        plan_id: plan_id.to_string(),
        node_id,
        node_name: name,
        questions,
        created: now_str(),
    })
}

// ---------------- 查询 / 判分 / 掌握度 ----------------

/// 全部知识点的落库状态(阶段树合并用)。
pub fn nodes_state(plan_id: &str) -> Result<Vec<NodeState>, String> {
    let c = conn()?;
    nodes_state_conn(&c, plan_id)
}

pub(crate) fn nodes_state_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
) -> Result<Vec<NodeState>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.stage_index, n.name, n.difficulty, n.why, n.must_learn, n.mastery, n.done_at,
                    EXISTS(SELECT 1 FROM study_units u WHERE u.plan_id = n.plan_id AND u.node_id = n.id)
             FROM knowledge_nodes n WHERE n.plan_id=?1
             ORDER BY n.stage_index, n.difficulty, n.id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([plan_id], |r| {
            let done_at: Option<String> = r.get(7)?;
            Ok(NodeState {
                node_id: r.get(0)?,
                stage_index: r.get(1)?,
                name: r.get(2)?,
                difficulty: r.get(3)?,
                why: r.get(4)?,
                must_learn: r.get::<_, i64>(5)? != 0,
                mastery: r.get(6)?,
                done: done_at.is_some(),
                has_unit: r.get::<_, i64>(8)? != 0,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

fn quiz_row_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
    quiz_id: i64,
) -> Result<(String, i64, String, Vec<QuizQuestion>), String> {
    let (pid, node_id, node_name, payload): (String, i64, String, String) = conn
        .query_row(
            "SELECT plan_id, node_id, node_name, payload_json FROM quizzes WHERE id=?1 AND plan_id=?2",
            rusqlite::params![quiz_id, plan_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|_| "测验不存在".to_string())?;
    let questions: Vec<QuizQuestion> =
        serde_json::from_str(&payload).map_err(|e| format!("测验数据损坏:{e}"))?;
    Ok((pid, node_id, node_name, questions))
}

/// 提交测验:客观题确定性判分,简答题自评;落作答历史、更新测验分与掌握度、排复习。
pub fn quiz_submit(plan_id: &str, quiz_id: i64, answers: &[QuizAnswerIn]) -> Result<QuizResult, String> {
    let c = conn()?;
    quiz_submit_conn(&c, plan_id, quiz_id, answers)
}

pub(crate) fn quiz_submit_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
    quiz_id: i64,
    answers: &[QuizAnswerIn],
) -> Result<QuizResult, String> {
    let (_, node_id, _name, questions) = quiz_row_conn(conn, plan_id, quiz_id)?;
    if questions.is_empty() {
        return Err("测验题目为空".into());
    }
    let now = now_str();
    let mut results = Vec::new();
    let mut correct_n = 0usize;
    for q in &questions {
        let (ua, sc) = answers
            .iter()
            .find(|a| a.question_idx == q.idx)
            .map(|a| (a.user_answer.clone(), a.self_correct))
            .unwrap_or((String::new(), None));
        let ok = grade(&q.qtype, &q.answer, &ua, sc);
        if ok {
            correct_n += 1;
        }
        conn.execute(
            "INSERT INTO quiz_attempts(quiz_id, question_idx, user_answer, correct, answered_at)
             VALUES(?1,?2,?3,?4,?5)",
            rusqlite::params![quiz_id, q.idx, ua, ok as i64, now],
        )
        .map_err(|e| format!("save attempt failed: {e}"))?;
        results.push(QuizGrade {
            question_idx: q.idx,
            correct: ok,
            expected: q.answer.clone(),
            explanation: q.explanation.clone(),
        });
    }
    let score = correct_n as f64 / questions.len() as f64;

    // 更新测验分与掌握度(掌握度 = 自评×0.4 + 测验×0.6)
    let sr: f64 = conn
        .query_row(
            "SELECT self_rating FROM knowledge_nodes WHERE plan_id=?1 AND id=?2",
            rusqlite::params![plan_id, node_id],
            |r| r.get(0),
        )
        .unwrap_or(-1.0);
    let mastery = weighted_mastery(sr, score);
    conn.execute(
        "UPDATE knowledge_nodes SET quiz_score=?1, mastery=?2 WHERE plan_id=?3 AND id=?4",
        rusqlite::params![score, mastery, plan_id, node_id],
    )
    .map_err(|e| format!("update quiz_score failed: {e}"))?;

    // 复习排程:高分晋级,低分回退到明天
    if score >= 0.8 {
        let cur: Option<i64> = conn
            .query_row(
                "SELECT interval_days FROM reviews WHERE plan_id=?1 AND node_id=?2",
                rusqlite::params![plan_id, node_id],
                |r| r.get(0),
            )
            .ok();
        let next = match cur {
            Some(c) => next_interval(c, true),
            None => INTERVALS[1],
        };
        upsert_review_conn(conn, &plan_id, node_id, &plus_days(next), next, 1)?;
    } else {
        upsert_review_conn(conn, &plan_id, node_id, &plus_days(INTERVALS[0]), INTERVALS[0], 0)?;
    }

    super::store::add_log(conn, &plan_id, Some(node_id), "quiz", 0)?;
    Ok(QuizResult { score, mastery, results })
}

/// 自评掌握(0 / 0.5 / 1 三档,clamp);评「掌握」同时置 done_at。返回重算后的 mastery。
pub fn set_mastery(plan_id: &str, node_id: i64, rating: f64) -> Result<f64, String> {
    let c = conn()?;
    set_mastery_conn(&c, plan_id, node_id, rating)
}

pub(crate) fn set_mastery_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
    node_id: i64,
    rating: f64,
) -> Result<f64, String> {
    let rating = rating.clamp(0.0, 1.0);
    let n = conn
        .execute(
            "UPDATE knowledge_nodes SET self_rating=?1,
                    done_at = CASE WHEN ?1 >= 1.0 THEN ?2 ELSE done_at END
             WHERE plan_id=?3 AND id=?4",
            rusqlite::params![rating, now_str(), plan_id, node_id],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("知识点不存在".into());
    }
    let (sr, qs): (f64, f64) = conn
        .query_row(
            "SELECT self_rating, quiz_score FROM knowledge_nodes WHERE plan_id=?1 AND id=?2",
            rusqlite::params![plan_id, node_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let mastery = weighted_mastery(sr, qs);
    conn.execute(
        "UPDATE knowledge_nodes SET mastery=?1 WHERE plan_id=?2 AND id=?3",
        rusqlite::params![mastery, plan_id, node_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(mastery)
}

/// 标记知识点完成/取消完成。
pub fn node_done(plan_id: &str, node_id: i64, done: bool) -> Result<(), String> {
    let c = conn()?;
    let n = c
        .execute(
            "UPDATE knowledge_nodes SET done_at=?1 WHERE plan_id=?2 AND id=?3",
            rusqlite::params![if done { Some(now_str()) } else { None }, plan_id, node_id],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("知识点不存在".into());
    }
    if done {
        super::store::add_log(&c, plan_id, Some(node_id), "learn", 0)?;
    }
    Ok(())
}

// ---------------- SM-2-lite 复习 ----------------

fn upsert_review_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
    node_id: i64,
    due: &str,
    interval_days: i64,
    last_result: i64,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO reviews(plan_id, node_id, due, interval_days, last_result)
         VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(plan_id, node_id) DO UPDATE SET due=?3, interval_days=?4, last_result=?5",
        rusqlite::params![plan_id, node_id, due, interval_days, last_result],
    )
    .map_err(|e| format!("upsert review failed: {e}"))?;
    Ok(())
}

fn review_item_from(row: (i64, i64, String, String, i64, i64)) -> ReviewItem {
    let overdue = row.3.as_str() < today_str().as_str();
    ReviewItem {
        id: row.0,
        node_id: row.1,
        node_name: row.2,
        due: row.3,
        interval_days: row.4,
        last_result: row.5,
        overdue,
    }
}

/// 到期复习队列(due <= 今天,按到期日升序)。
pub fn review_queue(plan_id: &str) -> Result<Vec<ReviewItem>, String> {
    let c = conn()?;
    review_queue_conn(&c, plan_id)
}

pub(crate) fn review_queue_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
) -> Result<Vec<ReviewItem>, String> {
    let today = today_str();
    let mut stmt = conn
        .prepare(
            "SELECT r.id, r.node_id, COALESCE(n.name, '(已删除知识点)'), r.due, r.interval_days, r.last_result
             FROM reviews r LEFT JOIN knowledge_nodes n ON n.id = r.node_id
             WHERE r.plan_id=?1 AND r.due <= ?2
             ORDER BY r.due, r.id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![plan_id, today], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(review_item_from(r.map_err(|e| e.to_string())?));
    }
    Ok(out)
}

/// 复习打卡:答对晋级一档,答错回退 1 天;无记录则先建行。返回更新后的条目。
pub fn review_done(plan_id: &str, node_id: i64, correct: bool) -> Result<ReviewItem, String> {
    let c = conn()?;
    review_done_conn(&c, plan_id, node_id, correct)
}

pub(crate) fn review_done_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
    node_id: i64,
    correct: bool,
) -> Result<ReviewItem, String> {
    let cur: Option<i64> = conn
        .query_row(
            "SELECT interval_days FROM reviews WHERE plan_id=?1 AND node_id=?2",
            rusqlite::params![plan_id, node_id],
            |r| r.get(0),
        )
        .ok();
    let interval = match cur {
        Some(iv) => iv,
        None => {
            conn.execute(
                "INSERT INTO reviews(plan_id, node_id, due, interval_days, last_result)
                 VALUES(?1,?2,?3,1,0)",
                rusqlite::params![plan_id, node_id, today_str()],
            )
            .map_err(|e| e.to_string())?;
            1
        }
    };
    let next = next_interval(interval, correct);
    let due = plus_days(next);
    conn.execute(
        "UPDATE reviews SET due=?1, interval_days=?2, last_result=?3 WHERE plan_id=?4 AND node_id=?5",
        rusqlite::params![due, next, correct as i64, plan_id, node_id],
    )
    .map_err(|e| e.to_string())?;
    super::store::add_log(conn, plan_id, Some(node_id), "review", 0)?;
    let id: i64 = conn
        .query_row(
            "SELECT id FROM reviews WHERE plan_id=?1 AND node_id=?2",
            rusqlite::params![plan_id, node_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let name: String = conn
        .query_row(
            "SELECT name FROM knowledge_nodes WHERE id=?1",
            [node_id],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "(已删除知识点)".into());
    Ok(review_item_from((id, node_id, name, due, next, correct as i64)))
}

// ---------------- 今日待办 ----------------

/// 今日待办:当前排期周内未完成知识点 + 到期复习。
pub fn today(plan_id: &str) -> Result<TodayPlan, String> {
    let payload = super::store::read_payload(plan_id);
    let c = conn()?;
    today_conn(&c, plan_id, payload.as_ref())
}

pub(crate) fn today_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
    payload: Option<&super::PlanPayload>,
) -> Result<TodayPlan, String> {
    let reviews = review_queue_conn(conn, plan_id)?;
    let Some(pl) = payload else {
        return Ok(TodayPlan { week: 0, total_weeks: 0, stage_names: Vec::new(), nodes: Vec::new(), reviews });
    };
    let created: String = conn
        .query_row("SELECT created FROM plans WHERE id=?1", [plan_id], |r| r.get(0))
        .unwrap_or_default();
    let created_date =
        NaiveDate::parse_from_str(created.get(..created.len().min(10)).unwrap_or(""), "%Y-%m-%d").ok();
    let total_weeks = pl.schedule.len();
    let week = created_date
        .map(|c| current_week(c, Local::now().date_naive(), total_weeks))
        .unwrap_or(0);
    let stages: Vec<u32> = if week > 0 {
        pl.schedule
            .iter()
            .filter(|w| w.week as i64 == week)
            .map(|w| w.stage)
            .collect()
    } else {
        Vec::new()
    };
    let stage_names = pl
        .stages
        .iter()
        .filter(|s| stages.contains(&s.index))
        .map(|s| s.name.clone())
        .collect();
    let nodes = nodes_state_conn(conn, plan_id)?
        .into_iter()
        .filter(|n| !n.done && stages.contains(&(n.stage_index as u32)))
        .map(|n| TodayItem {
            node_id: n.node_id,
            node_name: n.name,
            stage_index: n.stage_index,
            difficulty: n.difficulty,
            mastery: n.mastery,
        })
        .collect();
    Ok(TodayPlan { week, total_weeks, stage_names, nodes, reviews })
}

// ---------------- P2:专注计时 / 统计 / 记忆注入 ----------------

/// 专注计时结束:分钟数落 study_logs(kind='focus'),同时构成当日打卡。
pub fn focus_log(plan_id: &str, node_id: Option<i64>, minutes: i64) -> Result<(), String> {
    memory::validate_id(plan_id)?;
    let minutes = minutes.clamp(1, 480);
    let c = conn()?;
    super::store::add_log(&c, plan_id, node_id, "focus", minutes)
}

/// 单日统计(打卡网格用)。
#[derive(Debug, Clone, Serialize)]
pub struct DayStat {
    pub date: String,
    pub minutes: i64,
    /// 当日学习行为次数(learn/quiz/review/research/focus)
    pub activity: i64,
}

/// 计划级统计(连续天数 / 累计时长 / 完成度 / 掌握度均值 / 最近 35 天网格)。
#[derive(Debug, Clone, Serialize)]
pub struct StudyStats {
    pub streak_days: i64,
    pub total_days: i64,
    pub total_minutes: i64,
    pub done_nodes: i64,
    pub total_nodes: i64,
    pub mastery_avg: f64,
    /// 最近 35 天(含今天,升序)
    pub days: Vec<DayStat>,
}

/// 统计口径:打卡日 = 当日存在任意学习行为日志(kind 任意,含 research 深挖)。
pub fn stats(plan_id: &str) -> Result<StudyStats, String> {
    let c = conn()?;
    stats_conn(&c, plan_id)
}

pub(crate) fn stats_conn(conn: &rusqlite::Connection, plan_id: &str) -> Result<StudyStats, String> {
    use std::collections::BTreeMap;
    let mut stmt = conn
        .prepare(
            "SELECT substr(at,1,10), SUM(minutes), COUNT(*) FROM study_logs
             WHERE plan_id=?1 GROUP BY substr(at,1,10)",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([plan_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
        })
        .map_err(|e| e.to_string())?;
    // date -> (minutes, activity);BTreeMap 保证日期有序
    let mut by_day: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for r in rows {
        let (d, m, n) = r.map_err(|e| e.to_string())?;
        let e = by_day.entry(d).or_insert((0, 0));
        e.0 += m.max(0);
        e.1 += n;
    }

    let today = Local::now().date_naive();
    // 最近 35 天升序网格(最旧 → 今天)
    let mut days = Vec::with_capacity(35);
    for i in 0..=34i64 {
        let d = (today - Duration::days(34 - i)).format("%Y-%m-%d").to_string();
        let (m, n) = by_day.get(&d).copied().unwrap_or((0, 0));
        days.push(DayStat { date: d, minutes: m, activity: n });
    }
    let total_days = by_day.len() as i64;
    let total_minutes: i64 = by_day.values().map(|e| e.0).sum();

    // 连续打卡:从今天起往前数(今天没打卡则从昨天起,不归零昨天的连续)
    let mut streak: i64 = 0;
    let mut cursor = today;
    if !by_day.contains_key(&cursor.format("%Y-%m-%d").to_string()) {
        cursor -= Duration::days(1);
    }
    while by_day.contains_key(&cursor.format("%Y-%m-%d").to_string()) {
        streak += 1;
        cursor -= Duration::days(1);
    }

    let (total_nodes, done_nodes, mastery_avg): (i64, i64, f64) = conn
        .query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(CASE WHEN done_at IS NOT NULL THEN 1 ELSE 0 END), 0),
                    COALESCE(AVG(mastery), 0.0)
             FROM knowledge_nodes WHERE plan_id=?1",
            [plan_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;

    Ok(StudyStats {
        streak_days: streak,
        total_days,
        total_minutes,
        done_nodes,
        total_nodes,
        mastery_avg,
        days,
    })
}

/// 学习进度记忆摘要(注入会话 system):最多 3 个进行中/已完成计划的一行进度。
pub fn memory_digest() -> Result<Option<String>, String> {
    let c = conn()?;
    memory_digest_conn(&c)
}

pub(crate) fn memory_digest_conn(
    conn: &rusqlite::Connection,
) -> Result<Option<String>, String> {
    let c = conn;
    let mut stmt = c
        .prepare(
            "SELECT id, topic, status FROM plans WHERE status IN ('active','done')
             ORDER BY updated DESC LIMIT 3",
        )
        .map_err(|e| e.to_string())?;
    let plans: Vec<(String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if plans.is_empty() {
        return Ok(None);
    }
    let mut lines = Vec::new();
    for (id, topic, status) in plans {
        let (total, done, avg): (i64, i64, f64) = c
            .query_row(
                "SELECT COUNT(*),
                        COALESCE(SUM(CASE WHEN done_at IS NOT NULL THEN 1 ELSE 0 END), 0),
                        COALESCE(AVG(mastery), 0.0)
                 FROM knowledge_nodes WHERE plan_id=?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap_or((0, 0, 0.0));
        let last: Option<String> = c
            .query_row(
                "SELECT node_name FROM study_units WHERE plan_id=?1 ORDER BY created DESC LIMIT 1",
                [&id],
                |r| r.get(0),
            )
            .ok();
        let state = if status == "done" { "已完成计划" } else { "正在学习" };
        let mut line = format!(
            "- {state}「{topic}」:完成 {done}/{total} 个知识点,平均掌握 {:.0}%",
            avg * 100.0
        );
        if let Some(name) = last {
            line.push_str(&format!(",最近学到「{name}」"));
        }
        lines.push(line);
    }
    Ok(Some(lines.join("\n")))
}

// ---------------- 测试 ----------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::study::{PlanPayload, ScheduleWeek, StageSpec};

    fn seed_plan(conn: &rusqlite::Connection, plan_id: &str, created: &str) {
        conn.execute(
            "INSERT INTO plans(id, topic, status, created, updated) VALUES(?1,'t','active',?2,?2)",
            rusqlite::params![plan_id, created],
        )
        .unwrap();
    }

    fn seed_node(conn: &rusqlite::Connection, plan_id: &str, id: i64, name: &str, stage: i64) {
        conn.execute(
            "INSERT INTO knowledge_nodes(plan_id, stage_index, name, difficulty) VALUES(?1,?2,?3,2)",
            rusqlite::params![plan_id, stage, name],
        )
        .unwrap();
        // 显式指定 id
        conn.execute(
            "UPDATE knowledge_nodes SET id=?1 WHERE plan_id=?2 AND name=?3",
            rusqlite::params![id, plan_id, name],
        )
        .unwrap();
    }

    fn seed_quiz(conn: &rusqlite::Connection, plan_id: &str, quiz_id: i64, node_id: i64) {
        let qs = vec![
            QuizQuestion { idx: 0, qtype: "choice".into(), stem: "s0".into(), options: vec!["A. x".into(), "B. y".into()], answer: "A".into(), explanation: "e".into() },
            QuizQuestion { idx: 1, qtype: "judge".into(), stem: "s1".into(), options: vec![], answer: "对".into(), explanation: "e".into() },
            QuizQuestion { idx: 2, qtype: "short".into(), stem: "s2".into(), options: vec![], answer: "要点".into(), explanation: "e".into() },
        ];
        conn.execute(
            "INSERT INTO quizzes(id, plan_id, node_id, node_name, payload_json, created) VALUES(?1,?2,?3,'n',?4,'2026-01-01')",
            rusqlite::params![quiz_id, plan_id, node_id, serde_json::to_string(&qs).unwrap()],
        )
        .unwrap();
    }

    #[test]
    fn grade_covers_choice_judge_short() {
        assert!(grade("choice", "A", "a", None));
        assert!(grade("choice", "B. 所有权", "B", None));
        assert!(!grade("choice", "A", "C", None));
        assert!(grade("choice", "A", "正确答案:D", None) == false);
        assert!(grade("judge", "对", "正确", None));
        assert!(grade("judge", "错", "false", None));
        assert!(!grade("judge", "对", "错", None));
        assert!(grade("short", "要点", "随便", Some(true)));
        assert!(!grade("short", "要点", "随便", Some(false)));
        assert!(!grade("short", "要点", "没答", None));
    }

    #[test]
    fn next_interval_promotes_and_resets() {
        assert_eq!(next_interval(1, true), 3);
        assert_eq!(next_interval(3, true), 7);
        assert_eq!(next_interval(14, true), 30);
        assert_eq!(next_interval(30, true), 30, "封顶");
        assert_eq!(next_interval(30, false), 1, "答错回退");
        assert_eq!(next_interval(0, true), 3, "未知当前从 3 起步");
    }

    #[test]
    fn weighted_mastery_prefers_available_source() {
        assert!((weighted_mastery(0.5, 0.8) - (0.5 * 0.4 + 0.8 * 0.6)).abs() < 1e-9);
        assert!((weighted_mastery(0.5, -1.0) - 0.5).abs() < 1e-9);
        assert!((weighted_mastery(-1.0, 0.8) - 0.8).abs() < 1e-9);
        assert_eq!(weighted_mastery(-1.0, -1.0), 0.0);
    }

    #[test]
    fn current_week_clamps() {
        let created = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        let d = |n: i64| created + Duration::days(n);
        assert_eq!(current_week(created, d(0), 10), 1);
        assert_eq!(current_week(created, d(6), 10), 1);
        assert_eq!(current_week(created, d(7), 10), 2);
        assert_eq!(current_week(created, d(100), 10), 10, "clamp 到末周");
        assert_eq!(current_week(created, d(3), 0), 0, "无排期");
    }

    #[test]
    fn strip_outer_fence_keeps_inner_code_blocks() {
        assert_eq!(strip_outer_fence("```markdown\n# 标题\n```"), "# 标题");
        assert_eq!(strip_outer_fence("# 标题\n\n正文"), "# 标题\n\n正文");
        let with_code = "# 标题\n\n```rust\nfn main() {}\n```\n\n结尾";
        assert_eq!(strip_outer_fence(with_code), with_code, "未包裹的正文不动");
    }

    #[test]
    fn parse_quiz_normalizes_and_caps() {
        let text = r#"好的,这是题目:{"questions":[
            {"idx":5,"type":"choice","stem":"Q1","options":["A. a","B. b"],"answer":"B","explanation":"x"},
            {"idx":1,"type":"judge","stem":"Q2","answer":true},
            {"idx":2,"type":"short","stem":"Q3","answer":"要点"},
            {"stem":"","answer":"x"},
            {"idx":4,"type":"choice","stem":"Q4","options":[],"answer":"A"},
            {"idx":5,"type":"choice","stem":"Q5","options":[],"answer":"B"},
            {"idx":6,"type":"choice","stem":"Q6","options":[],"answer":"C"}
        ]}"#;
        let qs = parse_quiz(text).unwrap();
        assert_eq!(qs.len(), 5, "截到 5 题");
        assert_eq!(qs[0].idx, 0, "idx 重排");
        assert_eq!(qs[0].qtype, "choice");
        assert_eq!(qs[1].qtype, "judge");
        assert_eq!(qs[1].answer, "对", "布尔答案归一");
        assert_eq!(qs[2].qtype, "short");
        assert_eq!(qs[4].stem, "Q5");
        assert!(parse_quiz("no json").is_none());
        assert!(parse_quiz(r#"{"questions":[{"idx":0,"type":"short","stem":"Q","answer":"a"}]}"#).is_none(), "不足 3 题判失败");
    }

    #[test]
    fn quiz_submit_grades_and_schedules_review() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        seed_plan(&conn, "p0000001", "2026-01-01T00:00:00Z");
        seed_node(&conn, "p0000001", 1, "所有权", 0);
        seed_quiz(&conn, "p0000001", 9, 1);

        // 全对:0.8 阈值之上 → 晋级到 3 天
        let answers = vec![
            QuizAnswerIn { question_idx: 0, user_answer: "A".into(), self_correct: None },
            QuizAnswerIn { question_idx: 1, user_answer: "对".into(), self_correct: None },
            QuizAnswerIn { question_idx: 2, user_answer: "答了".into(), self_correct: Some(true) },
        ];
        let r = quiz_submit_conn(&conn, "p0000001", 9, &answers).unwrap();
        assert!((r.score - 1.0).abs() < 1e-9);
        assert!((r.mastery - 1.0).abs() < 1e-9, "测验分 1.0 且无自评 → mastery = 1.0");
        let iv: i64 = conn.query_row("SELECT interval_days FROM reviews WHERE plan_id='p0000001' AND node_id=1", [], |x| x.get(0)).unwrap();
        assert_eq!(iv, 3);
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM quiz_attempts WHERE quiz_id=9", [], |x| x.get(0)).unwrap();
        assert_eq!(n, 3);
        let qsc: f64 = conn.query_row("SELECT quiz_score FROM knowledge_nodes WHERE id=1", [], |x| x.get(0)).unwrap();
        assert!((qsc - 1.0).abs() < 1e-9);

        // 低分:回退到 1 天,加权 mastery = 0.5*0.4 + 0.4*0.6
        let answers = vec![
            QuizAnswerIn { question_idx: 0, user_answer: "B".into(), self_correct: None },
            QuizAnswerIn { question_idx: 1, user_answer: "对".into(), self_correct: None },
            QuizAnswerIn { question_idx: 2, user_answer: "答了".into(), self_correct: Some(false) },
        ];
        conn.execute("UPDATE knowledge_nodes SET self_rating=0.5 WHERE id=1", []).unwrap();
        let r2 = quiz_submit_conn(&conn, "p0000001", 9, &answers).unwrap();
        assert!((r2.score - (1.0 / 3.0)).abs() < 1e-9);
        let iv2: i64 = conn.query_row("SELECT interval_days FROM reviews WHERE plan_id='p0000001' AND node_id=1", [], |x| x.get(0)).unwrap();
        assert_eq!(iv2, 1);
        let m: f64 = conn.query_row("SELECT mastery FROM knowledge_nodes WHERE id=1", [], |x| x.get(0)).unwrap();
        assert!((m - (0.5 * 0.4 + (1.0 / 3.0) * 0.6)).abs() < 1e-9);
    }

    #[test]
    fn review_done_and_queue_flow() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        seed_plan(&conn, "p0000002", "2026-01-01T00:00:00Z");
        seed_node(&conn, "p0000002", 2, "借用", 0);

        // 无记录先建行;答对晋级
        let it = review_done_conn(&conn, "p0000002", 2, true).unwrap();
        assert_eq!(it.interval_days, 3);
        assert!(!it.overdue);
        // 再答对晋级 7;答错回退 1
        let it2 = review_done_conn(&conn, "p0000002", 2, true).unwrap();
        assert_eq!(it2.interval_days, 7);
        let it3 = review_done_conn(&conn, "p0000002", 2, false).unwrap();
        assert_eq!(it3.interval_days, 1);
        assert_eq!(it3.last_result, 0);

        // due 为今天 → 进队列
        conn.execute(
            "UPDATE reviews SET due=?1 WHERE plan_id='p0000002' AND node_id=2",
            [super::today_str()],
        )
        .unwrap();
        let q = review_queue_conn(&conn, "p0000002").unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].node_name, "借用");
        // 未来到期 → 不进队列
        conn.execute(
            "UPDATE reviews SET due='2099-01-01' WHERE plan_id='p0000002' AND node_id=2",
            [],
        )
        .unwrap();
        assert!(review_queue_conn(&conn, "p0000002").unwrap().is_empty());
    }

    #[test]
    fn set_mastery_and_today_flow() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        seed_plan(&conn, "p0000003", "2026-01-01T00:00:00Z");
        seed_node(&conn, "p0000003", 1, "A", 0);
        seed_node(&conn, "p0000003", 2, "B", 0);

        // 自评 0.5,无测验 → mastery 0.5;评 1.0 置 done
        let m = set_mastery_conn(&conn, "p0000003", 1, 0.5).unwrap();
        assert!((m - 0.5).abs() < 1e-9);
        let done: Option<String> = conn.query_row("SELECT done_at FROM knowledge_nodes WHERE id=1", [], |x| x.get(0)).unwrap();
        assert!(done.is_none());
        let m2 = set_mastery_conn(&conn, "p0000003", 1, 1.0).unwrap();
        assert!((m2 - 1.0).abs() < 1e-9);
        let done2: Option<String> = conn.query_row("SELECT done_at FROM knowledge_nodes WHERE id=1", [], |x| x.get(0)).unwrap();
        assert!(done2.is_some(), "评掌握自动置 done_at");

        // today:无 payload → 空周但复习仍在
        let tp = today_conn(&conn, "p0000003", None).unwrap();
        assert_eq!(tp.week, 0);
        assert!(tp.nodes.is_empty());

        // 有 payload:第 1 周阶段 0,未完成节点 B 在列,A 已 done 不在列
        let payload = PlanPayload {
            stages: vec![StageSpec {
                index: 0,
                name: "基础".into(),
                goal: String::new(),
                difficulty: 1,
                estimated_weeks: 2,
                topics: vec!["A".into(), "B".into()],
                milestone: "m".into(),
            }],
            schedule: vec![
                ScheduleWeek { week: 1, stage: 0, focus: "基础".into(), minutes_per_day: 30 },
                ScheduleWeek { week: 2, stage: 0, focus: "基础".into(), minutes_per_day: 30 },
            ],
            ..Default::default()
        };
        let tp2 = today_conn(&conn, "p0000003", Some(&payload)).unwrap();
        assert!(tp2.week >= 1);
        assert_eq!(tp2.stage_names, vec!["基础".to_string()]);
        assert!(tp2.nodes.iter().all(|n| n.node_name != "A"), "done 节点不进今日待办");
        assert!(tp2.nodes.iter().any(|n| n.node_name == "B"));
    }

    #[test]
    fn nodes_state_includes_unit_flag() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        seed_plan(&conn, "p0000004", "2026-01-01T00:00:00Z");
        seed_node(&conn, "p0000004", 1, "A", 0);
        seed_node(&conn, "p0000004", 2, "B", 1);
        conn.execute(
            "INSERT INTO study_units(plan_id, node_id, node_name, title, est_minutes, content_md, created)
             VALUES('p0000004', 1, 'A', 'A', 30, '# A', '2026-01-01')",
            [],
        )
        .unwrap();
        let ns = nodes_state_conn(&conn, "p0000004").unwrap();
        assert_eq!(ns.len(), 2);
        assert!(ns[0].has_unit);
        assert!(!ns[1].has_unit);
        assert!(!ns[1].must_learn);
    }

    #[test]
    fn stats_counts_streak_minutes_and_grid() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        seed_plan(&conn, "p0000005", "2026-01-01T00:00:00Z");
        seed_node(&conn, "p0000005", 1, "A", 0);
        conn.execute("UPDATE knowledge_nodes SET done_at='2026-01-02', mastery=0.7 WHERE id=1", []).unwrap();
        seed_node(&conn, "p0000005", 2, "B", 0);

        let today = Local::now().date_naive();
        let yst = (today - Duration::days(1)).format("%Y-%m-%d").to_string();
        let d2 = (today - Duration::days(2)).format("%Y-%m-%d").to_string();
        for (at, kind, minutes) in [
            (format!("{yst}T10:00:00+08:00"), "learn", 0),
            (format!("{yst}T11:00:00+08:00"), "quiz", 0),
            (format!("{d2}T09:00:00+08:00"), "focus", 25),
        ] {
            conn.execute(
                "INSERT INTO study_logs(plan_id, node_id, kind, minutes, at) VALUES('p0000005', NULL, ?1, ?2, ?3)",
                rusqlite::params![kind, minutes, at],
            )
            .unwrap();
        }

        let st = stats_conn(&conn, "p0000005").unwrap();
        assert_eq!(st.total_days, 2);
        assert_eq!(st.total_minutes, 25);
        assert_eq!(st.streak_days, 2, "今天没打卡但从昨天连续");
        assert_eq!(st.done_nodes, 1);
        assert_eq!(st.total_nodes, 2);
        assert!((st.mastery_avg - 0.35).abs() < 1e-9, "未学节点 mastery=0 计入均值");
        assert_eq!(st.days.len(), 35);
        assert_eq!(st.days[34].date, today.format("%Y-%m-%d").to_string());
        assert_eq!(st.days[33].activity, 2);
        assert_eq!(st.days[32].minutes, 25);
        assert_eq!(st.days[0].activity, 0);
    }

    #[test]
    fn memory_digest_summarizes_active_plans() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        assert!(memory_digest_conn(&conn).unwrap().is_none(), "无计划 → None");
        seed_plan(&conn, "p0000006", "2026-01-01T00:00:00Z");
        conn.execute("UPDATE plans SET topic='Rust 入门', status='active' WHERE id='p0000006'", []).unwrap();
        seed_node(&conn, "p0000006", 1, "A", 0);
        conn.execute("UPDATE knowledge_nodes SET done_at='2026-01-02', mastery=1.0 WHERE id=1", []).unwrap();
        seed_node(&conn, "p0000006", 2, "B", 0);
        conn.execute(
            "INSERT INTO study_units(plan_id, node_id, node_name, title, est_minutes, content_md, created)
             VALUES('p0000006', 1, 'A', 'A', 30, '# A', '2026-01-02T00:00:00Z')",
            [],
        )
        .unwrap();
        let digest = memory_digest_conn(&conn).unwrap().unwrap();
        assert!(digest.contains("正在学习「Rust 入门」"));
        assert!(digest.contains("完成 1/2"));
        assert!(digest.contains("最近学到「A」"));
    }
}
