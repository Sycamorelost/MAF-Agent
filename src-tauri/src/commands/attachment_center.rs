//! 附件中心增值命令(迭代 F7.1):批量清理 + 提取失败重试。
//!
//! 批量删除复用 `delete_attachment` 的 DB 逻辑(`attachment::delete_one_on`),
//! 逐条返回结果(失败不影响其他条目);重试则重新读取磁盘文件并刷新元数据。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::commands::attachment::{
    count_doc_id_refs_on, delete_one_on, display_name_from_path, ensure_in_attachments_dir,
    extract_and_index, AttachmentMeta, MAX_ATTACHMENT_BYTES,
};

/// 批量删除请求条目
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BatchDeleteItem {
    pub message_id: i64,
    pub path: String,
}

/// 批量删除的单条结果(前端据此保留失败项)
#[derive(Debug, Clone, serde::Serialize)]
pub struct BatchDeleteOutcome {
    pub message_id: i64,
    pub path: String,
    pub ok: bool,
    pub error: Option<String>,
}

/// 单次批量清理的条目上限(防御一次 IPC 删除过多)
const MAX_BATCH_DELETE: usize = 500;

/// 批量删除附件:逐条执行,单条失败不影响其他条目;磁盘文件仅在全库无引用时清理。
#[tauri::command]
pub async fn delete_attachments_batch(
    items: Vec<BatchDeleteItem>,
) -> Result<Vec<BatchDeleteOutcome>, String> {
    if items.len() > MAX_BATCH_DELETE {
        return Err(format!(
            "一次最多清理 {} 个附件,请分批操作",
            MAX_BATCH_DELETE
        ));
    }
    let mut outcomes: Vec<BatchDeleteOutcome> = Vec::with_capacity(items.len());
    let mut to_remove: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<(i64, String)> = HashSet::new();
    {
        let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
        for item in &items {
            // 同一条目重复提交时只处理一次(前端已去重,这里做防御)
            if !seen.insert((item.message_id, item.path.clone())) {
                continue;
            }
            let p = Path::new(&item.path);
            let result = ensure_in_attachments_dir(p)
                .and_then(|_| delete_one_on(&conn, item.message_id, &item.path));
            match result {
                Ok(refs) => {
                    if refs == 0 {
                        to_remove.push(p.to_path_buf());
                    }
                    outcomes.push(BatchDeleteOutcome {
                        message_id: item.message_id,
                        path: item.path.clone(),
                        ok: true,
                        error: None,
                    });
                }
                Err(error) => outcomes.push(BatchDeleteOutcome {
                    message_id: item.message_id,
                    path: item.path.clone(),
                    ok: false,
                    error: Some(error),
                }),
            }
        }
    }
    // 释放连接后再动磁盘,避免文件删除耗时占住 DB 锁
    for p in to_remove {
        if !p.exists() {
            continue;
        }
        if let Err(e) = std::fs::remove_file(&p) {
            let msg = format!("删除磁盘文件失败: {e}");
            let path_str = p.to_string_lossy().to_string();
            if let Some(out) = outcomes
                .iter_mut()
                .find(|o| o.ok && o.path == path_str)
            {
                out.ok = false;
                out.error = Some(msg);
            }
        }
    }
    Ok(outcomes)
}

/// 把某消息中指定路径的附件元数据替换为新元数据;返回被替换的旧 doc_id。
pub(crate) fn replace_meta_on(
    conn: &rusqlite::Connection,
    message_id: i64,
    path: &str,
    new_meta: &AttachmentMeta,
) -> Result<Option<String>, String> {
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
    let mut old_doc: Option<String> = None;
    let mut found = false;
    for meta in metas.iter_mut() {
        if meta.path != path {
            continue;
        }
        found = true;
        if old_doc.is_none() {
            old_doc = meta.doc_id.clone();
        }
        *meta = new_meta.clone();
    }
    if !found {
        return Err("该消息中未找到此附件".into());
    }
    let new_json = serde_json::to_string(&metas).map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE messages SET attachments=?1 WHERE id=?2",
        rusqlite::params![new_json, message_id],
    )
    .map_err(|e| format!("更新消息附件失败: {e}"))?;
    Ok(old_doc)
}

