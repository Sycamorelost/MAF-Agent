//! 测验题目契约、解析、提交与确定性判分。

use serde::{Deserialize, Serialize};

use crate::study::extract_json;
use crate::study::store;

use super::conn;
use super::mastery::{grade, weighted_mastery};
use super::now_str;
use super::plus_days;
use super::review::{INTERVALS, next_interval, upsert_review_conn};

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

#[derive(Debug, Default, Deserialize)]
pub struct QuizAnswerIn {
    pub question_idx: u32,
    #[serde(default)]
    pub user_answer: String,
    /// short 题自评(true = 记得 / false = 没记住);兼容旧前端
    #[serde(default)]
    pub self_correct: Option<bool>,
    /// 主观题「批改此题」按钮的判定结果(用户点击后才产生)
    #[serde(default)]
    pub grade_correct: Option<bool>,
    /// 主观题批改得分(0-1)
    #[serde(default)]
    pub grade_score: Option<f64>,
    /// 主观题批改反馈
    #[serde(default)]
    pub grade_feedback: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuizGrade {
    pub question_idx: u32,
    pub correct: bool,
    /// 0-1(主观题可部分得分)
    pub score: f64,
    pub expected: String,
    pub explanation: String,
    pub feedback: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuizResult {
    pub score: f64,
    pub mastery: f64,
    pub results: Vec<QuizGrade>,
}

/// 解析测验输出:重排 idx、剔除无题干/答案项、judge 布尔答案归一、截到 5 题;有效 <3 题判失败。
pub fn parse_quiz(text: &str) -> Option<Vec<QuizQuestion>> {
    let json = extract_json(text)?;
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
            // 学科题型:calc/cloze 可确定性判分;proof/translate/reading/recite 走自评
            "calc" => "calc",
            "proof" => "proof",
            "cloze" => "cloze",
            "translate" => "translate",
            "reading" => "reading",
            "recite" => "recite",
            _ => "short",
        }
        .to_string();
        let options = item
            .get("options")
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
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
    if out.len() >= 3 { Some(out) } else { None }
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
pub fn quiz_submit(
    plan_id: &str,
    quiz_id: i64,
    answers: &[QuizAnswerIn],
) -> Result<QuizResult, String> {
    let c = conn()?;
    quiz_submit_conn(&c, plan_id, quiz_id, answers)
}

pub(crate) fn quiz_submit_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
    quiz_id: i64,
    answers: &[QuizAnswerIn],
) -> Result<QuizResult, String> {
    let (_, node_id, node_name, questions) = quiz_row_conn(conn, plan_id, quiz_id)?;
    if questions.is_empty() {
        return Err("测验题目为空".into());
    }
    let now = now_str();
    let mut results = Vec::new();
    let mut correct_n = 0usize;
    let mut scores_sum = 0.0f64;
    for q in &questions {
        let (ua, sc, gc, gs, gf) = answers
            .iter()
            .find(|a| a.question_idx == q.idx)
            .map(|a| {
                (
                    a.user_answer.clone(),
                    a.self_correct,
                    a.grade_correct,
                    a.grade_score,
                    a.grade_feedback.clone(),
                )
            })
            .unwrap_or((String::new(), None, None, None, None));
        // 客观题确定性判分;主观题以「批改此题」按钮产生的结果为准
        let (ok, item_score) = if super::paper::is_objective(&q.qtype) {
            let ok = grade(&q.qtype, &q.answer, &ua, None);
            (ok, if ok { 1.0 } else { 0.0 })
        } else {
            let ok = gc
                .or(sc)
                .unwrap_or_else(|| gs.map(|s| s >= 0.6).unwrap_or(false));
            let s = gs.unwrap_or(if ok { 1.0 } else { 0.0 }).clamp(0.0, 1.0);
            (ok, s)
        };
        if ok {
            correct_n += 1;
        }
        scores_sum += item_score;
        conn.execute(
            "INSERT INTO quiz_attempts(quiz_id, question_idx, user_answer, correct, answered_at)
             VALUES(?1,?2,?3,?4,?5)",
            rusqlite::params![quiz_id, q.idx, ua, ok as i64, now],
        )
        .map_err(|e| format!("save attempt failed: {e}"))?;
        results.push(QuizGrade {
            question_idx: q.idx,
            correct: ok,
            score: item_score,
            expected: q.answer.clone(),
            explanation: q.explanation.clone(),
            feedback: gf.unwrap_or_default(),
        });
    }
    let score = scores_sum / questions.len() as f64;

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
        upsert_review_conn(conn, plan_id, node_id, &plus_days(next), next, 1)?;
    } else {
        upsert_review_conn(
            conn,
            plan_id,
            node_id,
            &plus_days(INTERVALS[0]),
            INTERVALS[0],
            0,
        )?;
    }

