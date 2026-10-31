//! 判分:客观题(choice/judge/calc/cloze)Rust 确定性判定;主观题模型按 rubric 批改。
//!
//! 来源方法(调研自开源 LLM 批改实践):固定输出 Schema `{score, feedback}`,
//! score ∈ [0,1],>= 0.6 视为答对;解析失败带反馈重试一次,仍失败则报错不猜测。

use serde::Serialize;

use crate::provider::Provider;
use crate::study::pipeline::{llm_plain, sys_msg, user_msg};
use crate::study::template;

use super::super::mastery::grade;

/// 单题批改结果。
#[derive(Debug, Clone, Serialize)]
pub struct GradeOutcome {
    /// 0-1
    pub score: f64,
    pub correct: bool,
    pub feedback: String,
    pub expected: String,
    pub explanation: String,
}

/// 是否为可确定性判分的客观题。
pub fn is_objective(qtype: &str) -> bool {
    matches!(qtype, "choice" | "judge" | "calc" | "cloze")
}

/// 客观题确定性判分。
pub fn grade_objective(qtype: &str, answer: &str, user_answer: &str) -> GradeOutcome {
    let ua = user_answer.trim();
    if ua.is_empty() {
        return GradeOutcome {
            score: 0.0,
            correct: false,
            feedback: "未作答。".into(),
            expected: answer.to_string(),
            explanation: String::new(),
        };
    }
    let ok = grade(qtype, answer, ua, None);
    GradeOutcome {
        score: if ok { 1.0 } else { 0.0 },
        correct: ok,
        feedback: if ok {
            "客观题判定:正确。".into()
        } else {
            "客观题判定:错误,请对照参考答案。".into()
        },
        expected: answer.to_string(),
        explanation: String::new(),
    }
}

/// 解析模型批改 JSON(score + feedback)。
pub fn parse_grade(text: &str) -> Option<(f64, String)> {
    let json = crate::study::extract_json(text)?;
    let v: serde_json::Value = serde_json::from_str(&json).ok()?;
    let score = v.get("score")?.as_f64()?;
    let feedback = v
        .get("feedback")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if feedback.is_empty() {
        return None;
    }
    Some((score.clamp(0.0, 1.0), feedback))
}

/// 主观题批改(单次或整卷逐题调用;不写库)。
pub async fn grade_subjective(
    provider: &dyn Provider,
    model: &str,
    qtype: &str,
    stem: &str,
    answer: &str,
    explanation: &str,
    user_answer: &str,
) -> Result<(f64, String), String> {
    let ua = user_answer.trim();
    if ua.is_empty() {
        return Ok((0.0, "未作答。".into()));
    }
    let sys = template::effective("study_grade");
    let base = format!(
        "题型:{qtype}\n题干:{stem}\n参考答案:{answer}\n解析:{}\n学习者作答:{ua}",
        if explanation.trim().is_empty() {
            "未提供"
        } else {
            explanation
        }
    );
    let mut last = String::new();
    for attempt in 1..=2u32 {
        let user = if attempt == 1 {
            base.clone()
        } else {
            format!(
                "{base}\n\n上一轮输出不是合法 JSON(缺少 score 或 feedback):{last}\n只输出约定 Schema 的 JSON。"
            )
        };
        let text = llm_plain(
            provider,
            model,
            vec![sys_msg(sys.clone()), user_msg(user)],
        )
        .await?;
        match parse_grade(&text) {
            Some(v) => return Ok(v),
            None => last = "不是含 score(0-1) 与 feedback 的 JSON".into(),
        }
    }
    Err(format!("主观题批改失败:{last}"))
}

/// 单题批改入口(命令层用;客观题本地判,主观题模型批改)。
pub async fn question_grade(
    provider: &dyn Provider,
    model: &str,
    qtype: &str,
    stem: &str,
    answer: &str,
    explanation: &str,
    user_answer: &str,
) -> Result<GradeOutcome, String> {
    if is_objective(qtype) {
        let mut g = grade_objective(qtype, answer, user_answer);
        g.explanation = explanation.to_string();
        return Ok(g);
    }
    let (score, feedback) =
        grade_subjective(provider, model, qtype, stem, answer, explanation, user_answer).await?;
    Ok(GradeOutcome {
        correct: score >= 0.6,
        score,
        feedback,
        expected: answer.to_string(),
        explanation: explanation.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objective_grade_and_parse() {
        assert!(grade_objective("choice", "A", "a").correct);
        assert!(!grade_objective("choice", "A", "B").correct);
        assert!(grade_objective("judge", "对", "正确").correct);
        assert!(grade_objective("calc", "x=2", "x = 2").correct);
        assert!(!grade_objective("calc", "2", "").correct);
        let (s, f) = parse_grade(r#"好的:{"score":0.75,"feedback":"要点一命中"}"#).unwrap();
        assert!((s - 0.75).abs() < 1e-9);
        assert!(f.contains("命中"));
        assert!(parse_grade(r#"{"score":2,"feedback":"x"}"#).is_some(), "越界分数被夹到 0-1");
        assert!(parse_grade(r#"{"score":0.5}"#).is_none(), "缺 feedback 判失败");
    }
}
