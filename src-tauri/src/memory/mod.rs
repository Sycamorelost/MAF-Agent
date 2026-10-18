//! memory.md 管线核心 API。
//!
//! 对应 plan/01-phase1-mvp.md §3.2:
//!   create_session / read_memory / write_memory / watch_memory / summarize_memory
//!
//! 数据落盘(架构文档 idea/01-architecture.md):
//!   data\conversations\{id}\memory.md
//!   data\conversations\{id}\memory.versions\{时间戳}.md   (保留最近 20 版)

use std::{fs, path::PathBuf};

use chrono::Local;
use rand::Rng;

use crate::db;

pub mod budget;
pub mod frontmatter;
pub mod template;
pub mod watcher;

/// memory_versions 保留的最大份数(可配,先写死 20)
pub const MAX_VERSIONS: usize = 20;

// ---------------- 路径工具 ----------------

/// 校验会话/笔记 id 可安全用于文件路径(CWE-22 路径遍历防护)。
/// 规则:非空、≤64 字符、仅 [a-zA-Z0-9_-],且不含 ".." 序列。
pub fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 64 {
        return Err("id 长度非法".into());
    }
    if id.contains("..") {
        return Err("id 含非法序列".into());
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("id 含非法字符".into());
    }
    Ok(())
}

/// 会话目录:`<data_dir>/conversations/{id}`
pub fn session_dir(id: &str) -> PathBuf {
    crate::paths::data_dir().join("conversations").join(id)
}

pub fn memory_path(id: &str) -> PathBuf {
    session_dir(id).join("memory.md")
}

pub fn versions_dir(id: &str) -> PathBuf {
    session_dir(id).join("memory.versions")
}

// ---------------- ID 生成 ----------------

/// 8 位 hex 会话 id(从 u32 随机数派生)
pub fn new_session_id() -> String {
    let mut rng = rand::thread_rng();
    format!("{:08x}", rng.r#gen::<u32>())
}

// ---------------- 核心 API ----------------

/// 创建会话:建目录 + 写 memory.md 骨架 + sessions 表插入。
/// 返回 (session_id, memory_file_path)。
pub fn create_session(title: &str, model_id: &str) -> Result<(String, PathBuf), String> {
    let id = new_session_id();
    create_session_with_id(&id, title, model_id).map(|p| (id, p))
}

pub fn create_session_with_id(id: &str, title: &str, model_id: &str) -> Result<PathBuf, String> {
    validate_id(id)?;
    let dir = session_dir(id);
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    fs::create_dir_all(versions_dir(id)).map_err(|e| format!("mkdir versions failed: {e}"))?;

    let mem = memory_path(id);
    let content = template::render(id, title, model_id);
    fs::write(&mem, content).map_err(|e| format!("write memory.md failed: {e}"))?;

    // sessions 表插入
    let now = Local::now().to_rfc3339();
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO sessions(id, title, created, updated, model_id, memory_version) VALUES(?1,?2,?3,?3,?4,1)",
        rusqlite::params![id, title, now, model_id],
    )
    .map_err(|e| format!("insert sessions failed: {e}"))?;

    // 初始版本快照(note = "init")
    drop(conn);
    let _ = snapshot_version(id, "init");

    Ok(mem)
}

/// 读取 memory.md,解析 frontmatter + body。
pub fn read_memory(id: &str) -> Result<(frontmatter::Frontmatter, String), String> {
    validate_id(id)?;
    let p = memory_path(id);
    let text = fs::read_to_string(&p).map_err(|e| format!("read {} failed: {e}", p.display()))?;
    frontmatter::parse(&text)
}

