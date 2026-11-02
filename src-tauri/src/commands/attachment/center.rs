//! 附件管理中心(迭代 F7):聚合历史附件、再次引入会话、删除与引用统计。

use serde::Serialize;

use crate::attachment;

use super::meta::{AttachmentMeta, MAX_ATTACHMENT_BYTES, ensure_in_attachments_dir, extract_and_index};

/// 附件中心条目:展开 messages.attachments JSON,联表 sessions 取标题。
#[derive(Debug, Clone, Serialize)]
pub struct AttachmentEntry {
    pub message_id: i64,
    pub session_id: String,
    pub session_title: String,
    /// 来源消息所在分支(前端定位时需先切换分支)
    pub branch_id: String,
    /// 来源消息创建时间
    pub created: String,
    pub meta: AttachmentMeta,
}

/// 聚合全部历史附件(按消息 id 倒序 = 最新在前)。可测试核心。
pub(crate) fn list_all_on(conn: &rusqlite::Connection) -> Result<Vec<AttachmentEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.session_id, COALESCE(s.title,''), m.branch_id, m.created, m.attachments
             FROM messages m JOIN sessions s ON s.id = m.session_id
             WHERE m.attachments IS NOT NULL AND m.attachments <> '[]'
             ORDER BY m.id DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (message_id, session_id, session_title, branch_id, created, raw) =
            row.map_err(|e| e.to_string())?;
        let metas: Vec<AttachmentMeta> = serde_json::from_str(&raw).unwrap_or_default();
        for meta in metas {
            out.push(AttachmentEntry {
                message_id,
                session_id: session_id.clone(),
                session_title: session_title.clone(),
                branch_id: branch_id.clone(),
                created: created.clone(),
                meta,
            });
        }
    }
    Ok(out)
}

/// 列出全部历史附件(附件中心数据源)。
#[tauri::command]
pub async fn list_all_attachments() -> Result<Vec<AttachmentEntry>, String> {
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    list_all_on(&conn)
}

/// 从落盘文件名剥离 `{ts}_` 前缀,还原展示名(落盘格式:{yyyyMMdd-HHmmssSSS}_{safe})。
pub(crate) fn display_name_from_path(p: &std::path::Path) -> String {
    let file = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    match file.split_once('_') {
        Some((prefix, rest))
            if !rest.is_empty()
                && prefix.len() >= 8
                && prefix.chars().all(|c| c.is_ascii_digit() || c == '-') =>
        {
            rest.to_string()
        }
        _ => file,
    }
}

/// 把附件中心已有文件再次引入某会话:复用磁盘文件(不复制),重新生成元数据。
/// 文件可能被外部改动过,文本类重新提取保证注入内容新鲜。
#[tauri::command]
pub async fn insert_attachment_to_session(
    session_id: String,
    path: String,
) -> Result<AttachmentMeta, String> {
    crate::memory::validate_id(&session_id)?;
    let p = std::path::Path::new(&path);
    ensure_in_attachments_dir(p)?;
    let fs_meta = std::fs::metadata(p).map_err(|e| format!("附件不存在或不可读: {e}"))?;
    // H-3 复检:复用入口的文件可能在落盘后被外部替换为超大文件,再次施加 20MB 上限
    if fs_meta.len() > MAX_ATTACHMENT_BYTES {
        return Err("附件超过 20MB 上限".into());
    }
    // 会话配额:同一路径已在会话内复用时放行,新文件按数量 / 容量校验
    let path_str = p.to_string_lossy().to_string();
    {
        let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
        crate::commands::attachment_quota::check_quota_on(
            &conn,
            &session_id,
            Some(&path_str),
            fs_meta.len(),
        )?;
    }
    let name = display_name_from_path(p);
    let kind = attachment::classify(&name);
    let (extracted_text, extract_note, chunk_count, summary, doc_id) =
        extract_and_index(&session_id, &name, kind, &path_str);
    Ok(AttachmentMeta {
        doc_id,
        name,
        path: path_str,
        kind: kind.as_str().to_string(),
        size: fs_meta.len(),
        extracted_text,
        chunk_count,
        summary,
        extract_note,
    })
}

