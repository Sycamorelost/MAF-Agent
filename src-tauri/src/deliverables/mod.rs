//! 产物(交付物)登记与查询(M2 / 阶段 A)。
//!
//! 模型通过内置工具 `present` 显式声明「本轮产出了哪些文件」,不再依赖正文里
//! 是否恰好写了路径。登记分两步:
//!   1. `present` 执行时写入一行,`message_id = NULL`(暂存,同一轮内按路径去重);
//!   2. 助手消息落库时 `bind_pending_on` 把该会话暂存行绑定到这条消息。
//!
//! 这样历史会话重新打开时可按 message_id 回读,正文解析只作兜底。
//! 表里只存路径引用与元数据,禁止存 base64 或临时文件内容。

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;

/// 单轮最多登记的产物数,避免模型一次性刷屏。
const MAX_PER_TURN: usize = 12;

/// 一条产物记录。`path` 优先存工作区相对路径(工作区外为绝对路径),
/// 前端交给 `fs_open` / `fs_reveal` 按会话工作区解析。
#[derive(Debug, Clone, Serialize)]
pub struct Deliverable {
    pub id: i64,
    pub session_id: String,
    /// 绑定的助手消息 id;NULL = 尚未落库(仅在流式过程中出现)
    pub message_id: Option<i64>,
    /// image / doc / sheet / slides / pdf / archive / text / html / file
    pub kind: String,
    pub path: String,
    pub name: String,
    pub mime: Option<String>,
    pub size: Option<i64>,
    /// 模型给出的一句话说明(可选)
    pub note: Option<String>,
    pub created: String,
}

const SELECT_COLS: &str = "id, session_id, message_id, kind, path, name, mime, size, note, created";

fn row_to_deliverable(r: &rusqlite::Row) -> rusqlite::Result<Deliverable> {
    Ok(Deliverable {
        id: r.get(0)?,
        session_id: r.get(1)?,
        message_id: r.get(2)?,
        kind: r.get(3)?,
        path: r.get(4)?,
        name: r.get(5)?,
        mime: r.get(6)?,
        size: r.get(7)?,
        note: r.get(8)?,
        created: r.get(9)?,
    })
}

/// `present` 工具的执行入口:解析参数 → 校验文件 → 登记 → 返回给模型的文字结果。
pub fn present(session_id: Option<&str>, args: &Value) -> (String, bool) {
    let Some(sid) = session_id.map(str::trim).filter(|s| !s.is_empty()) else {
        return ("登记交付物失败:缺少会话上下文。".into(), false);
    };
    let Ok(root) = crate::fs::paths::resolve_workspace_root(Some(sid)) else {
        return ("登记交付物失败:无法确定工作区。".into(), false);
    };
    let conn = match crate::db::conn().lock() {
        Ok(c) => c,
        Err(_) => return ("登记交付物失败:数据库不可用。".into(), false),
    };
    match register_pending_on(&conn, sid, &root, args) {
        Ok(d) => (
            format!(
                "已登记交付物:{} ({}),前端会在本条回答下方展示交付卡。",
                d.name, d.path
            ),
            true,
        ),
        Err(e) => (format!("登记交付物失败:{e}"), false),
    }
}

