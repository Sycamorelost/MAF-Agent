//! 文档生命周期统一入口(val.md §P2「文档服务统一」)。
//!
//! 「提取 → 分块 → 摘要 → 检索 → 删除 → 迁移」这六件事此前散落在
//! `commands/attachment.rs`(提取/摘要/入库)、`doc/mod.rs`(分块/检索/读取)、
//! `paths.rs`(数据目录迁移重写)三处。本模块把**编排**收拢到一处:
//! 调用方只调 `ingest` / `purge` / `rewrite_attachment_paths`,
//! 不必关心摘要怎么生成、块怎么切、JSON 怎么改。
//!
//! 纯算法(分块、检索、n-gram)仍留在 `doc/mod.rs`,本模块只做编排与存储边界,
//! 避免把 800 行的核心算法文件继续撑大。

use rusqlite::Connection;
use serde_json::Value;

use super::{index_document, summary};

/// 消息 JSON 中随附件保存的正文上限:全文仍在 `doc_chunks`,这里只留摘录。
/// 与 `EXTRACT_BUDGET_CHARS`(索引全量)分工不同:那个管入库,这个管消息表体积。
pub const MESSAGE_TEXT_BUDGET_CHARS: usize = 20_000;

/// `ingest` 的产物:一次调用拿到消息层需要的全部派生数据。
#[derive(Debug, Clone, Default)]
pub struct Ingest {
    /// 文档指纹(内容哈希);入库失败为 None
    pub doc_id: Option<String>,
    /// 驻留块数;入库失败为 None
    pub chunks: Option<usize>,
    /// 摘要卡(随每轮注入)
    pub summary: Option<String>,
    /// 消息层正文摘录(已截断到 `MESSAGE_TEXT_BUDGET_CHARS`)
    pub excerpt: Option<String>,
}

/// 截断到指定字符数(按字符而非字节,避免切断多字节 UTF-8)。
pub fn clamp_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect()
}

/// 文档入库:生成摘要 + 分块建索引 + 产出消息层摘录。
///
/// 入库失败(如连接锁竞争)不返回 Err,只把 `doc_id` / `chunks` 置 None,
/// 让调用方回退为「全文注入」;摘要与摘录仍可用,附件保存不会因此失败。
pub fn ingest(
    conn: &Connection,
    session_id: &str,
    doc_name: &str,
    kind: &str,
    text: &str,
) -> Ingest {
    let summary = Some(summary(text));
    let excerpt = Some(clamp_chars(text, MESSAGE_TEXT_BUDGET_CHARS));
    let (doc_id, chunks) = match index_document(conn, session_id, doc_name, kind, text) {
        Ok((id, n)) => (Some(id), Some(n)),
        Err(e) => {
            log::warn!("文档入库失败(回退全文注入): {e}");
            (None, None)
        }
    };
    Ingest {
        doc_id,
        chunks,
        summary,
        excerpt,
    }
}

/// 删除一份文档的全部检索块。返回删除条数。
///
/// 会话级删除由 `doc_chunks` 的外键 `ON DELETE CASCADE` 负责,这里只处理
/// 「同一会话内移除某个附件」的场景(重传覆盖也走同一条通道)。
pub fn purge(conn: &Connection, session_id: &str, doc_id: &str) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM doc_chunks WHERE session_id=?1 AND doc_id=?2",
        rusqlite::params![session_id, doc_id],
    )
    .map_err(|e| format!("清理文档块失败: {e}"))
}

