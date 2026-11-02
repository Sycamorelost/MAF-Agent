use super::*;

#[test]
fn sanitize_name_strips_unsafe_chars() {
    assert_eq!(sanitize_name("报告 2026/终版.docx"), "报告_2026_终版.docx");
    assert_eq!(sanitize_name("a<b>:c.pdf"), "a_b__c.pdf");
    // ".." 序列会被替换(防御纵深)
    assert_eq!(sanitize_name("..."), "__.");
    assert_eq!(sanitize_name("..\\evil"), "___evil");
    // 超长截断
    let long = format!("{}.txt", "x".repeat(100));
    assert!(sanitize_name(&long).len() <= 60);
}

#[test]
fn sanitize_name_keeps_cjk() {
    assert_eq!(sanitize_name("会议纪要.docx"), "会议纪要.docx");
}

// ---------------- F7 附件中心测试 ----------------

/// 内存库 + 两个会话(main 分支元数据)。
fn setup_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::schema::migrate(&conn).unwrap();
    for (id, title) in [("s1", "项目讨论"), ("s2", "翻译记录")] {
        conn.execute(
            "INSERT INTO sessions(id, title, created, updated) VALUES(?1,?2,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            rusqlite::params![id, title],
        )
        .unwrap();
        crate::db::branches::ensure_main_on(&conn, id).unwrap();
    }
    conn
}

fn meta_json(metas: &[AttachmentMeta]) -> String {
    serde_json::to_string(metas).unwrap()
}

fn fake_meta(name: &str, path: &str, kind: &str) -> AttachmentMeta {
    AttachmentMeta {
        name: name.into(),
        path: path.into(),
        kind: kind.into(),
        size: 42,
        extracted_text: None,
        doc_id: None,
        chunk_count: None,
        summary: None,
        extract_note: None,
    }
}

fn add_msg(conn: &rusqlite::Connection, session: &str, attachments: Option<&str>) -> i64 {
    crate::db::messages::append_on(
        conn,
        session,
        "user",
        "带附件的消息",
        None,
        0,
        attachments,
        None,
        None,
        "main",
    )
    .unwrap()
}

#[test]
fn list_all_expands_json_and_joins_title() {
    let conn = setup_db();
    let m1 = add_msg(
        &conn,
        "s1",
        Some(&meta_json(&[
            fake_meta("a.png", r"D:\att\s1\a.png", "image"),
            fake_meta("b.txt", r"D:\att\s1\b.txt", "text"),
        ])),
    );
    let _m2 = add_msg(&conn, "s1", None);
    let m3 = add_msg(
        &conn,
        "s2",
        Some(&meta_json(&[fake_meta("c.pdf", r"D:\att\s2\c.pdf", "pdf")])),
    );

    let all = list_all_on(&conn).unwrap();
    assert_eq!(all.len(), 3, "两条消息共 3 个附件, got: {}", all.len());
    // 按消息 id 倒序:最新的 m3 在最前
    assert_eq!(all[0].message_id, m3);
    assert_eq!(all[0].session_title, "翻译记录");
    assert_eq!(all[0].meta.name, "c.pdf");
    assert_eq!(all[0].branch_id, "main");
    // m1 的两个附件保持 JSON 内顺序
    assert_eq!(all[1].message_id, m1);
    assert_eq!(all[1].session_title, "项目讨论");
    assert_eq!(all[1].meta.name, "a.png");
    assert_eq!(all[2].meta.name, "b.txt");
}

