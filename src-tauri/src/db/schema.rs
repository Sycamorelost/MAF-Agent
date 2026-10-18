//! SQLite schema 定义与迁移。
//!
//! 对应 plan/01-phase1-mvp.md §3.1。
//! 通过 PRAGMA user_version 做版本迁移,当前版本 = SCHEMA_VERSION(14)。

use rusqlite::Connection;

/// 当前 schema 版本。每次结构变更 +1 并追加迁移 SQL。
pub const SCHEMA_VERSION: i32 = 14;

/// V1 初始建表。
const MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,             -- 8 位 hex,如 8f3a2c1e
  title TEXT NOT NULL DEFAULT '',
  created TEXT NOT NULL,           -- ISO8601 / RFC3339
  updated TEXT NOT NULL,
  model_id TEXT,                   -- 当前使用的模型
  memory_version INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  role TEXT NOT NULL,              -- user / assistant / system
  content TEXT NOT NULL,
  model_id TEXT,                   -- 本条消息用的模型(切换后保留历史)
  created TEXT NOT NULL,
  tokens INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, id);

CREATE TABLE IF NOT EXISTS memory_versions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  snapshot TEXT NOT NULL,          -- 完整 memory.md 内容快照
  created TEXT NOT NULL,
  note TEXT NOT NULL DEFAULT ''    -- 触发原因:切换模型 / 手动编辑 / 自动提炼 vN
);
CREATE INDEX IF NOT EXISTS idx_memory_versions_session ON memory_versions(session_id, id);
"#;

/// V2:深度研究会话表(Phase 2)。
const MIGRATION_V2: &str = r#"
CREATE TABLE IF NOT EXISTS research_sessions (
  id TEXT PRIMARY KEY,             -- 8 位 hex
  topic TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'running',  -- running / done / error / cancelled
  rounds INTEGER NOT NULL DEFAULT 0,       -- 实际执行的轮数
  max_rounds INTEGER NOT NULL DEFAULT 8,
  model_id TEXT,
  error TEXT,
  created TEXT NOT NULL,
  updated TEXT NOT NULL
);
"#;

/// V3:消息全文索引(FTS5 trigram 分词;rowid 与 messages.id 对应)。
/// trigram 支持任意 ≥3 字符的子串匹配(含中文);更短的查询由搜索命令回退 LIKE。
/// 同步点:db::messages::append / clear、db::sessions::delete;此处回填存量。
const MIGRATION_V3: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(content, tokenize='trigram');
"#;

/// V8(迭代 D8):知识库 / 笔记库。
/// notes 为元数据表(rid 为 FTS 关联的整数 rowid);正文本身落盘 data/notes/{id}.md,
/// 同时镜像一份到 notes.body 作为 FTS 索引源(文件仍是唯一事实源,读取以文件为准)。
const MIGRATION_NOTES: &str = r#"
CREATE TABLE IF NOT EXISTS notes (
  rid INTEGER PRIMARY KEY AUTOINCREMENT,
  id TEXT NOT NULL UNIQUE,         -- 8 位 hex,与文件名一致
  title TEXT NOT NULL DEFAULT '',
  tags TEXT NOT NULL DEFAULT '',   -- 逗号分隔
  path TEXT NOT NULL,              -- 绝对路径 data/notes/{id}.md
  created TEXT NOT NULL,
  updated TEXT NOT NULL,
  body TEXT NOT NULL DEFAULT ''    -- 正文镜像(FTS 索引源)
);
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(content, tokenize='trigram');
"#;

/// V9(迭代 B6 / F4):会话分支树的元数据表。
/// 每个会话至少有一行 main;fork 时插入新行,fork_parent_id 记录分叉点
/// (新分支首条消息的 parent_id,即被替换消息的前一条;为 NULL 表示从会话开头分叉)。
const MIGRATION_BRANCHES: &str = r#"
CREATE TABLE IF NOT EXISTS branches (
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  branch_id TEXT NOT NULL,          -- 'main' 或 8 位 hex
  name TEXT NOT NULL DEFAULT '',    -- 用户自定义分支名(空 = 显示默认名)
  fork_parent_id INTEGER,           -- 分叉点(main 恒为 NULL)
  created TEXT NOT NULL,
  PRIMARY KEY (session_id, branch_id)
);
CREATE INDEX IF NOT EXISTS idx_messages_branch ON messages(session_id, branch_id, id);
"#;

