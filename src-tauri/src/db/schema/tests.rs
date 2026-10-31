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

    // V23:研究功能移除,新库不再创建 research_sessions
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='research_sessions'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0, "research_sessions should not exist after V23");

    // 幂等:二次执行不报错
    migrate(&conn).unwrap();
}

/// V33:冗余列归一 —— plans.preferences 可写,study_units.title(与 node_name 恒等)已删除。
#[test]
fn v33_normalizes_redundant_columns() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();

    let prefs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('plans') WHERE name='preferences'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(prefs, 1, "plans.preferences 应存在");

    let title: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('study_units') WHERE name='title'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(title, 0, "study_units.title 冗余列应已删除");
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
    // 模拟漂移:user_version 已是最新,但 V19 引入的 todos 表缺失
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(MIGRATION_V1).unwrap();
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)
        .unwrap();

    migrate(&conn).unwrap();

    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='todos'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "drifted db should self-heal todos");
}

/// V23:旧库遗留的 research_sessions 在迁移后被删除。
#[test]
fn migrate_v23_drops_research_sessions() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE research_sessions(id TEXT PRIMARY KEY);")
        .unwrap();

    migrate(&conn).unwrap();

    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='research_sessions'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0, "legacy research_sessions should be dropped by V23");
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

/// V26:旧库(仅 V1 结构)迁移后自动补齐 preset_id / preset_version 两列(运行时预设钉住)。
#[test]
fn migrate_v26_adds_preset_columns_to_old_db() {
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
    assert!(has_col("sessions", "preset_id"));
    assert!(has_col("sessions", "preset_version"));

    // 幂等:二次执行不报错
    migrate(&conn).unwrap();

    // 新列可读写
    conn.execute(
        "INSERT INTO sessions(id, title, created, updated, preset_id, preset_version)
         VALUES('s1','t','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','deep',1)",
        [],
    )
    .unwrap();
    let (pid, pv): (Option<String>, Option<i64>) = conn
        .query_row(
            "SELECT preset_id, preset_version FROM sessions WHERE id='s1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(pid.as_deref(), Some("deep"));
    assert_eq!(pv, Some(1));
}

/// V27:doc_chunks_fts 随迁移创建(trigram),存量 doc_chunks 回填入索引,且幂等。
#[test]
fn migrate_v27_creates_doc_chunks_fts() {
    let conn = Connection::open_in_memory().unwrap();
    // 旧库:V1 + V22 风格的 doc_chunks,且已有存量块
    conn.execute_batch(MIGRATION_V1).unwrap();
    conn.execute_batch(
        "CREATE TABLE doc_chunks (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           session_id TEXT NOT NULL,
           doc_id TEXT NOT NULL,
           doc_name TEXT NOT NULL,
           kind TEXT NOT NULL DEFAULT 'txt',
           page INTEGER,
           section TEXT NOT NULL DEFAULT '',
           ord INTEGER NOT NULL,
           content TEXT NOT NULL,
           created TEXT NOT NULL
         );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO doc_chunks(session_id, doc_id, doc_name, ord, content, section, created)
         VALUES('s1','d1','白皮书.pdf',0,'定价策略分为按量计费与包年包月两种','第三章 定价','2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();

    migrate(&conn).unwrap();

    // 表存在且为 trigram 分词
    let sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='doc_chunks_fts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        sql.contains("trigram"),
        "doc_chunks_fts should use trigram: {sql}"
    );

    // 存量块已回填(rowid = doc_chunks.id),章节标题也入索引
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM doc_chunks_fts WHERE doc_chunks_fts MATCH '\"定价策略\"'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "existing chunk should be backfilled into fts");
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM doc_chunks_fts WHERE doc_chunks_fts MATCH '\"第三章\"'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "section title should be indexed");

    // 幂等:二次迁移不重复回填
    migrate(&conn).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM doc_chunks_fts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}