/// 登记一条暂存产物;同会话同路径(大小写不敏感)且尚未绑定时直接复用旧行。
fn register_pending_on(
    conn: &Connection,
    session_id: &str,
    root: &Path,
    args: &Value,
) -> Result<Deliverable, String> {
    let raw = args
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if raw.is_empty() {
        return Err("缺少 path 参数".into());
    }
    let abs = crate::fs::paths::resolve(root, raw, true)?;
    if !abs.is_file() {
        return Err(format!("`{raw}` 不是已存在的文件"));
    }
    // 工作区内用相对路径,工作区外保留绝对路径(两者都能被 fs_open / fs_reveal 解析)
    let path = crate::fs::paths::rel_of(root, &abs).unwrap_or_else(|_| abs.display().to_string());
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            abs.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone())
        });
    let size = std::fs::metadata(&abs).ok().map(|m| m.len() as i64);
    let mime = mime_for(&abs);
    let kind = args
        .get("kind")
        .and_then(|v| v.as_str())
        .map(normalize_kind)
        .unwrap_or_else(|| kind_for(&abs).to_string());
    let note = args
        .get("note")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    // 同轮重复声明同一文件:复用已有暂存行,避免交付卡重复
    let existing: Option<Deliverable> = conn
        .query_row(
            &format!(
                "SELECT {SELECT_COLS} FROM deliverables \
                 WHERE session_id=?1 AND message_id IS NULL AND lower(path)=lower(?2) \
                 ORDER BY id DESC LIMIT 1"
            ),
            params![session_id, path],
            row_to_deliverable,
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(d) = existing {
        return Ok(d);
    }

    let pending: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM deliverables WHERE session_id=?1 AND message_id IS NULL",
            params![session_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if pending as usize >= MAX_PER_TURN {
        return Err(format!("单轮最多登记 {MAX_PER_TURN} 个交付物"));
    }

    conn.execute(
        "INSERT INTO deliverables(session_id, message_id, kind, path, name, mime, size, note) \
         VALUES(?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![session_id, kind, path, name, mime, size, note],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    conn.query_row(
        &format!("SELECT {SELECT_COLS} FROM deliverables WHERE id=?1"),
        params![id],
        row_to_deliverable,
    )
    .map_err(|e| e.to_string())
}

/// 助手消息落库后,把该会话所有暂存产物绑定到这条消息。
/// 返回绑定条数;在消息写入事务内调用,失败随事务回滚。
pub fn bind_pending_on(
    conn: &Connection,
    session_id: &str,
    message_id: i64,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE deliverables SET message_id=?1 WHERE session_id=?2 AND message_id IS NULL",
        params![message_id, session_id],
    )
}

/// 按会话列出全部产物(新→旧),前端按 message_id 归入各条消息。
pub fn list_by_session_on(
    conn: &Connection,
    session_id: &str,
) -> rusqlite::Result<Vec<Deliverable>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLS} FROM deliverables WHERE session_id=?1 ORDER BY id ASC"
    ))?;
    let rows = stmt.query_map(params![session_id], row_to_deliverable)?;
    rows.collect()
}

/// 按会话列出全部产物(全局连接包装;库未就绪时返回空)。
pub fn list_by_session(session_id: &str) -> Vec<Deliverable> {
    if !crate::db::is_initialized() {
        return Vec::new();
    }
    let Ok(conn) = crate::db::conn().lock() else {
        return Vec::new();
    };
    list_by_session_on(&conn, session_id).unwrap_or_default()
}

/// 删除单条消息时清掉其产物(避免悬空引用)。
pub fn delete_by_message_on(conn: &Connection, message_id: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM deliverables WHERE message_id=?1",
        params![message_id],
    )
}

/// 新一轮用户消息到达:清掉上一轮遗留的暂存产物。
/// 上一轮若被取消/中断,`present` 登记的行可能永远没有助手消息可绑定,
/// 留着会占用单轮上限并错误地挂到下一条回答上。
pub fn clear_pending_on(conn: &Connection, session_id: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM deliverables WHERE session_id=?1 AND message_id IS NULL",
        params![session_id],
    )
}

/// 允许模型显式声明 kind 的白名单;未知值退回按扩展名推断。
fn normalize_kind(kind: &str) -> String {
    const ALLOWED: [&str; 9] = [
        "image", "doc", "sheet", "slides", "pdf", "archive", "text", "html", "file",
    ];
    let k = kind.trim().to_ascii_lowercase();
    if ALLOWED.contains(&k.as_str()) {
        k
    } else {
        "file".into()
    }
}

/// 按扩展名推断产物类型。
fn kind_for(path: &Path) -> &'static str {
    match ext_of(path).as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" => "image",
        "doc" | "docx" => "doc",
        "xls" | "xlsx" | "csv" | "tsv" => "sheet",
        "ppt" | "pptx" => "slides",
        "pdf" => "pdf",
        "zip" | "7z" | "tar" | "gz" => "archive",
        "md" | "txt" | "json" | "yaml" | "yml" | "log" => "text",
        "html" | "htm" => "html",
        _ => "file",
    }
}

