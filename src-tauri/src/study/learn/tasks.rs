//! 每日任务(P2):阶段 topics × daily_minutes 的确定性切分,不走 LLM。
//!
//! - 生成:计划流水线成功后按「阶段顺序 × 知识点顺序」逐日铺开;
//!   每天按 daily_minutes 切 1-4 个 25-50 分钟任务;类型在 学新(learn) 与 检索练习(quiz) 间交错,
//!   quiz 固定在对应 learn 的次日及以后(检索练习间隔);
//! - 错题重做(redo):测验有答错题时,次日自动排一条重做任务(去重);
//! - 间隔复习(review):日期由现有 SM-2-lite 复习排期给出(upsert_review 时同步,算法不动);
//! - 勾选完成写 study_logs(分钟数计入统计),取消勾选回滚对应日志。

use std::collections::{HashMap, VecDeque};

use chrono::{Duration, Local, NaiveDate};
use serde::Serialize;

use crate::study::PlanPayload;

use super::{conn, now_str, today_str};

/// 一条每日任务。
#[derive(Debug, Clone, Serialize)]
pub struct StudyTask {
    pub id: i64,
    pub plan_id: String,
    /// YYYY-MM-DD(本地日期)
    pub date: String,
    /// learn / quiz / redo / review
    pub kind: String,
    pub node_id: Option<i64>,
    /// 知识点名(展示用;节点删除后仍可读)
    pub node: String,
    pub minutes: i64,
    /// pending / done
    pub status: String,
    /// 是否过期(date 早于今天且未完成)
    pub overdue: bool,
}

/// 把每天可用分钟切成 n 段:n 取 1-4,使每段落在 25-50 分钟。
pub fn split_day(daily_minutes: i64) -> (usize, i64) {
    let daily = daily_minutes.max(1);
    let mut n = ((daily + 15) / 30).clamp(1, 4) as usize;
    while n < 4 && daily / n as i64 > 50 {
        n += 1;
    }
    while n > 1 && daily / (n as i64) < 25 {
        n -= 1;
    }
    (n, daily / n as i64)
}

pub(crate) struct RawTask {
    day: u32,
    kind: &'static str,
    node: String,
}

/// 确定性切分:阶段顺序 × 知识点顺序逐日铺开,learn/quiz 交错,quiz 排在 learn 次日之后。
/// 返回 (任务, 总天数) ;超出排期总天数的知识点不排(仍留在阶段树)。
pub(crate) fn build_tasks(payload: &PlanPayload) -> Vec<RawTask> {
    let total_days = (payload.schedule.len() as u32).max(1) * 7;
    let (per_day, _) = split_day(payload.daily_minutes as i64);
    // 学习队列:阶段 index 升序,阶段内按 topics 顺序
    let mut stages = payload.stages.clone();
    stages.sort_by_key(|s| s.index);
    let mut learn_q: VecDeque<String> = VecDeque::new();
    for s in &stages {
        for t in &s.topics {
            learn_q.push_back(t.clone());
        }
    }
    let mut quiz_avail: VecDeque<String> = VecDeque::new();
    let mut out: Vec<RawTask> = Vec::new();
    for day in 0..total_days {
        let mut today_learned: Vec<String> = Vec::new();
        let mut placed = 0usize;
        for slot in 0..per_day {
            // 奇数位优先排 quiz(交错),队列空则退化为 learn;偶数位优先 learn
            let prefer_quiz = slot % 2 == 1;
            let picked = if prefer_quiz {
                quiz_avail
                    .pop_front()
                    .map(|n| ("quiz", n))
                    .or_else(|| learn_q.pop_front().map(|n| ("learn", n)))
            } else {
                learn_q
                    .pop_front()
                    .map(|n| ("learn", n))
                    .or_else(|| quiz_avail.pop_front().map(|n| ("quiz", n)))
            };
            let Some((kind, node)) = picked else { break };
            if kind == "learn" {
                today_learned.push(node.clone());
            }
            out.push(RawTask {
                day,
                kind,
                node,
            });
            placed += 1;
        }
        // 今天学的内容,明天起才进入检索练习队列
        quiz_avail.extend(today_learned);
        if placed == 0 && learn_q.is_empty() && quiz_avail.is_empty() {
            break;
        }
    }
    out
}

/// 计划生成后铺每日任务(幂等:先清后插)。返回任务条数。
pub fn generate_for_plan(plan_id: &str, payload: &PlanPayload) -> Result<usize, String> {
    let c = conn()?;
    generate_for_plan_conn(&c, plan_id, payload)
}

