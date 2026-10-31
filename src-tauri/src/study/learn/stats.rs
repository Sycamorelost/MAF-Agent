//! 专注计时、计划级统计与学习进度记忆摘要。

use chrono::{Duration, Local};
use serde::Serialize;

use crate::memory;

use super::conn;

/// 专注计时结束:分钟数落 study_logs(kind='focus'),同时构成当日打卡。
pub fn focus_log(plan_id: &str, node_id: Option<i64>, minutes: i64) -> Result<(), String> {
    memory::validate_id(plan_id)?;
    let minutes = minutes.clamp(1, 480);
    let c = conn()?;
    crate::study::store::add_log(&c, plan_id, node_id, "focus", minutes)
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
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
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
        let d = (today - Duration::days(34 - i))
            .format("%Y-%m-%d")
            .to_string();
        let (m, n) = by_day.get(&d).copied().unwrap_or((0, 0));
        days.push(DayStat {
            date: d,
            minutes: m,
            activity: n,
        });
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

pub(crate) fn memory_digest_conn(conn: &rusqlite::Connection) -> Result<Option<String>, String> {
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
        let state = if status == "done" {
            "已完成计划"
        } else {
            "正在学习"
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::study::learn::test_util::{seed_node, seed_plan};

    #[test]
    fn stats_counts_streak_minutes_and_grid() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        seed_plan(&conn, "p0000005", "2026-01-01T00:00:00Z");
        seed_node(&conn, "p0000005", 1, "A", 0);
        conn.execute(
            "UPDATE knowledge_nodes SET done_at='2026-01-02', mastery=0.7 WHERE id=1",
            [],
        )
        .unwrap();
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
        assert!(
            (st.mastery_avg - 0.35).abs() < 1e-9,
            "未学节点 mastery=0 计入均值"
        );
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
        assert!(
            memory_digest_conn(&conn).unwrap().is_none(),
            "无计划 → None"
        );
        seed_plan(&conn, "p0000006", "2026-01-01T00:00:00Z");
        conn.execute(
            "UPDATE plans SET topic='Rust 入门', status='active' WHERE id='p0000006'",
            [],
        )
        .unwrap();
        seed_node(&conn, "p0000006", 1, "A", 0);
        conn.execute(
            "UPDATE knowledge_nodes SET done_at='2026-01-02', mastery=1.0 WHERE id=1",
            [],
        )
        .unwrap();
        seed_node(&conn, "p0000006", 2, "B", 0);
        conn.execute(
            "INSERT INTO study_units(plan_id, node_id, node_name, est_minutes, content_md, created)
             VALUES('p0000006', 1, 'A', 30, '# A', '2026-01-02T00:00:00Z')",
            [],
        )
        .unwrap();
        let digest = memory_digest_conn(&conn).unwrap().unwrap();
        assert!(digest.contains("正在学习「Rust 入门」"));
        assert!(digest.contains("完成 1/2"));
        assert!(digest.contains("最近学到「A」"));
    }
}