/// 常见扩展名 → MIME;未收录返回 None(前端不依赖,只用于展示)。
fn mime_for(path: &Path) -> Option<String> {
    let mime = match ext_of(path).as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "csv" => "text/csv",
        "md" => "text/markdown",
        "txt" => "text/plain",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "zip" => "application/zip",
        _ => return None,
    };
    Some(mime.into())
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Connection, std::path::PathBuf, std::path::PathBuf) {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, title, created, updated) VALUES('s1','新会话','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let dir = std::env::temp_dir().join(format!("maf-deliverables-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("report.md");
        std::fs::write(&file, "hello").unwrap();
        (conn, dir, file)
    }

    fn args(file: &Path) -> Value {
        serde_json::json!({ "path": file.display().to_string(), "note": "周报" })
    }

    #[test]
    fn register_binds_and_reads_back() {
        let (conn, root, file) = setup();
        let d = register_pending_on(&conn, "s1", &root, &args(&file)).unwrap();
        assert_eq!(d.kind, "text");
        assert_eq!(d.name, "report.md");
        assert_eq!(d.size, Some(5));
        assert!(d.message_id.is_none());

        // 消息落库后绑定
        let n = bind_pending_on(&conn, "s1", 42).unwrap();
        assert_eq!(n, 1);
        let rows = list_by_session_on(&conn, "s1").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message_id, Some(42));
    }

    #[test]
    fn same_path_dedupes_within_turn() {
        let (conn, root, file) = setup();
        let a = register_pending_on(&conn, "s1", &root, &args(&file)).unwrap();
        let b = register_pending_on(&conn, "s1", &root, &args(&file)).unwrap();
        assert_eq!(a.id, b.id, "同一轮同路径只保留一条");
        assert_eq!(bind_pending_on(&conn, "s1", 1).unwrap(), 1);
        // 绑定后再次声明 → 新的一条(新消息应得到新卡片)
        let c = register_pending_on(&conn, "s1", &root, &args(&file)).unwrap();
        assert_ne!(a.id, c.id);
    }

    #[test]
    fn rejects_missing_and_directory() {
        let (conn, root, _file) = setup();
        let err = register_pending_on(
            &conn,
            "s1",
            &root,
            &serde_json::json!({ "path": "nope.md" }),
        )
        .unwrap_err();
        assert!(err.contains("不存在"), "实际:{err}");
        let dir_arg = serde_json::json!({ "path": root.display().to_string() });
        assert!(register_pending_on(&conn, "s1", &root, &dir_arg).is_err());
    }

    #[test]
    fn delete_by_message_clears_rows() {
        let (conn, root, file) = setup();
        register_pending_on(&conn, "s1", &root, &args(&file)).unwrap();
        bind_pending_on(&conn, "s1", 7).unwrap();
        assert_eq!(delete_by_message_on(&conn, 7).unwrap(), 1);
        assert!(list_by_session_on(&conn, "s1").unwrap().is_empty());
    }

    #[test]
    fn clear_pending_keeps_bound_rows() {
        let (conn, root, file) = setup();
        register_pending_on(&conn, "s1", &root, &args(&file)).unwrap();
        bind_pending_on(&conn, "s1", 11).unwrap();
        // 同一路径绑定后再登记,形成一条暂存行
        register_pending_on(&conn, "s1", &root, &args(&file)).unwrap();
        assert_eq!(clear_pending_on(&conn, "s1").unwrap(), 1);
        let rows = list_by_session_on(&conn, "s1").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message_id, Some(11));
    }

    #[test]
    fn kind_whitelist_falls_back_to_extension() {
        assert_eq!(normalize_kind("IMAGE"), "image");
        assert_eq!(normalize_kind("nonsense"), "file");
        assert_eq!(kind_for(Path::new("a.docx")), "doc");
        assert_eq!(kind_for(Path::new("a.unknown")), "file");
        assert_eq!(mime_for(Path::new("a.png")).as_deref(), Some("image/png"));
    }
}