pub(crate) fn generate_for_plan_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
    payload: &PlanPayload,
) -> Result<usize, String> {
    // 起始日 = 计划创建日(本地日期)
    let created: String = conn
        .query_row("SELECT created FROM plans WHERE id=?1", [plan_id], |r| {
            r.get(0)
        })
        .map_err(|e| format!("plan not found: {e}"))?;
    let start = NaiveDate::parse_from_str(
        created.get(..created.len().min(10)).unwrap_or(""),
        "%Y-%m-%d",
    )
    .unwrap_or_else(|_| Local::now().date_naive());
    // 知识点名 → id(阶段树行已在 finish_plan 落库)
    let mut ids: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT name, id FROM knowledge_nodes WHERE plan_id=?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([plan_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for r in rows {
            let (name, id) = r.map_err(|e| e.to_string())?;
            ids.insert(name, id);
        }
    }
    let (_, per_minutes) = split_day(payload.daily_minutes as i64);
    let raw = build_tasks(payload);
    let now = now_str();
    conn.execute("DELETE FROM study_tasks WHERE plan_id=?1", [plan_id])
        .map_err(|e| e.to_string())?;
    for t in &raw {
        let date = (start + Duration::days(t.day as i64))
            .format("%Y-%m-%d")
            .to_string();
        conn.execute(
            "INSERT INTO study_tasks(plan_id, date, kind, node_id, node, minutes, status, created)
             VALUES(?1,?2,?3,?4,?5,?6,'pending',?7)",
            rusqlite::params![
                plan_id,
                date,
                t.kind,
                ids.get(&t.node).copied(),
                t.node,
                per_minutes,
                now
            ],
        )
        .map_err(|e| format!("insert study_task failed: {e}"))?;
    }
    Ok(raw.len())
}

fn task_from(row: (i64, String, String, String, Option<i64>, String, i64, String)) -> StudyTask {
    let overdue = row.2.as_str() < today_str().as_str() && row.7 == "pending";
    StudyTask {
        id: row.0,
        plan_id: row.1,
        date: row.2,
        kind: row.3,
        node_id: row.4,
        node: row.5,
        minutes: row.6,
        status: row.7,
        overdue,
    }
}

/// 今日任务:未完成的到期任务(date <= 今天,过期浮到今天)+ 今天已完成的。
pub fn tasks_today(plan_id: &str) -> Result<Vec<StudyTask>, String> {
    let c = conn()?;
    tasks_today_conn(&c, plan_id)
}

pub(crate) fn tasks_today_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
) -> Result<Vec<StudyTask>, String> {
    let today = today_str();
    let mut stmt = conn
        .prepare(
            "SELECT id, plan_id, date, kind, node_id, node, minutes, status FROM study_tasks
             WHERE plan_id=?1 AND ((status='pending' AND date<=?2) OR (status='done' AND date=?2))
             ORDER BY CASE WHEN status='pending' THEN 0 ELSE 1 END, date, id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![plan_id, today], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, String>(7)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(task_from(r.map_err(|e| e.to_string())?));
    }
    Ok(out)
}

/// 勾选/取消勾选一条任务。
/// 完成:写 study_logs(分钟计入统计);review 任务同时按「记得」推进复习排期。
/// 取消:回滚该次完成写入的日志行,review 排期不回退。
pub fn task_done(plan_id: &str, task_id: i64, done: bool) -> Result<(), String> {
    let c = conn()?;
    task_done_conn(&c, plan_id, task_id, done)
}

