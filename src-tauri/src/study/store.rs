//! 学习计划的持久化。
//!
//! - db:`plans`(元数据 + 计划载荷 JSON)、`knowledge_nodes`(知识点行)、`study_logs`(行为日志)
//! - 磁盘:`<learningcenter>/plans/{id}/trail.jsonl`(过程事件)、`plan.json`(计划载荷镜像)

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use chrono::Local;
use serde::Serialize;

use crate::db;

use super::{PlanPayload, StudyEvent, store_payload_nodes};

/// plans 表行。
#[derive(Debug, Clone, Serialize)]
pub struct PlanRow {
    pub id: String,
    pub topic: String,
    /// generating / active / done / archived / error / cancelled
    pub status: String,
    pub level: String,
    pub goal: String,
    pub daily_minutes: i64,
    pub deadline: Option<String>,
    pub lang: String,
    pub model_id: Option<String>,
    pub error: Option<String>,
    /// true = 校验失败后降级生成的默认结构
    pub repaired: bool,
    /// 学科附录 key(math/english/physics/chinese/programming/other;None = 未识别/旧计划)
    pub subject: Option<String>,
    /// 学段(primary/junior/senior/university/adult;None = 未指定)
    pub grade_band: Option<String>,
    /// 考试目标(自由文本;None = 无)
    pub exam_target: Option<String>,
    /// 用户明确的内容/格式要求(None = 按模板默认)
    pub preferences: Option<String>,
    pub created: String,
    pub updated: String,
}

const COLS: &str = "id, topic, status, level, goal, daily_minutes, deadline, lang, model_id, error, repaired, subject, grade_band, exam_target, preferences, created, updated";

fn row_from(r: &rusqlite::Row) -> rusqlite::Result<PlanRow> {
    Ok(PlanRow {
        id: r.get(0)?,
        topic: r.get(1)?,
        status: r.get(2)?,
        level: r.get(3)?,
        goal: r.get(4)?,
        daily_minutes: r.get(5)?,
        deadline: r.get(6)?,
        lang: r.get(7)?,
        model_id: r.get(8)?,
        error: r.get(9)?,
        repaired: r.get::<_, i64>(10)? != 0,
        subject: r.get(11)?,
        grade_band: r.get(12)?,
        exam_target: r.get(13)?,
        preferences: r.get(14)?,
        created: r.get(15)?,
        updated: r.get(16)?,
    })
}

/// 学习目录:`<data_dir>/learningcenter/plans/{id}`(旧 `data/study/` 由 `lc::migrate_legacy_dirs` 迁入)。
pub fn study_dir(id: &str) -> PathBuf {
    super::lc::plan_dir(id)
}

// ---------------- plans ----------------

pub fn insert_plan(row: &PlanRow) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO plans(id, topic, status, level, goal, daily_minutes, deadline, lang, model_id, error, repaired, subject, grade_band, exam_target, preferences, created, updated)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
        rusqlite::params![
            row.id,
            row.topic,
            row.status,
            row.level,
            row.goal,
            row.daily_minutes,
            row.deadline,
            row.lang,
            row.model_id,
            row.error,
            row.repaired as i64,
            row.subject,
            row.grade_band,
            row.exam_target,
            row.preferences,
            row.created,
            row.updated,
        ],
    )
    .map_err(|e| format!("insert plans failed: {e}"))?;
    Ok(())
}

pub fn get_plan(id: &str) -> Result<Option<PlanRow>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!("SELECT {COLS} FROM plans WHERE id=?1"))
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query_map([id], row_from).map_err(|e| e.to_string())?;
    match rows.next() {
        Some(Ok(r)) => Ok(Some(r)),
        Some(Err(e)) => Err(e.to_string()),
        None => Ok(None),
    }
}

/// 按创建时间倒序列出全部学习计划。
pub fn list_plans() -> Result<Vec<PlanRow>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!("SELECT {COLS} FROM plans ORDER BY created DESC"))
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], row_from).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// 更新计划状态(终态/中断标记)。
pub fn update_status(id: &str, status: &str, error: Option<&str>) -> Result<(), String> {
    let now = Local::now().to_rfc3339();
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE plans SET status=?1, error=?2, updated=?3 WHERE id=?4",
        rusqlite::params![status, error, now, id],
    )
    .map_err(|e| format!("update plan status failed: {e}"))?;
    Ok(())
}

/// 流水线成功:写回计划载荷与意图字段,置 active。
pub fn finish_plan(id: &str, payload: &PlanPayload) -> Result<(), String> {
    let now = Local::now().to_rfc3339();
    let json = serde_json::to_string(payload).map_err(|e| e.to_string())?;
    // 学科列存归一后的附录 key(空学科 → NULL,等价旧计划的通用模板行为)
    let subject_key = if payload.subject.trim().is_empty() {
        None
    } else {
        Some(
            super::subject::normalize_subject(&payload.subject)
                .as_str()
                .to_string(),
        )
    };
    let grade_band = if payload.grade_band.trim().is_empty() {
        None
    } else {
        Some(payload.grade_band.trim().to_string())
    };
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE plans SET status='active', level=?1, goal=?2, daily_minutes=?3, deadline=?4,
         repaired=?5, stages_json=?6, subject=?7, grade_band=?8, exam_target=?9, preferences=?10, updated=?11 WHERE id=?12",
        rusqlite::params![
            payload.level,
            payload.goal,
            payload.daily_minutes as i64,
            payload.deadline,
            payload.repaired as i64,
            json,
            subject_key,
            grade_band,
            payload.exam_target,
            if payload.preferences.trim().is_empty() {
                None
            } else {
                Some(payload.preferences.trim().to_string())
            },
            now,
            id,
        ],
    )
    .map_err(|e| format!("finish plan failed: {e}"))?;
    store_payload_nodes(&conn, id, payload)?;
    Ok(())
}

