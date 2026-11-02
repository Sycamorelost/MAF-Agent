//! 按块读取文档原文(带页码/章节头与单次读取上限)。

use rusqlite::{Connection, params};

use super::store::list_docs;

/// `doc_read` 单次返回的最大字符数(防止模型一次拉爆上下文)。
pub const READ_MAX_CHARS: usize = 12_000;

/// 解析 doc 提示(缺省且会话仅一份文档时自动选中)。
fn resolve_doc(
    conn: &Connection,
    session_id: &str,
    hint: Option<&str>,
) -> Result<Option<(String, String)>, String> {
    let docs = list_docs(conn, session_id)?;
    if let Some(h) = hint.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(docs
            .into_iter()
            .find(|d| d.doc_id == h || d.doc_name == h || d.doc_name.contains(h))
            .map(|d| (d.doc_id, d.doc_name)));
    }
    if docs.len() == 1 {
        return Ok(docs.into_iter().next().map(|d| (d.doc_id, d.doc_name)));
    }
    Ok(None)
}

/// 按块序号读取原文(带页码/章节头)。返回 (给模型的文本, 是否成功)。
pub fn read_range(
    conn: &Connection,
    session_id: &str,
    doc_hint: Option<&str>,
    start: usize,
    count: usize,
) -> Result<String, String> {
    let docs = list_docs(conn, session_id)?;
    if docs.is_empty() {
        return Err("本会话还没有驻留文档:请先上传附件".into());
    }
    let Some((doc_id, doc_name)) = resolve_doc(conn, session_id, doc_hint)? else {
        let list: Vec<String> = docs
            .iter()
            .map(|d| format!("- {} (id:{}, {} 块)", d.doc_name, d.doc_id, d.chunks))
            .collect();
        return Ok(format!(
            "会话内有 {} 份文档,请在 doc 参数中指定名称或 id:\n{}",
            docs.len(),
            list.join("\n")
        ));
    };
    let mut stmt = conn
        .prepare(
            "SELECT ord, page, section, content FROM doc_chunks
             WHERE session_id=?1 AND doc_id=?2 AND ord>=?3 AND ord<?4 ORDER BY ord",
        )
        .map_err(|e| e.to_string())?;
    let end = start.saturating_add(count.max(1)) as i64;
    let rows = stmt
        .query_map(params![session_id, doc_id, start as i64, end], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let total = docs
        .iter()
        .find(|d| d.doc_id == doc_id)
        .map(|d| d.chunks)
        .unwrap_or(0);
    let mut out = String::new();
    let mut used = 0usize;
    for row in rows {
        let (ord, page, section, content) = row.map_err(|e| e.to_string())?;
        let mut head = format!("【{} · 第 {}/{} 块", doc_name, ord + 1, total);
        if let Some(p) = page {
            head.push_str(&format!(" · 第 {p} 页"));
        }
        if !section.is_empty() {
            head.push_str(&format!(" · {section}"));
        }
        head.push_str("】\n");
        let piece = format!("{head}{content}\n\n");
        if used + piece.chars().count() > READ_MAX_CHARS {
            out.push_str(&format!(
                "…(已达单次读取上限 {READ_MAX_CHARS} 字符,剩余块请用 doc_read 续读)\n"
            ));
            break;
        }
        used += piece.chars().count();
        out.push_str(&piece);
    }
    if out.is_empty() {
        return Ok(format!(
            "`{doc_name}` 没有第 {} 块(共 {total} 块,块序号从 0 开始)",
            start
        ));
    }
    Ok(out)
}