/// 从指定消息的 attachments JSON 中移除 path 条目,返回被移除的元数据。可测试核心。
pub(crate) fn remove_from_message_on(
    conn: &rusqlite::Connection,
    message_id: i64,
    path: &str,
) -> Result<Vec<AttachmentMeta>, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT attachments FROM messages WHERE id=?1",
            rusqlite::params![message_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("消息不存在: {e}"))?;
    let mut metas: Vec<AttachmentMeta> = raw
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    let mut removed = Vec::new();
    metas.retain(|m| {
        if m.path == path {
            removed.push(m.clone());
            false
        } else {
            true
        }
    });
    if removed.is_empty() {
        return Err("该消息中未找到此附件".into());
    }
    let new_json: Option<String> = if metas.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&metas).map_err(|e| e.to_string())?)
    };
    conn.execute(
        "UPDATE messages SET attachments=?1 WHERE id=?2",
        rusqlite::params![new_json, message_id],
    )
    .map_err(|e| format!("更新消息附件失败: {e}"))?;
    Ok(removed)
}

/// 同会话内仍在引用该 doc_id 的附件数(内容相同的重复上传会共享 doc_id)。
pub(crate) fn count_doc_id_refs_on(
    conn: &rusqlite::Connection,
    session_id: &str,
    doc_id: &str,
) -> Result<usize, String> {
    let mut stmt = conn
        .prepare(
            "SELECT attachments FROM messages
             WHERE session_id=?1 AND attachments IS NOT NULL AND attachments <> '[]'",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![session_id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut n = 0;
    for row in rows {
        let raw: String = row.map_err(|e| e.to_string())?;
        if let Ok(list) = serde_json::from_str::<Vec<AttachmentMeta>>(&raw) {
            n += list
                .iter()
                .filter(|m| m.doc_id.as_deref() == Some(doc_id))
                .count();
        }
    }
    Ok(n)
}

/// 删除单条消息中的一个附件引用(DB 部分),返回全库剩余引用数。
/// 全库无引用时级联清理该文档检索块(R13)。
pub(crate) fn delete_one_on(
    conn: &rusqlite::Connection,
    message_id: i64,
    path: &str,
) -> Result<usize, String> {
    let removed = remove_from_message_on(conn, message_id, path)?;
    let session_id: String = conn
        .query_row(
            "SELECT session_id FROM messages WHERE id=?1",
            rusqlite::params![message_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let refs = count_references_on(conn, path)?;
    if refs == 0 {
        for meta in &removed {
            let Some(doc_id) = meta.doc_id.as_deref() else {
                continue;
            };
            if count_doc_id_refs_on(conn, &session_id, doc_id)? == 0 {
                crate::doc::service::purge(conn, &session_id, doc_id)?;
            }
        }
    }
    Ok(refs)
}

/// 删除附件:从来源消息 JSON 移除条目;全库无其他引用时一并删除磁盘文件。
#[tauri::command]
pub async fn delete_attachment(message_id: i64, path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    ensure_in_attachments_dir(p)?;
    let refs = {
        let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
        delete_one_on(&conn, message_id, &path)?
    };
    // 无引用则清理磁盘(文件已不存在则忽略)
    if refs == 0 && p.exists() {
        std::fs::remove_file(p).map_err(|e| format!("删除磁盘文件失败: {e}"))?;
    }
    Ok(())
}

/// 全库精确统计同一文件路径的引用数(Rust 侧比对,避免 LIKE 通配符误伤)。可测试核心。
pub(crate) fn count_references_on(
    conn: &rusqlite::Connection,
    path: &str,
) -> Result<usize, String> {
    let mut stmt = conn
        .prepare("SELECT attachments FROM messages WHERE attachments IS NOT NULL AND attachments <> '[]'")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut n = 0;
    for row in rows {
        let raw: String = row.map_err(|e| e.to_string())?;
        if let Ok(list) = serde_json::from_str::<Vec<AttachmentMeta>>(&raw) {
            n += list.iter().filter(|m| m.path == path).count();
        }
    }
    Ok(n)
}
