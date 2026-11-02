//! 文档块索引的存取:建索引、列出文档、生成记忆骨架。

use rusqlite::{Connection, params};
use serde::Serialize;

use super::chunk::{chunk_text, doc_id_of};

/// 文档概要(给模型看,也给前端做文档卡)。
#[derive(Debug, Clone, Serialize)]
pub struct DocInfo {
    pub doc_id: String,
    pub doc_name: String,
    pub kind: String,
    pub chunks: i64,
    pub chars: i64,
    pub page_from: Option<i64>,
    pub page_to: Option<i64>,
}

/// 检索命中。
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub doc_name: String,
    pub ord: i64,
    pub page: Option<i64>,
    pub section: String,
    pub snippet: String,
    pub score: i64,
}

/// 检索结果(附带扫描块数,供模型判断是否还有更多)。
#[derive(Debug, Clone, Serialize)]
pub struct SearchOutcome {
    pub hits: Vec<SearchHit>,
    pub scanned: usize,
}

/// 建/重建一份文档的块索引:先删同 (会话, 文档) 旧块,再整份写入。
/// 返回 (doc_id, 块数);doc_id 由正文内容哈希决定。
pub fn index_document(
    conn: &Connection,
    session_id: &str,
    doc_name: &str,
    kind: &str,
    text: &str,
) -> Result<(String, usize), String> {
    let doc_id = doc_id_of(text);
    let chunks = chunk_text(text);
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    // 同一会话内:同 doc_id(内容相同)或同 doc_name(同名文件重新上传)都视为同一份文档,
    // 重建索引时先清旧块,避免"同名新版本"叠成两份(R14)。
    // FTS 不随外键级联,必须先按 rowid 清旧索引再删块(V27)。
    tx.execute(
        "DELETE FROM doc_chunks_fts WHERE rowid IN
           (SELECT id FROM doc_chunks WHERE session_id=?1 AND (doc_id=?2 OR doc_name=?3))",
        params![session_id, doc_id, doc_name],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM doc_chunks WHERE session_id=?1 AND (doc_id=?2 OR doc_name=?3)",
        params![session_id, doc_id, doc_name],
    )
    .map_err(|e| e.to_string())?;
    let created = chrono::Local::now().to_rfc3339();
    for (i, c) in chunks.iter().enumerate() {
        tx.execute(
            "INSERT INTO doc_chunks
               (session_id, doc_id, doc_name, kind, page, section, ord, content, created)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                session_id,
                doc_id,
                doc_name,
                kind,
                c.page.map(|p| p as i64),
                c.section,
                i as i64,
                c.content,
                created,
            ],
        )
        .map_err(|e| e.to_string())?;
        // 同步入 FTS 索引(rowid = doc_chunks.id;章节标题一并入索引,V27)
        let rowid = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO doc_chunks_fts(rowid, content) VALUES (?1, ?2)",
            params![rowid, format!("{} {}", c.section, c.content)],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok((doc_id, chunks.len()))
}

/// 会话内全部已驻留文档。
pub fn list_docs(conn: &Connection, session_id: &str) -> Result<Vec<DocInfo>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT doc_id, doc_name, kind, COUNT(*), COALESCE(SUM(length(content)),0),
                    MIN(page), MAX(page)
             FROM doc_chunks WHERE session_id=?1
             GROUP BY doc_id, doc_name, kind ORDER BY MIN(id)",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![session_id], |r| {
            Ok(DocInfo {
                doc_id: r.get(0)?,
                doc_name: r.get(1)?,
                kind: r.get(2)?,
                chunks: r.get(3)?,
                chars: r.get(4)?,
                page_from: r.get(5)?,
                page_to: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// 文档骨架(供记忆提炼使用):每份文档的名称 / 规模 / 章节标题,不含正文。
/// 记忆提炼的输入预算只有 8000 字符,放不下正文;给出骨架让提炼器能维护
/// 「文档摘要」节,而不是让模型凭空气写。
pub fn outline(conn: &Connection, session_id: &str) -> Result<String, String> {
    let docs = list_docs(conn, session_id)?;
    if docs.is_empty() {
        return Ok(String::new());
    }
    let mut out = String::new();
    for d in docs {
        let mut stmt = conn
            .prepare(
                "SELECT section, MIN(ord) AS o FROM doc_chunks
                 WHERE session_id=?1 AND doc_name=?2 AND section <> ''
                 GROUP BY section ORDER BY o LIMIT 12",
            )
            .map_err(|e| e.to_string())?;
        let sections: Vec<String> = stmt
            .query_map(params![session_id, d.doc_name], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        out.push_str(&format!(
            "\n- 《{}》({} · {} 块 · 约 {} 字符)",
            d.doc_name,
            kind_label(&d.kind),
            d.chunks,
            d.chars
        ));
        if !sections.is_empty() {
            out.push_str(&format!(";章节:{}", sections.join(" / ")));
        }
    }
    Ok(out)
}

/// 文档类型的展示名(给记忆骨架用)。
fn kind_label(kind: &str) -> &str {
    match kind {
        "pdf" => "PDF",
        "docx" => "Word",
        "xlsx" => "Excel",
        "pptx" => "PPT",
        "epub" => "EPUB",
        "html" => "HTML",
        _ => "文本",
    }
}