    // P2 每日任务:有答错题 → 次日排错题重做(去重);复习任务日期由 upsert_review 同步
    if correct_n < questions.len() {
        super::tasks::schedule_redo_conn(conn, plan_id, node_id, &node_name)?;
    }

    store::add_log(conn, plan_id, Some(node_id), "quiz", 0)?;
    Ok(QuizResult {
        score,
        mastery,
        results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::study::learn::test_util::{seed_node, seed_plan, seed_quiz};

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
        assert!(
            parse_quiz(r#"{"questions":[{"idx":0,"type":"short","stem":"Q","answer":"a"}]}"#)
                .is_none(),
            "不足 3 题判失败"
        );
    }

    #[test]
    fn parse_quiz_accepts_subject_types() {
        let text = r#"{"questions":[
            {"idx":0,"type":"calc","stem":"计算 2+3×4","answer":"14","explanation":"先乘后加"},
            {"idx":1,"type":"cloze","stem":"I ____ a student.","answer":"am"},
            {"idx":2,"type":"translate","stem":"翻译:学而不思则罔","answer":"Learning without thought is labor lost."},
            {"idx":3,"type":"proof","stem":"证明:对顶角相等","answer":"证明要点"},
            {"idx":4,"type":"reading","stem":"阅读短文回答:主旨是什么","answer":"要点"},
            {"idx":5,"type":"recite","stem":"默写《静夜思》前两句","answer":"床前明月光,疑是地上霜。"}
        ]}"#;
        let qs = parse_quiz(text).unwrap();
        assert_eq!(qs.len(), 5, "仍截到 5 题");
        assert_eq!(qs[0].qtype, "calc");
        assert_eq!(qs[1].qtype, "cloze");
        assert_eq!(qs[2].qtype, "translate");
        assert_eq!(qs[3].qtype, "proof");
        // recite 被截断;reading 保留
        assert_eq!(qs[4].qtype, "reading");
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
            QuizAnswerIn {
                question_idx: 0,
                user_answer: "A".into(),
                self_correct: None,
                ..Default::default()
            },
            QuizAnswerIn {
                question_idx: 1,
                user_answer: "对".into(),
                self_correct: None,
                ..Default::default()
            },
            QuizAnswerIn {
                question_idx: 2,
                user_answer: "答了".into(),
                self_correct: Some(true),
                ..Default::default()
            },
        ];
        let r = quiz_submit_conn(&conn, "p0000001", 9, &answers).unwrap();
        assert!((r.score - 1.0).abs() < 1e-9);
        assert!(
            (r.mastery - 1.0).abs() < 1e-9,
            "测验分 1.0 且无自评 → mastery = 1.0"
        );
        let iv: i64 = conn
            .query_row(
                "SELECT interval_days FROM reviews WHERE plan_id='p0000001' AND node_id=1",
                [],
                |x| x.get(0),
            )
            .unwrap();
        assert_eq!(iv, 3);
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM quiz_attempts WHERE quiz_id=9",
                [],
                |x| x.get(0),
            )
            .unwrap();
        assert_eq!(n, 3);
        let qsc: f64 = conn
            .query_row(
                "SELECT quiz_score FROM knowledge_nodes WHERE id=1",
                [],
                |x| x.get(0),
            )
            .unwrap();
        assert!((qsc - 1.0).abs() < 1e-9);

        // 低分:回退到 1 天,加权 mastery = 0.5*0.4 + 0.4*0.6
        let answers = vec![
            QuizAnswerIn {
                question_idx: 0,
                user_answer: "B".into(),
                self_correct: None,
                ..Default::default()
            },
            QuizAnswerIn {
                question_idx: 1,
                user_answer: "对".into(),
                self_correct: None,
                ..Default::default()
            },
            QuizAnswerIn {
                question_idx: 2,
                user_answer: "答了".into(),
                self_correct: Some(false),
                ..Default::default()
            },
        ];
        conn.execute("UPDATE knowledge_nodes SET self_rating=0.5 WHERE id=1", [])
            .unwrap();
        let r2 = quiz_submit_conn(&conn, "p0000001", 9, &answers).unwrap();
        assert!((r2.score - (1.0 / 3.0)).abs() < 1e-9);
        let iv2: i64 = conn
            .query_row(
                "SELECT interval_days FROM reviews WHERE plan_id='p0000001' AND node_id=1",
                [],
                |x| x.get(0),
            )
            .unwrap();
        assert_eq!(iv2, 1);
        let m: f64 = conn
            .query_row("SELECT mastery FROM knowledge_nodes WHERE id=1", [], |x| {
                x.get(0)
            })
            .unwrap();
        assert!((m - (0.5 * 0.4 + (1.0 / 3.0) * 0.6)).abs() < 1e-9);
    }
}