/// 删除:db 行(级联 nodes/logs/units/quizzes/reviews)+ 题库/试卷文件 + 磁盘目录。
/// 题库/试卷已文件化(learningcenter),先清理文件再删 db 行,任一步失败即中止,避免留下不一致。
pub fn delete(id: &str) -> Result<(), String> {
    super::learn::bank::delete_scope(id)?;
    super::learn::paper::delete_by_plan(id)?;
    {
        let conn = db::conn().lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM plans WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
    }
    let dir = study_dir(id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("remove {} failed: {e}", dir.display()))?;
    }
    Ok(())
}

/// 计划下全部知识点的 (id, 名称, 难度),按入库顺序(完整教学内容批量生成用)。
pub fn node_ids(plan_id: &str) -> Result<Vec<(i64, String, i64)>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let mut st = conn
        .prepare("SELECT id, name, difficulty FROM knowledge_nodes WHERE plan_id=?1 ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows = st
        .query_map([plan_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// L1 同款:启动清理 — 生成中(generating)的任务是进程内任务,重启后无法恢复,标记为 error。
pub fn mark_interrupted(conn: &rusqlite::Connection) -> Result<usize, String> {
    let now = Local::now().to_rfc3339();
    conn.execute(
        "UPDATE plans SET status='error', error='应用重启中断', updated=?1 WHERE status='generating'",
        rusqlite::params![now],
    )
    .map_err(|e| format!("mark interrupted plans failed: {e}"))
}

// ---------------- trail / plan.json ----------------

/// 追加一条过程事件到 trail.jsonl(Done 的 plan 字段落盘时置 None,载荷在 plan.json)。
pub fn append_trail(id: &str, ev: &StudyEvent) -> Result<(), String> {
    let persisted = match ev {
        StudyEvent::Done { .. } => StudyEvent::Done { plan: None },
        other => other.clone(),
    };
    let dir = study_dir(id);
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    let line = serde_json::to_string(&persisted).map_err(|e| e.to_string())?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("trail.jsonl"))
        .map_err(|e| format!("open trail.jsonl failed: {e}"))?;
    f.write_all(line.as_bytes())
        .and_then(|_| f.write_all(b"\n"))
        .map_err(|e| format!("append trail failed: {e}"))?;
    Ok(())
}

/// 回放 trail.jsonl(跳过无法解析的行)。
pub fn read_trail(id: &str) -> Result<Vec<StudyEvent>, String> {
    let p = study_dir(id).join("trail.jsonl");
    if !p.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&p).map_err(|e| format!("read trail failed: {e}"))?;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(ev) = serde_json::from_str::<StudyEvent>(line) {
            out.push(ev);
        }
    }
    Ok(out)
}

pub fn save_payload(id: &str, payload: &PlanPayload) -> Result<(), String> {
    let dir = study_dir(id);
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir failed: {e}"))?;
    let text = serde_json::to_string_pretty(payload).map_err(|e| e.to_string())?;
    fs::write(dir.join("plan.json"), text).map_err(|e| format!("write plan.json failed: {e}"))
}

pub fn read_payload(id: &str) -> Option<PlanPayload> {
    let p = study_dir(id).join("plan.json");
    let text = fs::read_to_string(p).ok()?;
    serde_json::from_str(&text).ok()
}

/// 追加一条行为日志(P0:深挖研究联动)。
pub fn add_log(
    conn: &rusqlite::Connection,
    plan_id: &str,
    node_id: Option<i64>,
    kind: &str,
    minutes: i64,
) -> Result<(), String> {
    let now = Local::now().to_rfc3339();
    conn.execute(
        "INSERT INTO study_logs(plan_id, node_id, kind, minutes, at) VALUES(?1,?2,?3,?4,?5)",
        rusqlite::params![plan_id, node_id, kind, minutes, now],
    )
    .map_err(|e| format!("insert study_logs failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_interrupted_only_touches_generating() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        for (id, status) in [
            ("bbbb0001", "generating"),
            ("bbbb0002", "active"),
            ("bbbb0003", "done"),
            ("bbbb0004", "error"),
        ] {
            conn.execute(
                "INSERT INTO plans(id, topic, status, created, updated)
                 VALUES(?1, 't', ?2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                rusqlite::params![id, status],
            )
            .unwrap();
        }

        let n = mark_interrupted(&conn).unwrap();
        assert_eq!(n, 1, "仅 generating 一行受影响");
        let st: String = conn
            .query_row("SELECT status FROM plans WHERE id='bbbb0001'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(st, "error");
        let st2: String = conn
            .query_row("SELECT status FROM plans WHERE id='bbbb0002'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(st2, "active");
        assert_eq!(mark_interrupted(&conn).unwrap(), 0);
    }
}
