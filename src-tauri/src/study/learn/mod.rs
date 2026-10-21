//! 学习执行闭环(P1):自学材料 / 测验 / 掌握度 / SM-2-lite 复习 / 今日待办。
//!
//! 来源声明 / Attribution:
//!   Unit segmentation, quiz flow and SM-2-lite review inspired by Deep-tutor
//!   https://github.com/hershate/Deep-tutor — Copyright (c) hershate and contributors
//!   Licensed under the Apache License, Version 2.0 (see LICENSE / NOTICE).
//!   Re-implemented in Rust; see NOTICE for the list of modifications.
//!
//! - 自学材料:每知识点一份 Markdown(study_units,UNIQUE 幂等缓存,支持 force 重新生成);
//! - 测验:每知识点 5 题(选择/判断/简答),客观题 Rust 确定性判分,简答题用户自评;
//! - 掌握度:mastery = 自评 ×0.4 + 最新测验正确率 ×0.6(单项缺失时取另一项);
//! - 复习:SM-2-lite 间隔序列 1/3/7/14/30 天,答对晋级、答错回退到 1 天;
//! - 今日待办:按 plans.created 推算当前排期周 → 该周阶段内未完成知识点 + 到期复习。
//!
//! 纯逻辑(判分/加权/间隔/周推算)与 DB 操作(conn 入参)分离,便于单测。

pub mod mastery;
pub mod quiz;
pub mod review;
pub mod stats;
pub mod units;

use chrono::{Duration, Local};

#[allow(unused_imports)]
pub use mastery::{grade, node_done, nodes_state, set_mastery, weighted_mastery, NodeState};
#[allow(unused_imports)]
pub use quiz::{parse_quiz, quiz_submit, QuizAnswerIn, QuizGrade, QuizQuestion, QuizResult, QuizRow};
#[allow(unused_imports)]
pub use review::{
    current_week, next_interval, review_done, review_queue, today, ReviewItem, TodayItem, TodayPlan,
    INTERVALS,
};
#[allow(unused_imports)]
pub use stats::{focus_log, memory_digest, stats, DayStat, StudyStats};
#[allow(unused_imports)]
pub use units::{generate_unit, new_quiz, strip_outer_fence, UnitRow};

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

fn conn() -> Result<std::sync::MutexGuard<'static, rusqlite::Connection>, String> {
    crate::db::conn().lock().map_err(|e| e.to_string())
}

#[cfg(test)]
pub(crate) mod test_util {
    use super::quiz::QuizQuestion;

    pub(crate) fn seed_plan(conn: &rusqlite::Connection, plan_id: &str, created: &str) {
        conn.execute(
            "INSERT INTO plans(id, topic, status, created, updated) VALUES(?1,'t','active',?2,?2)",
            rusqlite::params![plan_id, created],
        )
        .unwrap();
    }

    pub(crate) fn seed_node(conn: &rusqlite::Connection, plan_id: &str, id: i64, name: &str, stage: i64) {
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

    pub(crate) fn seed_quiz(conn: &rusqlite::Connection, plan_id: &str, quiz_id: i64, node_id: i64) {
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
}
