//! SM-2-lite 复习队列与今日待办。

use chrono::{Local, NaiveDate};
use serde::Serialize;

use crate::study::PlanPayload;
use crate::study::store;

use super::conn;
use super::mastery::nodes_state_conn;
use super::plus_days;
use super::today_str;

/// SM-2-lite 间隔序列(天)。
pub const INTERVALS: [i64; 5] = [1, 3, 7, 14, 30];

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

pub(crate) fn upsert_review_conn(
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
    // P2 每日任务:复习排期变化 → 同步复习任务(日期跟随排期,算法不动)
    super::tasks::sync_review_task_conn(conn, plan_id, node_id, due)?;
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
    // P2 每日任务:复习打卡推进排期 → 同步下一条复习任务
    super::tasks::sync_review_task_conn(conn, plan_id, node_id, &due)?;
    store::add_log(conn, plan_id, Some(node_id), "review", 0)?;
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
    Ok(review_item_from((
        id,
        node_id,
        name,
        due,
        next,
        correct as i64,
    )))
}

// ---------------- 今日待办 ----------------

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

/// 由计划创建日推算当前排期周(1 起),clamp 到 [1, total_weeks];无排期返回 0。
pub fn current_week(created: NaiveDate, today: NaiveDate, total_weeks: usize) -> i64 {
    if total_weeks == 0 {
        return 0;
    }
    let days = (today - created).num_days();
    (days / 7 + 1).clamp(1, total_weeks as i64)
}

/// 今日待办:当前排期周内未完成知识点 + 到期复习。
pub fn today(plan_id: &str) -> Result<TodayPlan, String> {
    let payload = store::read_payload(plan_id);
    let c = conn()?;
    today_conn(&c, plan_id, payload.as_ref())
}

pub(crate) fn today_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
    payload: Option<&PlanPayload>,
) -> Result<TodayPlan, String> {
    let reviews = review_queue_conn(conn, plan_id)?;
    let Some(pl) = payload else {
        return Ok(TodayPlan {
            week: 0,
            total_weeks: 0,
            stage_names: Vec::new(),
            nodes: Vec::new(),
            reviews,
        });
    };
    let created: String = conn
        .query_row("SELECT created FROM plans WHERE id=?1", [plan_id], |r| {
            r.get(0)
        })
        .unwrap_or_default();
    let created_date = NaiveDate::parse_from_str(
        created.get(..created.len().min(10)).unwrap_or(""),
        "%Y-%m-%d",
    )
    .ok();
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
    Ok(TodayPlan {
        week,
        total_weeks,
        stage_names,
        nodes,
        reviews,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::study::learn::mastery::set_mastery_conn;
    use crate::study::learn::test_util::{seed_node, seed_plan};
    use crate::study::{PlanPayload, ScheduleWeek, StageSpec};
    use chrono::Duration;

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
            [today_str()],
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
    fn set_mastery_and_today_flow() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        seed_plan(&conn, "p0000003", "2026-01-01T00:00:00Z");
        seed_node(&conn, "p0000003", 1, "A", 0);
        seed_node(&conn, "p0000003", 2, "B", 0);

        // 自评 0.5,无测验 → mastery 0.5;评 1.0 置 done
        let m = set_mastery_conn(&conn, "p0000003", 1, 0.5).unwrap();
        assert!((m - 0.5).abs() < 1e-9);
        let done: Option<String> = conn
            .query_row("SELECT done_at FROM knowledge_nodes WHERE id=1", [], |x| {
                x.get(0)
            })
            .unwrap();
        assert!(done.is_none());
        let m2 = set_mastery_conn(&conn, "p0000003", 1, 1.0).unwrap();
        assert!((m2 - 1.0).abs() < 1e-9);
        let done2: Option<String> = conn
            .query_row("SELECT done_at FROM knowledge_nodes WHERE id=1", [], |x| {
                x.get(0)
            })
            .unwrap();
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
                ScheduleWeek {
                    week: 1,
                    stage: 0,
                    focus: "基础".into(),
                    minutes_per_day: 30,
                },
                ScheduleWeek {
                    week: 2,
                    stage: 0,
                    focus: "基础".into(),
                    minutes_per_day: 30,
                },
            ],
            ..Default::default()
        };
        let tp2 = today_conn(&conn, "p0000003", Some(&payload)).unwrap();
        assert!(tp2.week >= 1);
        assert_eq!(tp2.stage_names, vec!["基础".to_string()]);
        assert!(
            tp2.nodes.iter().all(|n| n.node_name != "A"),
            "done 节点不进今日待办"
        );
        assert!(tp2.nodes.iter().any(|n| n.node_name == "B"));
    }
}
