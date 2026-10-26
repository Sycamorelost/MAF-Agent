use super::migrate_v1::MIGRATION_V1;
use super::*;

#[test]
fn migrate_creates_all_tables() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();

    for t in [
        "sessions",
        "messages",
        "memory_versions",
        "research_sessions",
        "messages_fts",
        "notes",
        "notes_fts",
        "branches",
        "usage_record",
        "plans",
        "knowledge_nodes",
        "study_logs",
        "study_units",
        "quizzes",
        "quiz_attempts",
        "reviews",
        "todos",
    ] {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [t],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "table {t} should exist");
    }

    let v: i32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(v, SCHEMA_VERSION);

    // 幂等:二次执行不报错
    migrate(&conn).unwrap();
}

/// PERF-2:版本已最新但核心表被删时,快路径必须回落全量迁移并自愈。
#[test]
fn fast_path_self_heals_missing_table() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();

    conn.execute_batch("DROP TABLE todos;").unwrap();
    // 此时 user_version 仍是最新,但表缺失 → is_schema_ready 应判 false
    assert!(!is_schema_ready(&conn).unwrap());

    migrate(&conn).unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='todos'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "缺失的 core 表应被自愈重建");
}

/// PERF-2:版本落后时不得走快路径,必须完整迁移并推进版本号。
#[test]
fn fast_path_rejects_stale_version() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();

    conn.pragma_update(None, "user_version", 0).unwrap();
    assert!(!is_schema_ready(&conn).unwrap());

    migrate(&conn).unwrap();
    let v: i32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(v, SCHEMA_VERSION);
}

/// PERF-3:会话列表 / 附件列表的支撑索引应随迁移创建。
#[test]
fn migrate_creates_perf_indexes() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();

    for idx in ["idx_sessions_updated", "idx_messages_attachments"] {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                [idx],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "index {idx} should exist");
    }
}

#[test]
fn migrate_self_heals_version_drift() {
    // 模拟漂移:user_version 已是 2,但 research_sessions 表缺失
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(MIGRATION_V1).unwrap();
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)
        .unwrap();

    migrate(&conn).unwrap();

    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='research_sessions'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "drifted db should self-heal research_sessions");
}

/// FTS5 trigram:≥3 字符子串可命中(含中文);<3 字符由命令层回退 LIKE。
#[test]
fn fts5_matches_chinese_content() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO sessions(id, title, created, updated) VALUES('abcd1234','测试','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages(session_id, role, content, created) VALUES('abcd1234','user','记忆宫殿法是一种高效的记忆术','2026-01-01T00:00:01Z')",
        [],
    )
    .unwrap();
    conn.execute_batch(
        "INSERT INTO messages_fts(rowid, content) SELECT id, content FROM messages WHERE id NOT IN (SELECT rowid FROM messages_fts);",
    )
    .unwrap();

    for kw in ["记忆宫", "宫殿法", "的记忆术"] {
        let phrase = format!("\"{kw}\"");
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH ?1",
                [&phrase],
                |r| r.get(0),
            )
            .unwrap();
        assert!(n >= 1, "FTS5 trigram should match substring: {kw}");
    }
}

/// 旧表(unicode61)自动重建为 trigram。
#[test]
fn fts5_rebuilds_old_tokenizer() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(MIGRATION_V1).unwrap();
    conn.execute_batch("CREATE VIRTUAL TABLE messages_fts USING fts5(content);")
        .unwrap();

    migrate(&conn).unwrap();

    let sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='messages_fts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        sql.contains("trigram"),
        "old fts table should be rebuilt with trigram: {sql}"
    );
}

/// V4:旧库(仅 V1 结构)迁移后自动补齐 system_prompt / max_context_messages / attachments 三列。
#[test]
fn migrate_v4_adds_columns_to_old_db() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(MIGRATION_V1).unwrap();

    migrate(&conn).unwrap();

    let has_col = |table: &str, col: &str| {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        cols.iter().any(|c| c == col)
    };
    assert!(has_col("sessions", "system_prompt"));
    assert!(has_col("sessions", "max_context_messages"));
    assert!(has_col("messages", "attachments"));
    // V5 列同步补齐
    assert!(has_col("sessions", "web_search_enabled"));
    assert!(has_col("messages", "sources"));
    // V6 列同步补齐
    assert!(has_col("sessions", "memory_injection_strategy"));
    // V7 列同步补齐
    assert!(has_col("sessions", "agent_id"));
    // V9 列同步补齐
    assert!(has_col("messages", "parent_id"));
    assert!(has_col("messages", "branch_id"));
    assert!(has_col("sessions", "active_branch"));
    // V14 列同步补齐
    assert!(has_col("messages", "reasoning"));
    assert!(has_col("usage_record", "reasoning_tokens"));

    // 幂等:二次执行不报错(列已存在时跳过 ALTER)
    migrate(&conn).unwrap();

    // 新列可读写
    conn.execute(
        "INSERT INTO sessions(id, title, created, updated, system_prompt, max_context_messages)
         VALUES('s1','t','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','你是助手',20)",
        [],
    )
    .unwrap();
    let (sp, mc): (Option<String>, i64) = conn
        .query_row(
            "SELECT system_prompt, max_context_messages FROM sessions WHERE id='s1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(sp.as_deref(), Some("你是助手"));
    assert_eq!(mc, 20);
}

/// V9:旧库(V1 结构 + 历史消息)迁移后,历史消息落在 main 分支并形成线性链。
#[test]
fn migrate_v9_backfills_linear_chain_in_main() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(MIGRATION_V1).unwrap();
    conn.execute(
        "INSERT INTO sessions(id, title, created, updated) VALUES('s1','t','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    for (role, content) in [("user", "问一"), ("assistant", "答一"), ("user", "问二")] {
        conn.execute(
            "INSERT INTO messages(session_id, role, content, created) VALUES('s1', ?1, ?2, '2026-01-01T00:00:01Z')",
            rusqlite::params![role, content],
        )
        .unwrap();
    }

    migrate(&conn).unwrap();

    // 全部在 main 分支,sessions.active_branch 默认 main
    let branch: String = conn
        .query_row(
            "SELECT active_branch FROM sessions WHERE id='s1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(branch, "main");
    let n_main: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE session_id='s1' AND branch_id='main'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n_main, 3);

    // 线性链:首条 parent 为 NULL,其余指向前一条
    let mut stmt = conn
        .prepare("SELECT id, parent_id FROM messages WHERE session_id='s1' ORDER BY id ASC")
        .unwrap();
    let chain: Vec<(i64, Option<i64>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0].1, None);
    assert_eq!(chain[1].1, Some(chain[0].0));
    assert_eq!(chain[2].1, Some(chain[1].0));

    // 幂等:二次迁移不破坏链
    migrate(&conn).unwrap();
    let chain2: Vec<(i64, Option<i64>)> = conn
        .prepare("SELECT id, parent_id FROM messages WHERE session_id='s1' ORDER BY id ASC")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(chain, chain2);
}
