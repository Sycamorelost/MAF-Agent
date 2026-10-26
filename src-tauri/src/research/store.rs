//! 深度研究会话的持久化。
//!
//! - db:`research_sessions` 表行(元数据)
//! - 磁盘:`data/research/{id}/trail.jsonl`(过程事件)、`report.md`(最终报告)、
//!   `stages.json`(专项模式阶段编排快照 + 运行期状态,文件即事实源)

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use chrono::Local;
use serde::Serialize;

use crate::db;

use super::{ResearchEvent, stage::StagesFile};

/// research_sessions 表行。
#[derive(Debug, Clone, Serialize)]
pub struct ResearchRow {
    pub id: String,
    pub topic: String,
    pub status: String, // running / done / error / cancelled
    pub rounds: i64,
    pub max_rounds: i64,
    pub model_id: Option<String>,
    pub error: Option<String>,
    pub created: String,
    pub updated: String,
    /// 研究模式:legacy / staged(专项)
    pub mode: String,
    /// 当前/最后执行的阶段序号(staged 模式;0 = 未开始)
    pub current_stage: i64,
    /// 来源研究方案名(步骤 4 方案库;当前恒为 None)
    pub blueprint_name: Option<String>,
    /// 已完成阶段数(M4:阶段状态迁移时回写;legacy 恒 0)
    pub stages_done: i64,
    /// 启用阶段总数(M4:启动时回写;legacy 恒 0)
    pub stages_total: i64,
}

/// 研究目录:`<data_dir>/research/{id}`
pub fn research_dir(id: &str) -> PathBuf {
    crate::paths::data_dir().join("research").join(id)
}

// ---------------- db ----------------

pub fn insert(row: &ResearchRow) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO research_sessions(id, topic, status, rounds, max_rounds, model_id, error, created, updated, mode, current_stage, blueprint_name, stages_done, stages_total)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        rusqlite::params![
            row.id,
            row.topic,
            row.status,
            row.rounds,
            row.max_rounds,
            row.model_id,
            row.error,
            row.created,
            row.updated,
            row.mode,
            row.current_stage,
            row.blueprint_name,
            row.stages_done,
            row.stages_total,
        ],
    )
    .map_err(|e| format!("insert research_sessions failed: {e}"))?;
    Ok(())
}

const COLS: &str = "id, topic, status, rounds, max_rounds, model_id, error, created, updated, mode, current_stage, blueprint_name, stages_done, stages_total";

fn row_from(r: &rusqlite::Row) -> rusqlite::Result<ResearchRow> {
    Ok(ResearchRow {
        id: r.get(0)?,
        topic: r.get(1)?,
        status: r.get(2)?,
        rounds: r.get(3)?,
        max_rounds: r.get(4)?,
        model_id: r.get(5)?,
        error: r.get(6)?,
        created: r.get(7)?,
        updated: r.get(8)?,
        mode: r.get(9)?,
        current_stage: r.get(10)?,
        blueprint_name: r.get(11)?,
        stages_done: r.get(12)?,
        stages_total: r.get(13)?,
    })
}

pub fn get(id: &str) -> Result<Option<ResearchRow>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!("SELECT {COLS} FROM research_sessions WHERE id=?1"))
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query_map([id], row_from).map_err(|e| e.to_string())?;
    match rows.next() {
        Some(Ok(r)) => Ok(Some(r)),
        Some(Err(e)) => Err(e.to_string()),
        None => Ok(None),
    }
}

/// 按创建时间倒序列出全部研究会话。
pub fn list() -> Result<Vec<ResearchRow>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLS} FROM research_sessions ORDER BY created DESC"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], row_from).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

pub fn update_status(id: &str, status: &str, error: Option<&str>) -> Result<(), String> {
    let now = Local::now().to_rfc3339();
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE research_sessions SET status=?1, error=?2, updated=?3 WHERE id=?4",
        rusqlite::params![status, error, now, id],
    )
    .map_err(|e| format!("update research status failed: {e}"))?;
    Ok(())
}

pub fn update_rounds(id: &str, rounds: i64) -> Result<(), String> {
    let now = Local::now().to_rfc3339();
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE research_sessions SET rounds=?1, updated=?2 WHERE id=?3",
        rusqlite::params![rounds, now, id],
    )
    .map_err(|e| format!("update research rounds failed: {e}"))?;
    Ok(())
}

/// 更新当前阶段序号(staged 模式)。
pub fn update_stage(id: &str, stage: i64) -> Result<(), String> {
    let now = Local::now().to_rfc3339();
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE research_sessions SET current_stage=?1, updated=?2 WHERE id=?3",
        rusqlite::params![stage, now, id],
    )
    .map_err(|e| format!("update research stage failed: {e}"))?;
    Ok(())
}

