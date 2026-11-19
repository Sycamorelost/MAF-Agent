//! 附件管理中心(迭代 F7):聚合历史附件、再次引入会话、删除与引用统计。

use std::collections::HashMap;

use serde::Serialize;

use crate::attachment;

use super::meta::{
    AttachmentMeta, MAX_ATTACHMENT_BYTES, ensure_in_attachments_dir, extract_and_index,
};

/// 会话级文档清单条目(常驻系统提示的 doc manifest / 附件中心解析信息)。
#[derive(Debug, Clone, Serialize)]
pub struct SessionDoc {
    pub doc_id: String,
    pub doc_name: String,
    pub kind: String,
    /// 驻留块数
    pub chunks: i64,
    /// 正文字符数
    pub chars: i64,
    /// 上传时生成的 200 字摘要(可能缺省)
    pub summary: Option<String>,
    /// 首次入库时间
    pub created: String,
}

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

/// 会话级文档清单:按 doc_id 聚合块数/字符数/时间,并回填上传时的摘要。
/// 摘要不落在 `doc_chunks`,只能从消息附件 JSON 反查(同会话扫描一次)。
pub(crate) fn list_session_docs_on(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> Result<Vec<SessionDoc>, String> {
    let mut summaries: HashMap<String, String> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT attachments FROM messages
                 WHERE session_id=?1 AND attachments IS NOT NULL AND attachments <> '[]'",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![session_id], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let raw = row.map_err(|e| e.to_string())?;
            let Ok(list) = serde_json::from_str::<Vec<AttachmentMeta>>(&raw) else {
                continue;
            };
            for m in list {
                if let (Some(id), Some(sum)) = (m.doc_id, m.summary) {
                    summaries.entry(id).or_insert(sum);
                }
            }
        }
    }

    let mut stmt = conn
        .prepare(
            "SELECT doc_id, doc_name, kind, COUNT(*), COALESCE(SUM(length(content)),0), MIN(created)
             FROM doc_chunks WHERE session_id=?1
             GROUP BY doc_id, doc_name, kind ORDER BY MIN(id)",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![session_id], |r| {
            Ok(SessionDoc {
                doc_id: r.get(0)?,
                doc_name: r.get(1)?,
                kind: r.get(2)?,
                chunks: r.get(3)?,
                chars: r.get(4)?,
                summary: None,
                created: r.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out: Vec<SessionDoc> = rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?;
    for d in out.iter_mut() {
        d.summary = summaries.get(&d.doc_id).cloned();
    }
    Ok(out)
}

/// 列出本会话已解析文档(系统提示 doc manifest / 附件中心会话视图)。
#[tauri::command]
pub async fn list_session_docs(session_id: String) -> Result<Vec<SessionDoc>, String> {
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    list_session_docs_on(&conn, &session_id)
}

/// 从消息的 attachments JSON 中定位指定路径的元数据,返回(来源会话, 元数据)。
pub(crate) fn find_meta_on(
    conn: &rusqlite::Connection,
    message_id: i64,
    path: &str,
) -> Result<(String, AttachmentMeta), String> {
    let (session_id, raw): (String, Option<String>) = conn
        .query_row(
            "SELECT session_id, attachments FROM messages WHERE id=?1",
            rusqlite::params![message_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("消息不存在: {e}"))?;
    let metas: Vec<AttachmentMeta> = raw
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    metas
        .into_iter()
        .find(|m| m.path == path)
        .map(|m| (session_id, m))
        .ok_or_else(|| "该消息中未找到此附件".to_string())
}

/// 复用解析:源会话有该文档的块时复制到目标会话(同会话不复制,只补块数)。
/// 返回 (元数据, 是否复用成功)。未解析过的附件由调用方回退重新提取。
pub(crate) fn reuse_parsed_on(
    conn: &rusqlite::Connection,
    source_session: &str,
    target_session: &str,
    mut meta: AttachmentMeta,
) -> Result<(AttachmentMeta, bool), String> {
    let has_chunks = match meta.doc_id.as_deref() {
        Some(doc_id) => conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM doc_chunks WHERE session_id=?1 AND doc_id=?2)",
                rusqlite::params![source_session, doc_id],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|e| e.to_string())?,
        None => false,
    };
    if !has_chunks {
        return Ok((meta, false));
    }
    if source_session != target_session
        && let Some(doc_id) = meta.doc_id.as_deref()
    {
        meta.chunk_count = Some(crate::doc::copy_document(
            conn,
            source_session,
            target_session,
            doc_id,
        )?);
    }
    // 源侧解析成功:清掉历史失败提示,避免卡片误标「未解析」
    meta.extract_note = None;
    Ok((meta, true))
}

/// 从附件中心把附件导入目标会话。
///
/// 与 `insert_attachment_to_session` 的区别:源会话已有该文档的解析块时,
/// **直接复制 `doc_chunks` + FTS,不重新读盘解析**(省一次 PDF/Office 解析);
/// 只有源侧从未解析成功时才回退到重新提取。
#[tauri::command]
pub async fn import_attachment_from_center(
    target_session_id: String,
    source_message_id: i64,
    path: String,
) -> Result<AttachmentMeta, String> {
    crate::memory::validate_id(&target_session_id)?;
    let p = std::path::Path::new(&path);
    ensure_in_attachments_dir(p)?;
    let fs_meta = std::fs::metadata(p).map_err(|e| format!("附件不存在或不可读: {e}"))?;
    if fs_meta.len() > MAX_ATTACHMENT_BYTES {
        return Err("附件超过 20MB 上限".into());
    }
    let path_str = p.to_string_lossy().to_string();

    // 复用解析:读源元数据 → 配额校验 → 复制块(同一把锁内完成,避免元数据与块不一致)
    let (mut out, reused) = {
        let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
        let (source_session, src_meta) = find_meta_on(&conn, source_message_id, &path_str)?;
        crate::commands::attachment_quota::check_quota_on(
            &conn,
            &target_session_id,
            Some(&path_str),
            fs_meta.len(),
        )?;
        reuse_parsed_on(&conn, &source_session, &target_session_id, src_meta)?
    };

    out.path = path_str.clone();
    out.size = fs_meta.len();
    if !reused {
        // 回退:源会话没有可复用的解析结果,重新提取一次(与旧行为一致)
        let kind = attachment::classify(&out.name);
        if !matches!(
            kind,
            attachment::AttachmentKind::Image | attachment::AttachmentKind::Other
        ) {
            let (text, note, chunks, summary, doc_id) =
                extract_and_index(&target_session_id, &out.name, kind, &path_str);
            out.extracted_text = text;
            out.extract_note = note;
            out.chunk_count = chunks;
            out.summary = summary;
            out.doc_id = doc_id;
        }
    }
    Ok(out)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        for sid in ["s1", "s2"] {
            conn.execute(
                "INSERT INTO sessions(id,title,created,updated) VALUES(?1,'t','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
                rusqlite::params![sid],
            )
            .unwrap();
        }
        conn
    }

    fn add_msg(conn: &rusqlite::Connection, session: &str, metas: &[AttachmentMeta]) -> i64 {
        crate::db::messages::append_on(
            conn,
            session,
            "user",
            "x",
            None,
            0,
            Some(&serde_json::to_string(metas).unwrap()),
            None,
            None,
            "main",
        )
        .unwrap()
    }

    fn meta(name: &str, path: &str, doc_id: Option<&str>) -> AttachmentMeta {
        AttachmentMeta {
            name: name.into(),
            path: path.into(),
            kind: "pdf".into(),
            size: 10,
            extracted_text: Some("摘录".into()),
            doc_id: doc_id.map(|s| s.into()),
            chunk_count: None,
            summary: None,
            extract_note: None,
        }
    }

    #[test]
    fn list_session_docs_reports_parse_info() {
        let conn = setup();
        let text = "## 一\n多智能体协作提升完成率。";
        let r = crate::doc::service::ingest(&conn, "s1", "a.pdf", "pdf", text);
        let doc_id = r.doc_id.clone().unwrap();
        let mut m = meta("a.pdf", r"D:\att\s1\a.pdf", Some(&doc_id));
        m.summary = r.summary.clone();
        add_msg(&conn, "s1", &[m]);

        let docs = list_session_docs_on(&conn, "s1").unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].doc_name, "a.pdf");
        assert_eq!(docs[0].chunks, r.chunks.unwrap() as i64);
        assert_eq!(docs[0].summary, r.summary);
        assert!(!docs[0].created.is_empty());
    }

    #[test]
    fn list_session_docs_empty() {
        let conn = setup();
        assert!(list_session_docs_on(&conn, "s1").unwrap().is_empty());
    }

    #[test]
    fn find_meta_locates_path_and_reports_missing() {
        let conn = setup();
        let msg = add_msg(&conn, "s1", &[meta("a.pdf", r"D:\att\s1\a.pdf", None)]);
        let (sid, m) = find_meta_on(&conn, msg, r"D:\att\s1\a.pdf").unwrap();
        assert_eq!(sid, "s1");
        assert_eq!(m.name, "a.pdf");
        assert!(find_meta_on(&conn, msg, r"D:\att\s1\ghost.pdf").is_err());
        assert!(find_meta_on(&conn, 99999, r"D:\att\s1\a.pdf").is_err());
    }

    #[test]
    fn reuse_parsed_copies_chunks_to_target() {
        let conn = setup();
        let r = crate::doc::service::ingest(&conn, "s1", "a.pdf", "pdf", "## 一\n乙丙丁乙丙丁");
        let doc_id = r.doc_id.unwrap();
        let mut m = meta("a.pdf", r"D:\att\s1\a.pdf", Some(&doc_id));
        m.extract_note = Some("旧的失败提示".into());

        let (out, reused) = reuse_parsed_on(&conn, "s1", "s2", m).unwrap();
        assert!(reused);
        assert_eq!(out.chunk_count, r.chunks);
        assert!(out.extract_note.is_none(), "解析成功应清掉失败提示");

        let target_docs = crate::doc::list_docs(&conn, "s2").unwrap();
        assert_eq!(target_docs.len(), 1);
        assert_eq!(target_docs[0].doc_id, doc_id);
        assert!(crate::doc::search(&conn, "s2", "乙丙丁", None, 5)
            .unwrap()
            .hits
            .len()
            >= 1);
    }

    #[test]
    fn reuse_parsed_falls_back_when_unparsed() {
        let conn = setup();
        let (out, reused) =
            reuse_parsed_on(&conn, "s1", "s2", meta("a.pdf", r"D:\att\s1\a.pdf", None)).unwrap();
        assert!(!reused);
        assert!(out.chunk_count.is_none());
        assert!(crate::doc::list_docs(&conn, "s2").unwrap().is_empty());
    }
}
