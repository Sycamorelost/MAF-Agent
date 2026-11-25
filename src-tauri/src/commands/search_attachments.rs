//! 全局搜索的附件分组:命中文件名、解析摘要或提取正文。
//!
//! 附件元数据没有独立表,统一存放在 `messages.attachments` JSON 数组里;
//! 这里先用 LIKE 粗筛带附件的消息,再反序列化逐字段匹配,按路径去重后截取片段。

use serde::Serialize;

use crate::commands::attachment::AttachmentMeta;

#[derive(Debug, Serialize)]
pub struct AttachmentHit {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub session_id: String,
    pub session_title: String,
    pub message_id: i64,
    pub branch_id: String,
    /// 命中字段的前后 40 字片段(文件名命中时直接返回文件名)
    pub snippet: String,
}

fn escape_like(q: &str) -> String {
    q.replace('%', "\\%").replace('_', "\\_")
}

/// 按字符截取命中位置前后各 40 字;`start`/`end` 用字符索引,避免中文按字节切坏。
fn make_snippet(text: &str, q_lower: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let byte_idx = lower.find(q_lower)?;
    let char_idx = lower[..byte_idx].chars().count();
    let chars: Vec<char> = text.chars().collect();
    let start = char_idx.saturating_sub(40);
    let end = (char_idx + q_lower.chars().count() + 40).min(chars.len());
    let mut snip: String = chars[start..end]
        .iter()
        .collect::<String>()
        .replace(['\n', '\r'], " ")
        .trim()
        .to_string();
    if start > 0 {
        snip.insert(0, '…');
    }
    if end < chars.len() {
        snip.push('…');
    }
    Some(snip)
}

/// 单条附件的命中判断:文件名 > 摘要 > 提取正文,返回展示用片段。
fn hit_snippet(meta: &AttachmentMeta, q_lower: &str) -> Option<String> {
    if meta.name.to_lowercase().contains(q_lower) {
        return Some(meta.name.clone());
    }
    if let Some(summary) = &meta.summary
        && let Some(s) = make_snippet(summary, q_lower)
    {
        return Some(s);
    }
    if let Some(text) = &meta.extracted_text
        && let Some(s) = make_snippet(text, q_lower)
    {
        return Some(s);
    }
    None
}

/// 扫描带附件的消息并返回命中项(最新消息优先,按附件路径去重)。
pub(crate) fn search_attachments_on(
    conn: &rusqlite::Connection,
    q: &str,
    limit: usize,
) -> Result<Vec<AttachmentHit>, String> {
    let q = q.trim();
    if q.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let q_lower = q.to_lowercase();
    let like = format!("%{}%", escape_like(q));

    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.session_id, COALESCE(s.title,''), m.branch_id, m.attachments
             FROM messages m LEFT JOIN sessions s ON s.id = m.session_id
             WHERE m.attachments IS NOT NULL AND m.attachments <> '[]'
               AND m.attachments LIKE ?1 ESCAPE '\\'
             ORDER BY m.id DESC LIMIT 300",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([&like], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        let (message_id, session_id, session_title, branch_id, raw) =
            row.map_err(|e| e.to_string())?;
        let Ok(metas) = serde_json::from_str::<Vec<AttachmentMeta>>(&raw) else {
            continue;
        };
        for meta in metas {
            if !seen.insert(meta.path.clone()) {
                continue;
            }
            let Some(snippet) = hit_snippet(&meta, &q_lower) else {
                continue;
            };
            out.push(AttachmentHit {
                path: meta.path,
                name: meta.name,
                kind: meta.kind,
                session_id: session_id.clone(),
                session_title: session_title.clone(),
                message_id,
                branch_id: branch_id.clone(),
                snippet,
            });
            if out.len() >= limit {
                return Ok(out);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, title, created, updated)
             VALUES('s1','季度复盘','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
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
            size: 42,
            extracted_text: None,
            doc_id: None,
            chunk_count: None,
            summary: None,
            extract_note: None,
        }
    }

    fn add_msg(conn: &rusqlite::Connection, metas: &[AttachmentMeta]) -> i64 {
        let raw = serde_json::to_string(metas).unwrap();
        crate::db::messages::append_on(
            conn,
            "s1",
            "user",
            "带附件的消息",
            None,
            0,
            Some(&raw),
            None,
            None,
            "main",
        )
        .unwrap()
    }

    #[test]
    fn hits_chinese_summary_with_session_and_snippet() {
        let conn = setup_db();
        let mut m = meta("report.pdf", r"D:\att\report.pdf");
        m.summary = Some("本季度营收同比增长,成本结构优化明显。".into());
        add_msg(&conn, &[m]);

        let hits = search_attachments_on(&conn, "营收", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "report.pdf");
        assert_eq!(hits[0].session_title, "季度复盘");
        assert_eq!(hits[0].branch_id, "main");
        assert!(
            hits[0].snippet.contains("营收"),
            "片段: {}",
            hits[0].snippet
        );
    }

    #[test]
    fn hits_extracted_text() {
        let conn = setup_db();
        let mut m = meta("notes.txt", r"D:\att\notes.txt");
        m.extracted_text = Some("会议讨论了附件中心的导入流程与解析复用策略。".into());
        add_msg(&conn, &[m]);

        let hits = search_attachments_on(&conn, "解析复用", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("解析复用"));
    }

    #[test]
    fn dedupes_same_path() {
        let conn = setup_db();
        add_msg(&conn, &[meta("a.pdf", r"D:\att\a.pdf")]);
        add_msg(&conn, &[meta("a.pdf", r"D:\att\a.pdf")]);

        let hits = search_attachments_on(&conn, "a.pdf", 10).unwrap();
        assert_eq!(hits.len(), 1);
        // 最新消息优先
        assert_eq!(hits[0].message_id, 2);
    }

    #[test]
    fn respects_limit() {
        let conn = setup_db();
        let metas: Vec<_> = (0..5)
            .map(|i| meta(&format!("季度{i}.pdf"), &format!(r"D:\att\q{i}.pdf")))
            .collect();
        add_msg(&conn, &metas);

        let hits = search_attachments_on(&conn, "季度", 3).unwrap();
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn percent_is_literal_not_wildcard() {
        let conn = setup_db();
        add_msg(
            &conn,
            &[
                meta("完成度100%说明.pdf", r"D:\att\p1.pdf"),
                meta("普通说明.pdf", r"D:\att\p2.pdf"),
            ],
        );

        let hits = search_attachments_on(&conn, "100%", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "完成度100%说明.pdf");
    }

    #[test]
    fn empty_query_returns_empty() {
        let conn = setup_db();
        add_msg(&conn, &[meta("a.pdf", r"D:\att\a.pdf")]);
        assert!(search_attachments_on(&conn, "  ", 10).unwrap().is_empty());
    }
}