/// V10(API 流转):用量记录表(append-only)。
/// 每次实际发出的请求落一行(成功/失败/限流都记),slot_id 为 Key 归因主线。
/// 仅存 Key 掩码,绝不明文;聚合查询按 provider/model/slot 分组。
const MIGRATION_USAGE: &str = r#"
CREATE TABLE IF NOT EXISTS usage_record (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  request_id     TEXT NOT NULL,
  session_id     TEXT,
  provider_id    TEXT NOT NULL,
  provider_name  TEXT,
  model_id       TEXT NOT NULL,
  slot_id        INTEGER NOT NULL DEFAULT 0,
  slot_label     TEXT,
  key_masked     TEXT,
  status         TEXT NOT NULL,
  error_kind     TEXT,
  input_tokens   INTEGER,
  output_tokens  INTEGER,
  total_tokens   INTEGER,
  first_token_ms INTEGER,
  total_ms       INTEGER,
  created_at     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_usage_pm   ON usage_record(provider_id, model_id, created_at);
CREATE INDEX IF NOT EXISTS idx_usage_slot ON usage_record(slot_id, created_at);
"#;

/// V4(迭代 D1)通过 ensure_column 自愈式加列,见 migrate():
///   - sessions.system_prompt        TEXT          — 会话级系统提示词
///   - sessions.max_context_messages INTEGER       — 上下文滑窗(0 = 全部)
///   - messages.attachments          TEXT          — 附件元数据 JSON 数组
/// V5(迭代 D2):
///   - sessions.web_search_enabled   INTEGER       — 对话内联网开关(0/1)
///   - messages.sources              TEXT          — 联网来源 JSON 数组(assistant 消息)
/// V6(迭代 D3):切换模型时的记忆注入策略(full/summary/hybrid/none)
/// V7(迭代 B2):Agent 模板绑定
///   - sessions.agent_id              TEXT          — 绑定的 Agent 模板(NULL = 空白会话)
/// SQLite 的 ALTER TABLE ADD COLUMN 无 IF NOT EXISTS,故先查 PRAGMA table_info。

/// 若列不存在则 ALTER TABLE 加列(幂等,版本漂移自愈)。
fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    decl: &str,
) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<rusqlite::Result<_>>()?;
    if !cols.iter().any(|c| c == column) {
        conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl};"))?;
    }
    Ok(())
}

