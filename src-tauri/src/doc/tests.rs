//! 文档驻留层的单元测试(分块 / 索引 / 检索 / 读取)。

use rusqlite::Connection;

use super::*;

fn doc(conn: &Connection, sid: &str) {
    conn.execute(
        "INSERT INTO sessions(id,title,created,updated) VALUES(?1,'t','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
        rusqlite::params![sid],
    )
    .unwrap();
}

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::schema::migrate(&conn).unwrap();
    doc(&conn, "s1");
    doc(&conn, "s2");
    conn
}

#[test]
fn chunk_splits_by_page_and_section() {
    let text = "【第 1 页】\n摘要:这是第一段。\n\n【第 2 页】\n## 方法\n实验采用双盲设计。\n## 结果\n显著性 p<0.05。";
    let chunks = chunk_text(text);
    assert_eq!(chunks.len(), 3, "got: {chunks:?}");
    assert_eq!(chunks[0].page, Some(1));
    assert_eq!(chunks[1].page, Some(2));
    assert_eq!(chunks[1].section, "方法");
    assert_eq!(chunks[2].section, "结果");
    assert!(chunks[1].content.contains("双盲"));
}

#[test]
fn chunk_respects_target_size() {
    // 每行 120 字 × 12 行 → 至少切成 2 块
    let line = "字".repeat(120);
    let text = [line.as_str(); 12].join("\n");
    let chunks = chunk_text(&text);
    assert!(chunks.len() >= 2, "got {}", chunks.len());
    for c in &chunks {
        assert!(c.content.chars().count() <= CHUNK_TARGET + 200);
    }
}

#[test]
fn chunk_empty_text_yields_nothing() {
    assert!(chunk_text("").is_empty());
    assert!(chunk_text("   \n\n  ").is_empty());
}

#[test]
fn index_search_read_roundtrip() {
    let conn = setup();
    let text = "【第 1 页】\n## 引言\n本文研究多智能体协作。\n\n【第 2 页】\n## 方法\n我们使用检索增强生成。".to_string();
    let (_id, n) = index_document(&conn, "s1", "paper.pdf", "pdf", &text).unwrap();
    assert_eq!(n, 2);

    let docs = list_docs(&conn, "s1").unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].doc_name, "paper.pdf");
    assert_eq!(docs[0].page_from, Some(1));
    assert_eq!(docs[0].page_to, Some(2));

    let r = search(&conn, "s1", "检索增强", None, 5).unwrap();
    assert_eq!(r.hits.len(), 1, "got: {:?}", r.hits);
    assert!(r.hits[0].snippet.contains("检索增强"));
    assert_eq!(r.hits[0].page, Some(2));

    // 读第 0 块
    let out = read_range(&conn, "s1", None, 0, 1).unwrap();
    assert!(out.contains("第 1 页"), "got: {out}");
    assert!(out.contains("多智能体协作"), "got: {out}");
}

#[test]
fn reindex_replaces_old_chunks() {
    let conn = setup();
    index_document(&conn, "s1", "p.pdf", "pdf", "第一版内容").unwrap();
    index_document(&conn, "s1", "p.pdf", "pdf", "第二版内容更长了").unwrap();
    let docs = list_docs(&conn, "s1").unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].chunks, 1);
    let out = read_range(&conn, "s1", None, 0, 1).unwrap();
    assert!(out.contains("第二版"));
}

/// R14:同一内容重复上传(内容哈希相同)只保留一份块。
#[test]
fn same_content_reuses_doc_id() {
    let conn = setup();
    let (id1, _) = index_document(&conn, "s1", "a.pdf", "pdf", "同一份内容").unwrap();
    let (id2, _) = index_document(&conn, "s1", "b.pdf", "pdf", "同一份内容").unwrap();
    assert_eq!(id1, id2, "内容相同时 doc_id 必须一致");
    let docs = list_docs(&conn, "s1").unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].chunks, 1);
}

/// P0:中文自然语言提问必须能命中(此前整句被当成一个 token,召回为零)。
#[test]
fn natural_language_chinese_query_hits() {
    let conn = setup();
    let text = "【第 1 页】\n## 结论\n多智能体协作显著提升了任务完成率,实验组比对照组高 23%。";
    index_document(&conn, "s1", "report.pdf", "pdf", text).unwrap();
    let r = search(&conn, "s1", "这篇文档的主要结论是什么", None, 5).unwrap();
    assert_eq!(r.hits.len(), 1, "got: {:?}", r.hits);
    assert!(
        r.hits[0].snippet.contains("结论"),
        "got: {}",
        r.hits[0].snippet
    );
    // 命中词不应是整句
    assert!(
        query_tokens("这篇文档的主要结论是什么")
            .iter()
            .all(|t| t.chars().count() <= 4)
    );
}

/// 停用词不应制造误命中:问句套话单独出现时返回空。
#[test]
fn stopword_only_query_returns_empty() {
    let conn = setup();
    index_document(&conn, "s1", "a.pdf", "pdf", "一些普通内容").unwrap();
    let r = search(&conn, "s1", "这篇文档是什么", None, 5).unwrap();
    assert!(r.hits.is_empty(), "got: {:?}", r.hits);
}

#[test]
fn session_delete_cascades_chunks() {
    let conn = setup();
    index_document(&conn, "s1", "p.pdf", "pdf", "内容").unwrap();
    index_document(&conn, "s2", "q.pdf", "pdf", "内容").unwrap();
    conn.execute("DELETE FROM sessions WHERE id='s1'", [])
        .unwrap();
    let cnt: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM doc_chunks WHERE session_id='s1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cnt, 0, "删会话必须级联删块");
    assert_eq!(list_docs(&conn, "s2").unwrap().len(), 1);
}

#[test]
fn multi_doc_read_requires_hint() {
    let conn = setup();
    index_document(&conn, "s1", "a.pdf", "pdf", "甲文档内容").unwrap();
    index_document(&conn, "s1", "b.pdf", "pdf", "乙文档内容").unwrap();
    let ambiguous = read_range(&conn, "s1", None, 0, 1).unwrap();
    assert!(
        ambiguous.contains("请在 doc 参数中指定"),
        "got: {ambiguous}"
    );
    let hit = read_range(&conn, "s1", Some("b.pdf"), 0, 1).unwrap();
    assert!(hit.contains("乙文档"), "got: {hit}");
    assert!(read_range(&conn, "missing-session", None, 0, 1).is_err());
}