#[test]
fn remove_from_message_updates_json() {
    let conn = setup_db();
    let m1 = add_msg(
        &conn,
        "s1",
        Some(&meta_json(&[
            fake_meta("a.png", r"D:\att\s1\a.png", "image"),
            fake_meta("b.txt", r"D:\att\s1\b.txt", "text"),
        ])),
    );

    // 移除一个 → 剩一个
    remove_from_message_on(&conn, m1, r"D:\att\s1\a.png").unwrap();
    let raw: String = conn
        .query_row("SELECT attachments FROM messages WHERE id=?1", [m1], |r| {
            r.get(0)
        })
        .unwrap();
    let left: Vec<AttachmentMeta> = serde_json::from_str(&raw).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].name, "b.txt");

    // 再移除 → attachments 变 NULL
    remove_from_message_on(&conn, m1, r"D:\att\s1\b.txt").unwrap();
    let raw: Option<String> = conn
        .query_row("SELECT attachments FROM messages WHERE id=?1", [m1], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(raw.is_none(), "清空后应为 NULL");

    // 路径不存在 / 消息不存在均报错
    assert!(remove_from_message_on(&conn, m1, r"D:\att\s1\ghost.png").is_err());
    assert!(remove_from_message_on(&conn, 99999, r"D:\att\s1\a.png").is_err());
}

#[test]
fn count_references_exact_match() {
    let conn = setup_db();
    let shared = r"D:\att\shared\same.png";
    add_msg(
        &conn,
        "s1",
        Some(&meta_json(&[fake_meta("same.png", shared, "image")])),
    );
    add_msg(
        &conn,
        "s2",
        Some(&meta_json(&[
            fake_meta("same.png", shared, "image"),
            // 相似前缀路径不得误匹配(LIKE 会踩的坑)
            fake_meta("same2.png", r"D:\att\shared\same.png.bak", "image"),
        ])),
    );

    assert_eq!(count_references_on(&conn, shared).unwrap(), 2);
    assert_eq!(
        count_references_on(&conn, r"D:\att\shared\none.png").unwrap(),
        0
    );
}

#[test]
fn remove_from_message_returns_removed_meta() {
    let conn = setup_db();
    let mut m = fake_meta("a.png", r"D:\att\s1\a.png", "image");
    m.doc_id = Some("abc123".into());
    let m1 = add_msg(&conn, "s1", Some(&meta_json(&[m])));

    let removed = remove_from_message_on(&conn, m1, r"D:\att\s1\a.png").unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].doc_id.as_deref(), Some("abc123"));
}

/// R13:同一内容(doc_id 相同)在多条消息中引用时,删除一条不应误清块。
#[test]
fn count_doc_id_refs_detects_shared_content() {
    let conn = setup_db();
    let mut a = fake_meta("a.pdf", r"D:\att\s1\a.pdf", "pdf");
    a.doc_id = Some("dup".into());
    let mut b = fake_meta("b.pdf", r"D:\att\s1\b.pdf", "pdf");
    b.doc_id = Some("dup".into());
    add_msg(&conn, "s1", Some(&meta_json(&[a])));
    add_msg(&conn, "s1", Some(&meta_json(&[b])));
    add_msg(
        &conn,
        "s2",
        Some(&meta_json(&[fake_meta("c.pdf", r"D:\att\s2\c.pdf", "pdf")])),
    );

    assert_eq!(count_doc_id_refs_on(&conn, "s1", "dup").unwrap(), 2);
    assert_eq!(count_doc_id_refs_on(&conn, "s2", "dup").unwrap(), 0);
}

#[test]
fn display_name_strips_ts_prefix() {
    use std::path::Path;
    assert_eq!(
        display_name_from_path(Path::new(r"D:\att\s1\20261015-123000123_报告.docx")),
        "报告.docx"
    );
    // 名字里含下划线:只剥第一个 ts 段
    assert_eq!(
        display_name_from_path(Path::new("20261015-123000123_my_file.txt")),
        "my_file.txt"
    );
    // 无 ts 前缀(手工放入的文件):保留整体
    assert_eq!(display_name_from_path(Path::new("plain.txt")), "plain.txt");
    // 过短的类前缀不剥
    assert_eq!(
        display_name_from_path(Path::new("2026-10_a.txt")),
        "2026-10_a.txt"
    );
}
