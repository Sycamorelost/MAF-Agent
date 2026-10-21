//! memory.md 版本快照与回滚。

use std::{fs, path::PathBuf};

use chrono::Local;

use crate::db;

use super::{memory_path, validate_id, versions_dir, write_memory, MAX_VERSIONS};

/// 手动触发一次版本快照(用于「提炼前」等场景)。
pub fn snapshot_version(id: &str, note: &str) -> Result<PathBuf, String> {
    validate_id(id)?;
    let mem = memory_path(id);
    if !mem.exists() {
        return Err(format!("memory.md not exists for session {id}"));
    }
    let text = fs::read_to_string(&mem).map_err(|e| e.to_string())?;

    // 文件名版本快照:20261013_183425.md
    let ts = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let dir = versions_dir(id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(format!("{}.md", ts));
    fs::write(&target, &text).map_err(|e| e.to_string())?;

    // 表记录
    let now = Local::now().to_rfc3339();
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO memory_versions(session_id, snapshot, created, note) VALUES(?1,?2,?3,?4)",
        rusqlite::params![id, text, now, note],
    )
    .map_err(|e| format!("insert memory_versions failed: {e}"))?;

    Ok(target)
}

/// 列出会话的历史版本(按时间倒序)。
pub fn list_versions(id: &str) -> Result<Vec<(i64, String, String)>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, created, note FROM memory_versions WHERE session_id=?1 ORDER BY id DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// 读取指定历史版本的完整快照内容(迭代 D3.3 diff 视图用)。
pub fn read_version(id: &str, version_id: i64) -> Result<String, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT snapshot FROM memory_versions WHERE id=?1 AND session_id=?2",
        rusqlite::params![version_id, id],
        |r| r.get(0),
    )
    .map_err(|e| format!("version not found: {e}"))
}

/// 回滚到某个历史版本(从历史快照恢复 + 当前内容自动快照一次 note="rollback")。
pub fn rollback_to(id: &str, version_id: i64) -> Result<(), String> {
    let snapshot: String = {
        let conn = db::conn().lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT snapshot FROM memory_versions WHERE id=?1 AND session_id=?2",
            rusqlite::params![version_id, id],
            |r| r.get(0),
        )
        .map_err(|e| format!("version not found: {e}"))?
    };
    write_memory(id, &snapshot, "rollback")
}

/// 清理超出 MAX_VERSIONS 的最旧版本(表记录 + 磁盘快照文件同步删,M-12)。
pub(super) fn prune_versions(id: &str) -> Result<(), String> {
    {
        let conn = db::conn().lock().map_err(|e| e.to_string())?;
        // 查出 id 倒序第 N 个之后的版本
        let mut stmt = conn
            .prepare("SELECT id FROM memory_versions WHERE session_id=?1 ORDER BY id DESC LIMIT -1 OFFSET ?2")
            .map_err(|e| e.to_string())?;
        let stale: Vec<i64> = stmt
            .query_map(rusqlite::params![id, MAX_VERSIONS as i64], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        if !stale.is_empty() {
            let mut del = conn
                .prepare("DELETE FROM memory_versions WHERE id=?1")
                .map_err(|e| e.to_string())?;
            for vid in &stale {
                del.execute([vid]).map_err(|e| e.to_string())?;
            }
        }
    }
    // 磁盘对账:snapshot_version 同时落盘 memory.versions/{ts}.md,
    // 旧实现只删表记录导致 .md 永久残留;文件名等长且字典序即时间序,直接保留最新 N 个。
    prune_version_files(id, MAX_VERSIONS);
    Ok(())
}

/// 删除 memory.versions/ 中超出 keep 份的最旧快照文件。
/// 只清理应用自身命名(`YYYYMMDD_HHMMSS.md`)的文件,外部放入的其他文件不动;
/// 单个文件删除失败只记日志、不阻断写入流程(下次 prune 会再次尝试)。
fn prune_version_files(id: &str, keep: usize) {
    let dir = versions_dir(id);
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if is_version_filename(name) {
            files.push(p);
        }
    }
    if files.len() <= keep {
        return;
    }
    files.sort();
    let remove_count = files.len() - keep;
    for stale in files.into_iter().take(remove_count) {
        if let Err(e) = fs::remove_file(&stale) {
            log::warn!("删除过期 memory 版本快照失败 {}: {e}", stale.display());
        }
    }
}

/// 判定是否为应用生成的版本快照文件名:`YYYYMMDD_HHMMSS.md`(15 个 ASCII 数字/下划线 + .md)。
fn is_version_filename(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".md") else {
        return false;
    };
    let bytes = stem.as_bytes();
    bytes.len() == 15
        && bytes[8] == b'_'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| (i == 8 && *b == b'_') || b.is_ascii_digit())
}
