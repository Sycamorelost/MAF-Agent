//! 判分、掌握度加权与知识点完成状态。

use serde::Serialize;

use crate::study::store;

use super::conn;
use super::now_str;

/// 掌握度加权:自评 ×0.4 + 测验 ×0.6;-1 表示未评,单项缺失时取另一项。
pub fn weighted_mastery(self_rating: f64, quiz_score: f64) -> f64 {
    match (self_rating >= 0.0, quiz_score >= 0.0) {
        (true, true) => self_rating * 0.4 + quiz_score * 0.6,
        (true, false) => self_rating,
        (false, true) => quiz_score,
        (false, false) => 0.0,
    }
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
        store::add_log(&c, plan_id, Some(node_id), "learn", 0)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::study::learn::test_util::{seed_node, seed_plan};

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
    fn weighted_mastery_prefers_available_source() {
        assert!((weighted_mastery(0.5, 0.8) - (0.5 * 0.4 + 0.8 * 0.6)).abs() < 1e-9);
        assert!((weighted_mastery(0.5, -1.0) - 0.5).abs() < 1e-9);
        assert!((weighted_mastery(-1.0, 0.8) - 0.8).abs() < 1e-9);
        assert_eq!(weighted_mastery(-1.0, -1.0), 0.0);
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
}