pub(crate) fn task_done_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
    task_id: i64,
    done: bool,
) -> Result<(), String> {
    let (kind, node_id, minutes, status, done_at): (String, Option<i64>, i64, String, Option<String>) = conn
        .query_row(
            "SELECT kind, node_id, minutes, status, done_at FROM study_tasks WHERE id=?1 AND plan_id=?2",
            rusqlite::params![task_id, plan_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .map_err(|_| "任务不存在".to_string())?;
    if done {
        if status == "done" {
            return Ok(());
        }
        let now = now_str();
        conn.execute(
            "UPDATE study_tasks SET status='done', done_at=?1 WHERE id=?2",
            rusqlite::params![now, task_id],
        )
        .map_err(|e| e.to_string())?;
        // 学习日志:时间戳与 done_at 一致,取消勾选时可精确回滚
        conn.execute(
            "INSERT INTO study_logs(plan_id, node_id, kind, minutes, at) VALUES(?1,?2,?3,?4,?5)",
            rusqlite::params![plan_id, node_id, format!("task_{kind}"), minutes, now],
        )
        .map_err(|e| e.to_string())?;
        // 复习任务勾选 = 完成本次复习:按「记得」推进现有排期(会同步排下一条复习任务)
        if kind == "review"
            && let Some(nid) = node_id
        {
            let _ = super::review::review_done_conn(conn, plan_id, nid, true);
        }
    } else {
        if status != "done" {
            return Ok(());
        }
        conn.execute(
            "UPDATE study_tasks SET status='pending', done_at=NULL WHERE id=?1",
            [task_id],
        )
        .map_err(|e| e.to_string())?;
        // 回滚本次完成写入的日志(按计划+节点+类型+分钟+时间戳精确匹配)
        if let Some(at) = done_at {
            conn.execute(
                "DELETE FROM study_logs WHERE plan_id=?1 AND kind=?2 AND minutes=?3 AND at=?4
                 AND (node_id IS ?5 OR node_id=?5)",
                rusqlite::params![plan_id, format!("task_{kind}"), minutes, at, node_id],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 测验出现答错题 → 次日排一条错题重做(同一知识点只保留一条 pending)。
pub(crate) fn schedule_redo_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
    node_id: i64,
    node_name: &str,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM study_tasks WHERE plan_id=?1 AND kind='redo' AND node_id=?2 AND status='pending'",
        rusqlite::params![plan_id, node_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO study_tasks(plan_id, date, kind, node_id, node, minutes, status, created)
         VALUES(?1,?2,'redo',?3,?4,25,'pending',?5)",
        rusqlite::params![plan_id, super::plus_days(1), node_id, node_name, now_str()],
    )
    .map_err(|e| format!("schedule redo failed: {e}"))?;
    Ok(())
}

/// 复习排期变化时同步复习任务:清掉该节点 pending 复习任务,按新到期日排一条。
/// 由 review 排期唯一入口 upsert_review_conn 调用,保证日期与 SM-2-lite 一致。
pub(crate) fn sync_review_task_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
    node_id: i64,
    due: &str,
) -> Result<(), String> {
    // 表可能不存在(极老库迁移前)?迁移保证存在,直接操作。
    conn.execute(
        "DELETE FROM study_tasks WHERE plan_id=?1 AND kind='review' AND node_id=?2 AND status='pending'",
        rusqlite::params![plan_id, node_id],
    )
    .map_err(|e| e.to_string())?;
    let name: String = conn
        .query_row(
            "SELECT name FROM knowledge_nodes WHERE id=?1",
            [node_id],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "(已删除知识点)".into());
    conn.execute(
        "INSERT INTO study_tasks(plan_id, date, kind, node_id, node, minutes, status, created)
         VALUES(?1,?2,'review',?3,?4,25,'pending',?5)",
        rusqlite::params![plan_id, due, node_id, name, now_str()],
    )
    .map_err(|e| format!("sync review task failed: {e}"))?;
    Ok(())
}

// 复习任务同步挂在 review.rs 的 upsert_review_conn / review_done_conn 两个排期入口。

#[cfg(test)]
mod tests {
    use super::*;
    use crate::study::learn::plus_days;
    use crate::study::learn::test_util::{seed_node, seed_plan};
    use crate::study::{ScheduleWeek, StageSpec};

    fn payload_with(topics: &[&str], weeks: u32, daily: u32) -> PlanPayload {
        PlanPayload {
            daily_minutes: daily,
            stages: vec![StageSpec {
                index: 0,
                name: "基础".into(),
                goal: String::new(),
                difficulty: 1,
                estimated_weeks: weeks,
                topics: topics.iter().map(|s| s.to_string()).collect(),
                milestone: String::new(),
            }],
            schedule: (1..=weeks)
                .map(|w| ScheduleWeek {
                    week: w,
                    stage: 0,
                    focus: "基础".into(),
                    minutes_per_day: daily,
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn split_day_bounds() {
        assert_eq!(split_day(30), (1, 30));
        assert_eq!(split_day(60), (2, 30));
        assert_eq!(split_day(120), (4, 30));
        let (n, per) = split_day(200);
        assert_eq!(n, 4);
        assert!((25..=50).contains(&per));
        let (n1, per1) = split_day(45);
        assert_eq!(n1, 1);
        assert_eq!(per1, 45);
    }

    #[test]
    fn build_tasks_interleaves_learn_quiz() {
        let p = payload_with(&["A", "B", "C", "D"], 2, 60); // 每天 2 段
        let tasks = build_tasks(&p);
        // 第 0 天只有 learn(没有可练的);第 1 天 learn+quiz 交错
        let day0: Vec<_> = tasks.iter().filter(|t| t.day == 0).collect();
        assert!(day0.iter().all(|t| t.kind == "learn"));
        let day1: Vec<_> = tasks.iter().filter(|t| t.day == 1).collect();
        assert!(day1.iter().any(|t| t.kind == "quiz"), "次日出现检索练习");
        // quiz 一定排在对应 learn 之后
        let first_learn_b = tasks.iter().find(|t| t.node == "B" && t.kind == "learn").unwrap().day;
        let first_quiz_b = tasks.iter().find(|t| t.node == "B" && t.kind == "quiz").unwrap().day;
        assert!(first_quiz_b > first_learn_b, "quiz 在 learn 次日之后");
        // 每个知识点恰好 1 learn + 1 quiz
        for n in ["A", "B", "C", "D"] {
            assert_eq!(tasks.iter().filter(|t| t.node == n && t.kind == "learn").count(), 1);
            assert_eq!(tasks.iter().filter(|t| t.node == n && t.kind == "quiz").count(), 1);
        }
    }

    #[test]
    fn generate_and_done_roundtrip() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        seed_plan(&conn, "p0000010", &format!("{}T00:00:00Z", today_str()));
        seed_node(&conn, "p0000010", 1, "A", 0);
        seed_node(&conn, "p0000010", 2, "B", 0);
        let p = payload_with(&["A", "B"], 1, 30);

        let n = generate_for_plan_conn(&conn, "p0000010", &p).unwrap();
        assert_eq!(n, 4, "2 learn + 2 quiz");
        // 幂等:再生成不翻倍
        let n2 = generate_for_plan_conn(&conn, "p0000010", &p).unwrap();
        assert_eq!(n2, 4);

        let today = tasks_today_conn(&conn, "p0000010").unwrap();
        assert!(!today.is_empty(), "首日任务可见");
        assert!(today.iter().all(|t| t.kind == "learn"));
        let first = today[0].id;
        task_done_conn(&conn, "p0000010", first, true).unwrap();
        let after = tasks_today_conn(&conn, "p0000010").unwrap();
        assert_eq!(after.iter().filter(|t| t.status == "done").count(), 1);
        // 完成写日志,取消回滚
        let logged: i64 = conn
            .query_row("SELECT COUNT(*) FROM study_logs WHERE plan_id='p0000010' AND kind='task_learn'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(logged, 1);
        task_done_conn(&conn, "p0000010", first, false).unwrap();
        let logged2: i64 = conn
            .query_row("SELECT COUNT(*) FROM study_logs WHERE plan_id='p0000010' AND kind='task_learn'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(logged2, 0);
    }

    #[test]
    fn redo_and_review_sync() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        seed_plan(&conn, "p0000011", &format!("{}T00:00:00Z", today_str()));
        seed_node(&conn, "p0000011", 1, "A", 0);

        schedule_redo_conn(&conn, "p0000011", 1, "A").unwrap();
        schedule_redo_conn(&conn, "p0000011", 1, "A").unwrap();
        let cnt: i64 = conn
            .query_row("SELECT COUNT(*) FROM study_tasks WHERE plan_id='p0000011' AND kind='redo' AND status='pending'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cnt, 1, "重做任务去重");

        sync_review_task_conn(&conn, "p0000011", 1, &plus_days(3)).unwrap();
        let (date,): (String,) = conn
            .query_row("SELECT date FROM study_tasks WHERE plan_id='p0000011' AND kind='review'", [], |r| Ok((r.get(0)?,)))
            .unwrap();
        assert_eq!(date, plus_days(3), "复习任务日期跟随排期");
        // 再次同步:旧的 pending 被清,只剩新日期一条
        sync_review_task_conn(&conn, "p0000011", 1, &plus_days(7)).unwrap();
        let cnt2: i64 = conn
            .query_row("SELECT COUNT(*) FROM study_tasks WHERE plan_id='p0000011' AND kind='review' AND status='pending'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cnt2, 1);
    }

    #[test]
    fn review_task_done_advances_schedule() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        seed_plan(&conn, "p0000012", &format!("{}T00:00:00Z", today_str()));
        seed_node(&conn, "p0000012", 1, "A", 0);
        sync_review_task_conn(&conn, "p0000012", 1, &today_str()).unwrap();
        let tid: i64 = conn
            .query_row("SELECT id FROM study_tasks WHERE plan_id='p0000012' AND kind='review'", [], |r| r.get(0))
            .unwrap();
        task_done_conn(&conn, "p0000012", tid, true).unwrap();
        // 排期被推进:产生下一条未来复习任务
        let next: String = conn
            .query_row("SELECT due FROM reviews WHERE plan_id='p0000012' AND node_id=1", [], |r| r.get(0))
            .unwrap();
        assert!(next > today_str(), "复习排期晋级到未来");
        let pending_review: i64 = conn
            .query_row("SELECT COUNT(*) FROM study_tasks WHERE plan_id='p0000012' AND kind='review' AND status='pending'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(pending_review, 1, "下一条复习任务已同步");
    }
}