/// 把会话标题同步进 memory.md frontmatter 的 conversation 字段。
/// 轻量直写:不产生版本快照、不 bump memory_version(仅元数据对齐,非内容变更)。
/// 文件不存在(会话目录被外部清理)时静默跳过。
pub fn sync_title(id: &str, title: &str) -> Result<(), String> {
    validate_id(id)?;
    let p = memory_path(id);
    if !p.exists() {
        return Ok(());
    }
    let text = fs::read_to_string(&p).map_err(|e| format!("read {} failed: {e}", p.display()))?;
    let (mut fm, body) = frontmatter::parse(&text)?;
    if fm.conversation == title {
        return Ok(());
    }
    fm.conversation = title.to_string();
    fm.updated = Local::now().to_rfc3339();
    let out = frontmatter::render(&fm, &body)?;
    fs::write(&p, out).map_err(|e| format!("write {} failed: {e}", p.display()))?;
    Ok(())
}

/// 写入 memory.md(整段覆盖)。
/// - 先把旧文件快照到 memory.versions/
/// - 插入 memory_versions 表
/// - 清超出 MAX_VERSIONS 的旧版本
/// - 更新 sessions.updated 和 memory_version
pub fn write_memory(id: &str, content: &str, note: &str) -> Result<(), String> {
    validate_id(id)?;
    let p = memory_path(id);

    // 先快照旧版本(若存在且与待写内容不同)
    if p.exists() {
        let old = fs::read_to_string(&p).map_err(|e| e.to_string())?;
        if old != content {
            snapshot_version(id, note)?;
        }
    }

    fs::write(&p, content).map_err(|e| format!("write {} failed: {e}", p.display()))?;

    // 更新 sessions
    let now = Local::now().to_rfc3339();
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sessions SET updated=?1, memory_version=memory_version+1 WHERE id=?2",
        rusqlite::params![now, id],
    )
    .map_err(|e| format!("update sessions failed: {e}"))?;

    drop(conn);
    prune_versions(id)?;
    Ok(())
}

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
fn prune_versions(id: &str) -> Result<(), String> {
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

// ---------------- 提炼(占位) ----------------

/// 提炼器在 Phase 1 仅做骨架:
/// 真实提炼依赖 Provider(§3.3)生成摘要;此处先实现「是否需要提炼」的判断。
///
/// 触发条件(idea/03-memory-flow.md §2.1):
///   - 距上次提炼已完成 N 轮对话(默认 2)
///   - 会话 token 超阈值(默认 8k)
pub fn needs_summarize(id: &str, every_n_rounds: i64, token_threshold: i64) -> Result<bool, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let (msg_count, total_tokens): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(tokens),0) FROM messages WHERE session_id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    Ok(msg_count >= every_n_rounds * 2 || total_tokens >= token_threshold)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_id_rejects_traversal() {
        assert!(validate_id("deadbeef").is_ok());
        assert!(validate_id("abc-123_x").is_ok());
        assert!(validate_id("").is_err());
        assert!(validate_id("../../etc").is_err());
        assert!(validate_id("..\\windows").is_err());
        assert!(validate_id("a/b").is_err());
        assert!(validate_id("a\\b").is_err());
        assert!(validate_id("a.b").is_err());
        assert!(validate_id(&"x".repeat(65)).is_err());
    }

    #[test]
    fn id_is_8_hex() {
        let id = new_session_id();
        assert_eq!(id.len(), 8);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn paths_are_under_data_conversations() {
        let p = session_dir("abc123de");
        let s = p.to_string_lossy();
        assert!(s.contains("conversations"));
        assert!(s.ends_with("abc123de"));
    }

    #[test]
    fn template_renders_six_known_sections() {
        let t = template::render("deadbeef", "集成测试", "gpt-5.2");
        for sec in template::KNOWN_SECTIONS {
            assert!(t.contains(&format!("# {}", sec)));
        }
    }

    #[test]
    fn frontmatter_roundtrip_preserves_extra() {
        let raw = "---\nid: x1\nextra_field: 自定义\n---\n\n# 会话摘要\n\nbody";
        let (fm, body) = frontmatter::parse(raw).unwrap();
        assert_eq!(fm.id, "x1");
        assert!(fm.extra.contains_key("extra_field"));
        let back = frontmatter::render(&fm, &body).unwrap();
        assert!(back.contains("extra_field"));
    }
}