/// 执行迁移。幂等,可重复调用。
///
/// 迁移 SQL 全部使用 CREATE [VIRTUAL] TABLE IF NOT EXISTS,因此无条件全量执行:
/// 即使 user_version 与表结构发生漂移(版本号已升但某张表缺失)也能自愈。
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    // 外键约束默认关闭,必须显式开
    conn.pragma_update(None, "foreign_keys", "ON")?;

    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(MIGRATION_V1)?;
    tx.execute_batch(MIGRATION_V2)?;
    // V3:FTS 表。若旧定义存在且非 trigram(早期版本),重建之以切换分词器
    let fts_sql: Option<String> = tx
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='messages_fts'",
            [],
            |r| r.get(0),
        )
        .ok();
    if let Some(sql) = fts_sql {
        if !sql.contains("trigram") {
            tx.execute_batch("DROP TABLE messages_fts;")?;
        }
    }
    tx.execute_batch(MIGRATION_V3)?;
    // FTS 回填:把尚未入索引的存量消息补进去(幂等)
    tx.execute_batch(
        "INSERT INTO messages_fts(rowid, content)
         SELECT id, content FROM messages
         WHERE id NOT IN (SELECT rowid FROM messages_fts);",
    )?;
    // V8(迭代 D8):知识库 / 笔记库。若 notes_fts 旧定义非 trigram 则重建。
    let notes_fts_sql: Option<String> = tx
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='notes_fts'",
            [],
            |r| r.get(0),
        )
        .ok();
    if let Some(sql) = notes_fts_sql {
        if !sql.contains("trigram") {
            tx.execute_batch("DROP TABLE notes_fts;")?;
        }
    }
    tx.execute_batch(MIGRATION_NOTES)?;
    // notes_fts 回填:notes.body 尚未入索引的补进去(幂等)
    tx.execute_batch(
        "INSERT INTO notes_fts(rowid, content)
         SELECT rid, title || char(10) || body FROM notes
         WHERE rid NOT IN (SELECT rowid FROM notes_fts);",
    )?;
    // V4:自愈式加列(旧库自动补,新库 PRAGMA 命中跳过)
    ensure_column(&tx, "sessions", "system_prompt", "TEXT")?;
    ensure_column(&tx, "sessions", "max_context_messages", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(&tx, "messages", "attachments", "TEXT")?;
    // V5(迭代 D2):联网开关 + 消息来源
    ensure_column(&tx, "sessions", "web_search_enabled", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(&tx, "messages", "sources", "TEXT")?;
    // V6(迭代 D3):切换模型时的记忆注入策略(full/summary/hybrid/none)
    ensure_column(&tx, "sessions", "memory_injection_strategy", "TEXT NOT NULL DEFAULT 'full'")?;
    // V7(迭代 B2):会话绑定 Agent 模板
    ensure_column(&tx, "sessions", "agent_id", "TEXT")?;
    // V9(迭代 B6 / F4):会话分支树
    ensure_column(&tx, "messages", "parent_id", "INTEGER")?;
    ensure_column(&tx, "messages", "branch_id", "TEXT NOT NULL DEFAULT 'main'")?;
    ensure_column(&tx, "sessions", "active_branch", "TEXT NOT NULL DEFAULT 'main'")?;
    tx.execute_batch(MIGRATION_BRANCHES)?;
    // V12(专项研究):研究模式 / 当前阶段 / 来源方案名
    ensure_column(&tx, "research_sessions", "mode", "TEXT NOT NULL DEFAULT 'legacy'")?;
    ensure_column(&tx, "research_sessions", "current_stage", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(&tx, "research_sessions", "blueprint_name", "TEXT")?;
    // V13(专项研究 M4):阶段进度回写列,列表页零文件 IO(StageDone/StageSkipped 时更新)
    ensure_column(&tx, "research_sessions", "stages_done", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(&tx, "research_sessions", "stages_total", "INTEGER NOT NULL DEFAULT 0")?;
    // V10(API 流转):用量记录表
    tx.execute_batch(MIGRATION_USAGE)?;
    // V11(Phase 2):助手归因 + 成本定价快照(旧库自愈式加列)
    ensure_column(&tx, "usage_record", "agent_id", "TEXT")?;
    ensure_column(&tx, "usage_record", "agent_name", "TEXT")?;
    ensure_column(&tx, "usage_record", "cost_micro", "INTEGER")?;
    ensure_column(&tx, "usage_record", "pricing_snapshot", "TEXT")?;
    ensure_column(&tx, "usage_record", "cost_source", "TEXT")?;
    // V14(v1.9.0 工作块 I):思考能力
    //   - messages.reasoning:思考过程文本(与 content 分离;不进 FTS,避免搜索噪声)
    //   - usage_record.reasoning_tokens:思考 token 单列(append-only,旧记录为 NULL)
    ensure_column(&tx, "messages", "reasoning", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(&tx, "usage_record", "reasoning_tokens", "INTEGER")?;
    // 旧数据回填:main 分支内按 id 顺序形成线性链。
    // 仅回填 main:其他分支首条消息的 parent_id 可能合法为 NULL(从会话开头分叉),
    // 且 fork 消息的 parent 在写入时已显式设置,不在此 touched。
    tx.execute_batch(
        "UPDATE messages SET parent_id = (
           SELECT MAX(m2.id) FROM messages m2
           WHERE m2.session_id = messages.session_id AND m2.id < messages.id
         ) WHERE parent_id IS NULL AND branch_id = 'main';",
    )?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_all_tables() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        for t in ["sessions", "messages", "memory_versions", "research_sessions", "messages_fts", "notes", "notes_fts", "branches", "usage_record"] {
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
        conn.execute_batch(
            "CREATE VIRTUAL TABLE messages_fts USING fts5(content);",
        )
        .unwrap();

        migrate(&conn).unwrap();

        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='messages_fts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(sql.contains("trigram"), "old fts table should be rebuilt with trigram: {sql}");
    }

    /// V4:旧库(仅 V1 结构)迁移后自动补齐 system_prompt / max_context_messages / attachments 三列。
    #[test]
    fn migrate_v4_adds_columns_to_old_db() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MIGRATION_V1).unwrap();

        migrate(&conn).unwrap();

        let has_col = |table: &str, col: &str| {
            let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})")).unwrap();
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
            .query_row("SELECT system_prompt, max_context_messages FROM sessions WHERE id='s1'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
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
            .query_row("SELECT active_branch FROM sessions WHERE id='s1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(branch, "main");
        let n_main: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages WHERE session_id='s1' AND branch_id='main'", [], |r| r.get(0))
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
}