/// 数据目录迁移:把 `messages.attachments` JSON 里所有以 `old_root` 开头的
/// `path` 改写为 `new_root` 开头。返回被改写的消息条数。
///
/// 用 `serde_json::Value` 而非 `AttachmentMeta` 反序列化,保持 `doc` 模块
/// 对 `commands` 层零依赖;坏 JSON 静默跳过,不阻断迁移(R8)。
pub fn rewrite_attachment_paths(
    conn: &Connection,
    old_root: &str,
    new_root: &str,
) -> Result<usize, String> {
    if old_root.is_empty() || old_root == new_root {
        return Ok(0);
    }
    let mut stmt = conn
        .prepare(
            "SELECT id, attachments FROM messages
             WHERE attachments IS NOT NULL AND attachments <> '[]'",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);

    let old_lower = old_root.to_lowercase();
    let mut updates: Vec<(i64, String)> = Vec::new();
    for (id, raw) in rows {
        let Ok(mut list) = serde_json::from_str::<Vec<Value>>(&raw) else {
            continue;
        };
        let mut touched = false;
        for m in list.iter_mut() {
            let Some(s) = m.get("path").and_then(Value::as_str) else {
                continue;
            };
            // Windows 路径大小写不敏感:比较时转小写,替换时保留其余原文
            if !s.to_lowercase().starts_with(&old_lower) {
                continue;
            }
            let rest = &s[old_root.len().min(s.len())..];
            m["path"] = Value::String(format!("{new_root}{rest}"));
            touched = true;
        }
        if touched {
            updates.push((id, serde_json::to_string(&list).map_err(|e| e.to_string())?));
        }
    }

    let n = updates.len();
    for (id, json) in updates {
        conn.execute(
            "UPDATE messages SET attachments=?1 WHERE id=?2",
            rusqlite::params![json, id],
        )
        .map_err(|e| format!("改写附件路径失败: {e}"))?;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{list_docs, search};

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO sessions(id,title,created,updated) VALUES('s1','t','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn
    }

    fn add_msg(conn: &Connection, session: &str, attachments: &str) -> i64 {
        crate::db::messages::append_on(
            conn,
            session,
            "user",
            "x",
            None,
            0,
            Some(attachments),
            None,
            None,
            "main",
        )
        .unwrap()
    }

    #[test]
    fn ingest_returns_summary_excerpt_and_indexes() {
        let conn = setup();
        let text = "## 引言\n多智能体协作提升完成率。".to_string();
        let r = ingest(&conn, "s1", "a.pdf", "pdf", &text);
        assert!(r.doc_id.is_some(), "应成功入库");
        assert_eq!(r.chunks, Some(1));
        assert!(r.summary.as_deref().unwrap().contains("多智能体"));
        assert!(r.excerpt.as_deref().unwrap().contains("多智能体"));
        // 检索能命中
        let hits = search(&conn, "s1", "完成率", None, 5).unwrap();
        assert_eq!(hits.hits.len(), 1);
    }

    #[test]
    fn ingest_clamps_excerpt_to_budget() {
        let conn = setup();
        let text = "字".repeat(MESSAGE_TEXT_BUDGET_CHARS + 500);
        let r = ingest(&conn, "s1", "big.txt", "text", &text);
        let excerpt = r.excerpt.unwrap();
        assert_eq!(excerpt.chars().count(), MESSAGE_TEXT_BUDGET_CHARS);
        // 全文仍完整入库(索引全量,不受消息预算影响)
        assert_eq!(
            list_docs(&conn, "s1").unwrap()[0].chars as usize,
            MESSAGE_TEXT_BUDGET_CHARS + 500
        );
    }

    #[test]
    fn purge_removes_only_target_doc() {
        let conn = setup();
        let a = ingest(&conn, "s1", "a.pdf", "pdf", "甲文档内容甲")
            .doc_id
            .unwrap();
        let _b = ingest(&conn, "s1", "b.pdf", "pdf", "乙文档内容乙")
            .doc_id
            .unwrap();
        assert_eq!(purge(&conn, "s1", &a).unwrap(), 1);
        let docs = list_docs(&conn, "s1").unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].doc_name, "b.pdf");
        // 重复清理幂等
        assert_eq!(purge(&conn, "s1", &a).unwrap(), 0);
    }

    #[test]
    fn rewrite_paths_handles_case_and_keeps_other_fields() {
        let conn = setup();
        let raw = r#"[{"name":"a.png","path":"D:\\Old\\att\\s1\\a.png","kind":"image","size":1}]"#;
        add_msg(&conn, "s1", raw);
        let untouched = add_msg(
            &conn,
            "s1",
            r#"[{"name":"b.png","path":"D:\\Other\\b.png","kind":"image","size":1}]"#,
        );

        let n = rewrite_attachment_paths(&conn, r"D:\Old", r"E:\New").unwrap();
        assert_eq!(n, 1, "只应改写前缀命中的那一条");

        let updated: String = conn
            .query_row(
                "SELECT attachments FROM messages WHERE session_id='s1' AND id=(SELECT MIN(id) FROM messages)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let v: Vec<Value> = serde_json::from_str(&updated).unwrap();
        assert_eq!(v[0]["path"], r"E:\New\att\s1\a.png");
        assert_eq!(v[0]["name"], "a.png", "其他字段必须原样保留");

        let other: String = conn
            .query_row(
                "SELECT attachments FROM messages WHERE id=?1",
                [untouched],
                |r| r.get(0),
            )
            .unwrap();
        assert!(other.contains("D:\\\\Other"), "未命中的路径不得改动");
    }

    #[test]
    fn rewrite_paths_noop_when_same_root() {
        let conn = setup();
        add_msg(&conn, "s1", r#"[{"path":"D:\\Old\\a.png"}]"#);
        assert_eq!(
            rewrite_attachment_paths(&conn, r"D:\Old", r"D:\Old").unwrap(),
            0
        );
        assert_eq!(rewrite_attachment_paths(&conn, "", r"E:\New").unwrap(), 0);
    }

    #[test]
    fn rewrite_paths_skips_broken_json() {
        let conn = setup();
        add_msg(&conn, "s1", "not-json");
        assert_eq!(
            rewrite_attachment_paths(&conn, r"D:\Old", r"E:\New").unwrap(),
            0
        );
    }
}