/// 重试文本提取:文件仍存在时重新解析并刷新元数据;仍失败则返回带 `extract_note` 的元数据。
#[tauri::command]
pub async fn retry_attachment_extract(
    message_id: i64,
    path: String,
) -> Result<AttachmentMeta, String> {
    let p = Path::new(&path);
    ensure_in_attachments_dir(p)?;
    let fs_meta = std::fs::metadata(p).map_err(|e| format!("附件文件已丢失或不可读: {e}"))?;
    if fs_meta.len() > MAX_ATTACHMENT_BYTES {
        return Err("附件超过 20MB 上限".into());
    }
    let session_id: String = {
        let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT session_id FROM messages WHERE id=?1",
            rusqlite::params![message_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("消息不存在: {e}"))?
    };
    let name = display_name_from_path(p);
    let kind = crate::attachment::classify(&name);
    if matches!(
        kind,
        crate::attachment::AttachmentKind::Image | crate::attachment::AttachmentKind::Other
    ) {
        return Err("该格式不做文本提取,无需重试".into());
    }
    let (extracted_text, extract_note, chunk_count, summary, doc_id) =
        extract_and_index(&session_id, &name, kind, &path);
    let meta = AttachmentMeta {
        name,
        path,
        kind: kind.as_str().to_string(),
        size: fs_meta.len(),
        extracted_text,
        chunk_count,
        summary,
        doc_id,
        extract_note,
    };
    // 仍失败时不改写元数据,避免把已有的可用旧数据冲掉
    if meta.extracted_text.is_some() {
        let old_doc = {
            let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
            replace_meta_on(&conn, message_id, &meta.path, &meta)?
        };
        // 内容更新后旧文档块若已无引用,顺手清理,避免 doc_search 命中过期内容
        if let Some(old) = old_doc {
            if meta.doc_id.as_deref() != Some(old.as_str()) {
                let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
                if count_doc_id_refs_on(&conn, &session_id, &old)? == 0 {
                    crate::doc::service::purge(&conn, &session_id, &old)?;
                }
            }
        }
    }
    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, title, created, updated) VALUES('s1','重试测试','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        crate::db::branches::ensure_main_on(&conn, "s1").unwrap();
        conn
    }

    fn meta(name: &str, path: &str) -> AttachmentMeta {
        AttachmentMeta {
            name: name.into(),
            path: path.into(),
            kind: "pdf".into(),
            size: 10,
            extracted_text: None,
            doc_id: None,
            chunk_count: None,
            summary: None,
            extract_note: Some("提取失败".into()),
        }
    }

    fn add_msg(conn: &rusqlite::Connection, metas: &[AttachmentMeta]) -> i64 {
        crate::db::messages::append_on(
            conn,
            "s1",
            "user",
            "重试消息",
            None,
            0,
            Some(&serde_json::to_string(metas).unwrap()),
            None,
            None,
            "main",
        )
        .unwrap()
    }

    #[test]
    fn replace_meta_updates_only_target_path() {
        let conn = setup_db();
        let a = meta("a.pdf", r"D:\att\s1\a.pdf");
        let b = meta("b.pdf", r"D:\att\s1\b.pdf");
        let msg = add_msg(&conn, &[a.clone(), b.clone()]);

        let mut fresh = a.clone();
        fresh.extracted_text = Some("新正文".into());
        fresh.extract_note = None;
        fresh.doc_id = Some("doc-new".into());
        let old = replace_meta_on(&conn, msg, &a.path, &fresh).unwrap();
        assert_eq!(old, None);

        let raw: String = conn
            .query_row("SELECT attachments FROM messages WHERE id=?1", [msg], |r| {
                r.get(0)
            })
            .unwrap();
        let list: Vec<AttachmentMeta> = serde_json::from_str(&raw).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].extracted_text.as_deref(), Some("新正文"));
        assert_eq!(list[0].doc_id.as_deref(), Some("doc-new"));
        // 其他附件保持不变
        assert_eq!(list[1].name, "b.pdf");
        assert!(list[1].extracted_text.is_none());

        // 路径不存在 / 消息不存在
        assert!(replace_meta_on(&conn, msg, r"D:\att\s1\ghost.pdf", &fresh).is_err());
        assert!(replace_meta_on(&conn, 99999, &a.path, &fresh).is_err());
    }

    #[test]
    fn replace_meta_returns_previous_doc_id() {
        let conn = setup_db();
        let mut a = meta("a.pdf", r"D:\att\s1\a.pdf");
        a.doc_id = Some("old-doc".into());
        let msg = add_msg(&conn, &[a.clone()]);
        let mut fresh = a.clone();
        fresh.doc_id = Some("new-doc".into());
        assert_eq!(
            replace_meta_on(&conn, msg, &a.path, &fresh).unwrap().as_deref(),
            Some("old-doc")
        );
    }

    #[test]
    fn delete_one_returns_remaining_references() {
        let conn = setup_db();
        let a = meta("a.pdf", r"D:\att\s1\a.pdf");
        let m1 = add_msg(&conn, &[a.clone()]);
        let m2 = add_msg(&conn, &[a.clone()]);
        // 同一路径被两条消息引用:第一次删除后仍剩 1 个引用(磁盘文件保留)
        assert_eq!(delete_one_on(&conn, m1, &a.path).unwrap(), 1);
        // 最后一次引用被移除:返回 0,调用方据此清理磁盘与文档块
        assert_eq!(delete_one_on(&conn, m2, &a.path).unwrap(), 0);
        // 重复删除同一条目:消息中已无该路径,报错但不影响其他数据
        assert!(delete_one_on(&conn, m2, &a.path).is_err());
    }
}