/// M4:回写阶段进度(stages_done/stages_total),列表页直接读列、零文件 IO。
pub fn update_stage_progress(id: &str, done: i64, total: i64) -> Result<(), String> {
    let now = Local::now().to_rfc3339();
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE research_sessions SET stages_done=?1, stages_total=?2, updated=?3 WHERE id=?4",
        rusqlite::params![done, total, now, id],
    )
    .map_err(|e| format!("update research stage progress failed: {e}"))?;
    Ok(())
}

/// L1:启动清理 — 研究任务是进程内任务,应用重启后 running/paused 行永远无法恢复,
/// 统一标记为 error(「应用重启中断」),避免历史列表出现永远「进行中」的假状态。
pub fn mark_interrupted(conn: &rusqlite::Connection) -> Result<usize, String> {
    let now = Local::now().to_rfc3339();
    conn.execute(
        "UPDATE research_sessions SET status='error', error='应用重启中断', updated=?1
         WHERE status IN ('running','paused')",
        rusqlite::params![now],
    )
    .map_err(|e| format!("mark interrupted research failed: {e}"))
}

/// 删除:db 行 + 磁盘目录。
pub fn delete(id: &str) -> Result<(), String> {
    delete_row(id)?;
    let dir = research_dir(id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("remove {} failed: {e}", dir.display()))?;
    }
    Ok(())
}

/// 仅删除数据库行(N-7 兜底):引擎卡死超时后走这里——目录保留,
/// 避免运行中的引擎在下一事件点把目录重建并继续写盘(「删而又生」)。
pub fn delete_row(id: &str) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM research_sessions WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------- trail / report 文件 ----------------

/// 追加一条过程事件到 trail.jsonl。
/// - ReportDelta 不落盘(量太大,报告最终存 report.md)
/// - Done 落盘时 report 字段置空(避免重复存储;正文在 report.md)
pub fn append_trail(id: &str, ev: &ResearchEvent) -> Result<(), String> {
    let persisted = match ev {
        ResearchEvent::ReportDelta { .. } => return Ok(()),
        ResearchEvent::Done { .. } => ResearchEvent::Done {
            report: String::new(),
        },
        other => other.clone(),
    };
    let dir = research_dir(id);
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
pub fn read_trail(id: &str) -> Result<Vec<ResearchEvent>, String> {
    let p = research_dir(id).join("trail.jsonl");
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
        if let Ok(ev) = serde_json::from_str::<ResearchEvent>(line) {
            out.push(ev);
        }
    }
    Ok(out)
}

pub fn save_report(id: &str, report: &str) -> Result<(), String> {
    let dir = research_dir(id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("report.md"), report).map_err(|e| format!("write report.md failed: {e}"))?;
    Ok(())
}

pub fn read_report(id: &str) -> Result<Option<String>, String> {
    let p = research_dir(id).join("report.md");
    if !p.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&p).map_err(|e| format!("read report.md failed: {e}"))?;
    Ok(Some(text))
}

// ---------------- stages.json(专项模式) ----------------

/// 写入阶段编排快照 + 运行期状态(每阶段状态迁移时调用)。
pub fn save_stages(id: &str, file: &StagesFile) -> Result<(), String> {
    let dir = research_dir(id);
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    let text = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    fs::write(dir.join("stages.json"), text).map_err(|e| format!("write stages.json failed: {e}"))
}

/// 读取 stages.json(缺失/损坏返回 None,不影响历史回放)。
pub fn read_stages(id: &str) -> Option<StagesFile> {
    let p = research_dir(id).join("stages.json");
    let text = fs::read_to_string(p).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// L1:启动清理把 running/paused 标记为 error(应用重启中断),其余状态不受影响。
    #[test]
    fn mark_interrupted_only_touches_active_rows() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        for (id, status) in [
            ("aaaa0001", "running"),
            ("aaaa0002", "paused"),
            ("aaaa0003", "done"),
            ("aaaa0004", "error"),
            ("aaaa0005", "cancelled"),
        ] {
            conn.execute(
                "INSERT INTO research_sessions(id, topic, status, created, updated)
                 VALUES(?1, 't', ?2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                rusqlite::params![id, status],
            )
            .unwrap();
        }

        let n = mark_interrupted(&conn).unwrap();
        assert_eq!(n, 2, "仅 running/paused 两行受影响");

        let get = |id: &str| -> (String, Option<String>) {
            conn.query_row(
                "SELECT status, error FROM research_sessions WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        };
        assert_eq!(get("aaaa0001").0, "error");
        assert_eq!(get("aaaa0001").1.as_deref(), Some("应用重启中断"));
        assert_eq!(get("aaaa0002").0, "error");
        assert_eq!(get("aaaa0003").0, "done");
        assert_eq!(get("aaaa0004").0, "error");
        assert_eq!(get("aaaa0005").0, "cancelled");

        // 幂等:二次执行不再有影响行
        assert_eq!(mark_interrupted(&conn).unwrap(), 0);
    }
}
