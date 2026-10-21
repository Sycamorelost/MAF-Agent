//! V1–V15 迁移 SQL 与自愈式加列助手。

use rusqlite::Connection;

/// V1 初始建表。
pub(super) const MIGRATION_V1: &str = r#"
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
pub(super) const MIGRATION_V2: &str = r#"
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
pub(super) const MIGRATION_V3: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(content, tokenize='trigram');
"#;

/// V8(迭代 D8):知识库 / 笔记库。
/// notes 为元数据表(rid 为 FTS 关联的整数 rowid);正文本身落盘 data/notes/{id}.md,
/// 同时镜像一份到 notes.body 作为 FTS 索引源(文件仍是唯一事实源,读取以文件为准)。
pub(super) const MIGRATION_NOTES: &str = r#"
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
pub(super) const MIGRATION_BRANCHES: &str = r#"
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
pub(super) const MIGRATION_USAGE: &str = r#"
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

/// V15(学习规划板块 P0):计划 / 知识点 / 行为日志三表。
/// 计划完整载荷(阶段/排期/资料)存 plans.stages_json,同时镜像 data/study/{id}/plan.json;
/// knowledge_nodes 供 P1 掌握度使用;study_logs 记录 学习/测验/复习/深挖研究 行为。
pub(super) const MIGRATION_STUDY: &str = r#"
CREATE TABLE IF NOT EXISTS plans (
  id TEXT PRIMARY KEY,             -- 8 位 hex
  topic TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'generating', -- generating/active/done/archived/error/cancelled
  level TEXT NOT NULL DEFAULT '',
  goal TEXT NOT NULL DEFAULT '',
  daily_minutes INTEGER NOT NULL DEFAULT 30,
  deadline TEXT,
  lang TEXT NOT NULL DEFAULT 'zh',
  model_id TEXT,
  error TEXT,
  repaired INTEGER NOT NULL DEFAULT 0,
  stages_json TEXT NOT NULL DEFAULT '[]',
  created TEXT NOT NULL,
  updated TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS knowledge_nodes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  plan_id TEXT NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
  stage_index INTEGER NOT NULL,
  name TEXT NOT NULL,
  difficulty INTEGER NOT NULL DEFAULT 3,
  why TEXT NOT NULL DEFAULT '',
  must_learn INTEGER NOT NULL DEFAULT 0,
  prereq_json TEXT NOT NULL DEFAULT '[]',
  mastery REAL NOT NULL DEFAULT 0,
  done_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_nodes_plan ON knowledge_nodes(plan_id, stage_index);

CREATE TABLE IF NOT EXISTS study_logs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  plan_id TEXT NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
  node_id INTEGER,
  kind TEXT NOT NULL,              -- plan/learn/quiz/review/research
  minutes INTEGER NOT NULL DEFAULT 0,
  at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_study_logs_plan ON study_logs(plan_id, at);
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
pub(super) fn ensure_column(
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
