//! SQLite schema 定义与迁移。
//!
//! 对应 plan/01-phase1-mvp.md §3.1。
//! 通过 PRAGMA user_version 做版本迁移,当前版本 = SCHEMA_VERSION(27)。
//!
//! 拆分(§10.2.1):`migrate_v1` 收敛 V1–V15 的迁移 SQL 与自愈式加列助手,
//! `v16` 收敛 V16–V22 的迁移 SQL;版本执行顺序仍由本文件统一编排。

mod migrate_v1;
mod v16;

#[cfg(test)]
mod tests;

use rusqlite::Connection;

use migrate_v1::{
    MIGRATION_BRANCHES, MIGRATION_NOTES, MIGRATION_STUDY, MIGRATION_USAGE, MIGRATION_V1,
    MIGRATION_V2, MIGRATION_V3, ensure_column,
};
use v16::{
    MIGRATION_STUDY_P1, MIGRATION_V17_INDEX, MIGRATION_V18_INDEX, MIGRATION_V19_TODOS,
    MIGRATION_V20_INDEX, MIGRATION_V21_AIFLAVOR, MIGRATION_V22_DOC_CHUNKS,
};

/// 当前 schema 版本。每次结构变更 +1 并追加迁移 SQL。
pub const SCHEMA_VERSION: i32 = 30;

/// V29(P8.1 项目化升级):folders → projects,加字段对齐 Cursor Projects 设计。
/// - 重命名 folders → projects(SQLite ALTER TABLE RENAME)
/// - 加 model_override TEXT(项目级默认模型,覆盖全局)
/// - 加 shared_context TEXT(项目级共享系统提示/记忆)
/// - sessions.folder_id → sessions.project_id
/// UI 视觉不变,内部概念从「文件夹」升级为「项目」。
///
/// 注意:V29 DDL 不在 SQL 常量里写,而是在 migrate() 里用 Rust 条件化执行
/// (先查 sqlite_master / pragma_table_info 再决定执行哪条 DDL),保证幂等。

/// V28(P8 工作台文件夹化):folders 表 + sessions.folder_id 列。
///
/// V28.1 增补:folders.path 列(真实磁盘目录路径)。
/// 文件夹 = 用户从磁盘选一个本地目录,系统挂载为「项目文件夹」;
/// 每个文件夹下可以有多个任务会话。未选时回退到全局 workspace_root。
const MIGRATION_V28_FOLDERS: &str = "
CREATE TABLE IF NOT EXISTS folders (
  id       TEXT PRIMARY KEY,
  name     TEXT NOT NULL,
  path     TEXT NOT NULL DEFAULT '',
  sort     INTEGER NOT NULL DEFAULT 0,
  created  TEXT NOT NULL,
  updated  TEXT NOT NULL
);
";

/// V23(P1 结构调整):深度研究功能整体移除,删除研究会话表。
/// 依据 D2 不做数据导出;历史产物文件 data/research/** 保留在数据目录,由用户自行清理。
const MIGRATION_V23_DROP_RESEARCH: &str = "DROP TABLE IF EXISTS research_sessions;";

/// V25(P4 结构调整):专业工作台任务表。plan 只存结构化 JSON(步骤/状态),不存大段正文。
/// created/updated 为 RFC3339 文本(与 sessions 表口径一致)。
const MIGRATION_V25_TASKS: &str = "
CREATE TABLE IF NOT EXISTS tasks (
  id         TEXT PRIMARY KEY,
  session_id TEXT NOT NULL,
  title      TEXT NOT NULL,
  goal       TEXT NOT NULL DEFAULT '',
  status     TEXT NOT NULL DEFAULT 'pending',
  priority   TEXT NOT NULL DEFAULT 'normal',
  plan       TEXT,
  created    TEXT NOT NULL,
  updated    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tasks_session ON tasks(session_id, created);
";

/// 快路径判定(PERF-2):`user_version` 已是最新且核心表齐全时,可跳过全量 DDL 与回填。
///
/// 仍保留自愈能力:版本号不匹配或任一核心表缺失都会回落全量迁移。
/// 因此这里的表清单只需覆盖「缺失即说明结构损坏」的核心表。
fn is_schema_ready(conn: &Connection) -> rusqlite::Result<bool> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version != SCHEMA_VERSION {
        return Ok(false);
    }
    const CORE: &[&str] = &[
        "sessions",
        "projects",
        "messages",
        "messages_fts",
        "memory_versions",
        "branches",
        "notes",
        "notes_fts",
        "usage_record",
        "plans",
        "knowledge_nodes",
        "study_logs",
        "study_units",
        "quizzes",
        "quiz_attempts",
        "reviews",
        "todos",
        "doc_chunks",
        "doc_chunks_fts",
        "tasks",
        "tool_call_log",
    ];
    let placeholders = vec!["?"; CORE.len()].join(",");
    let sql = format!("SELECT COUNT(*) FROM sqlite_master WHERE name IN ({placeholders})");
    let found: i64 = conn.query_row(&sql, rusqlite::params_from_iter(CORE), |r| r.get(0))?;
    Ok(found as usize == CORE.len())
}

/// 执行迁移。幂等,可重复调用。
///
/// 迁移 SQL 全部使用 CREATE [VIRTUAL] TABLE IF NOT EXISTS,因此无条件全量执行:
/// 即使 user_version 与表结构发生漂移(版本号已升但某张表缺失)也能自愈。
///
/// PERF-2:健康库(版本一致 + 核心表齐全)命中快路径直接返回,
/// 避免每次启动都跑全量 DDL 与 FTS 回填;需要自愈时自动回落全量迁移。
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    // 外键约束默认关闭,必须显式开
    conn.pragma_update(None, "foreign_keys", "ON")?;

    if is_schema_ready(conn)? {
        return Ok(());
    }

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
    if let Some(sql) = fts_sql
        && !sql.contains("trigram")
    {
        tx.execute_batch("DROP TABLE messages_fts;")?;
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
    if let Some(sql) = notes_fts_sql
        && !sql.contains("trigram")
    {
        tx.execute_batch("DROP TABLE notes_fts;")?;
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
    ensure_column(
        &tx,
        "sessions",
        "max_context_messages",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(&tx, "messages", "attachments", "TEXT")?;
    // V5(迭代 D2):联网开关 + 消息来源
    ensure_column(
        &tx,
        "sessions",
        "web_search_enabled",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(&tx, "messages", "sources", "TEXT")?;
    // V6(迭代 D3):切换模型时的记忆注入策略(full/summary/hybrid/none)
    // 默认 hybrid:摘要+结论+待办按需注入,避免每轮全量注入 memory.md(省 token / 降延迟)
    ensure_column(
        &tx,
        "sessions",
        "memory_injection_strategy",
        "TEXT NOT NULL DEFAULT 'hybrid'",
    )?;
    // V7(迭代 B2):会话绑定 Agent 模板
    ensure_column(&tx, "sessions", "agent_id", "TEXT")?;
    // V9(迭代 B6 / F4):会话分支树
    ensure_column(&tx, "messages", "parent_id", "INTEGER")?;
    ensure_column(&tx, "messages", "branch_id", "TEXT NOT NULL DEFAULT 'main'")?;
    ensure_column(
        &tx,
        "sessions",
        "active_branch",
        "TEXT NOT NULL DEFAULT 'main'",
    )?;
    tx.execute_batch(MIGRATION_BRANCHES)?;
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
    // V15(学习规划板块 P0):plans / knowledge_nodes / study_logs
    tx.execute_batch(MIGRATION_STUDY)?;
    // V16(学习规划板块 P1):knowledge_nodes 增掌握度来源两列(自评 / 测验分,-1 = 未评)
    ensure_column(
        &tx,
        "knowledge_nodes",
        "self_rating",
        "REAL NOT NULL DEFAULT -1",
    )?;
    ensure_column(
        &tx,
        "knowledge_nodes",
        "quiz_score",
        "REAL NOT NULL DEFAULT -1",
    )?;
    tx.execute_batch(MIGRATION_STUDY_P1)?;
    // V17(计量口径):缓存命中 token + 纳元成本 + 纯时间索引。
    //   - cached_tokens:厂商未返回为 NULL(未知 ≠ 0 命中),成本按命中/未命中分段;
    //   - cost_nano:1e-9 元精度,避免微元四舍五入把低价模型小额请求舍成 0;
    //     旧记录该列为 NULL,聚合时以 cost_micro × 1000 兜底,历史数字不跳变;
    //   - idx_usage_time:纯时间范围查询走索引,90 天窗口不再全表扫描。
    ensure_column(&tx, "usage_record", "cached_tokens", "INTEGER")?;
    ensure_column(&tx, "usage_record", "cost_nano", "INTEGER")?;
    tx.execute_batch(MIGRATION_V17_INDEX)?;
    // V18(维度复合索引):agent_id + created_at 覆盖 summary_by(agent) 查询。
    tx.execute_batch(MIGRATION_V18_INDEX)?;
    // V19(harness 任务清单):todos 表。
    tx.execute_batch(MIGRATION_V19_TODOS)?;
    // V20(PERF-3):会话列表 / 附件列表的支撑索引。
    tx.execute_batch(MIGRATION_V20_INDEX)?;
    // V21(R3 B2):去 AI 味报告与改写记录。
    tx.execute_batch(MIGRATION_V21_AIFLAVOR)?;
    // V22(R3 C1):文档驻留层(doc_chunks)。
    tx.execute_batch(MIGRATION_V22_DOC_CHUNKS)?;
    // V23:移除深度研究功能(建表语句已随 V2 退役,此处清理旧库遗留表)。
    tx.execute_batch(MIGRATION_V23_DROP_RESEARCH)?;
    // V24(P3 结构调整):会话级工作台模式覆盖(D16)。inherit = 跟随全局 default_workbench。
    ensure_column(
        &tx,
        "sessions",
        "workbench_mode",
        "TEXT NOT NULL DEFAULT 'inherit'",
    )?;
    // V25(P4 结构调整):专业工作台任务模型 + 权限三档会话级覆盖(D8/D11)。
    //   - tasks:目标级任务(步骤级清单仍在 todos,task_id 可空关联,旧数据按会话维度继续工作);
    //   - sessions.permission_mode:inherit = 跟随全局审批档;ask/workspace/full = 会话覆盖。
    tx.execute_batch(MIGRATION_V25_TASKS)?;
    ensure_column(&tx, "todos", "task_id", "TEXT")?;
    tx.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_todos_task ON todos(task_id, item_id);",
    )?;
    ensure_column(
        &tx,
        "sessions",
        "permission_mode",
        "TEXT NOT NULL DEFAULT 'inherit'",
    )?;
    // V26(P5 结构调整 / D19):会话级运行时预设钉住。
    //   - preset_id:NULL = 继承(Agent 模板默认 → 全局默认);非空 = 会话钉住的预设 id;
    //   - preset_version:钉住时的内置预设清单版本,老会话不随内置预设升级漂移。
    ensure_column(&tx, "sessions", "preset_id", "TEXT")?;
    ensure_column(&tx, "sessions", "preset_version", "INTEGER")?;
    // V27(P7 检索升级):doc_chunks 的 FTS5 trigram 索引(内容 + 章节标题一并入索引)。
    // 旧定义若非 trigram 则重建;rowid 直接复用 doc_chunks.id(INTEGER PRIMARY KEY)。
    let doc_fts_sql: Option<String> = tx
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='doc_chunks_fts'",
            [],
            |r| r.get(0),
        )
        .ok();
    if let Some(sql) = doc_fts_sql
        && !sql.contains("trigram")
    {
        tx.execute_batch("DROP TABLE doc_chunks_fts;")?;
    }
    tx.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS doc_chunks_fts USING fts5(content, tokenize='trigram');",
    )?;
    // 存量回填(幂等):章节标题入索引,使「按章节名检索」也能命中。
    tx.execute_batch(
        "INSERT INTO doc_chunks_fts(rowid, content)
         SELECT id, section || ' ' || content FROM doc_chunks
         WHERE id NOT IN (SELECT rowid FROM doc_chunks_fts);",
    )?;
    // V28(P8 工作台文件夹化):folders 表 + sessions.folder_id 列。
    // 先加列(后才能建 index),再建文件夹表,最后建复合索引。
    // 注意 V29 会把 folders rename 成 projects,所以这里也要条件化。
    let has_folders_v28: bool = tx
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='folders'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;
    let has_projects_v28: bool = tx
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='projects'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;
    let has_projects_col: bool = tx
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='project_id'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;
    let has_folders_col: bool = tx
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='folder_id'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;

    // sessions 的文件夹列:优先用 project_id,旧库可能还是 folder_id
    if !has_projects_col && !has_folders_col {
        ensure_column(&tx, "sessions", "folder_id", "TEXT")?;
    }
    // folders/projects 表:如果两个都不存在,建 folders(V29 会 rename)
    if !has_folders_v28 && !has_projects_v28 {
        tx.execute_batch(MIGRATION_V28_FOLDERS)?;
        ensure_column(&tx, "folders", "path", "TEXT NOT NULL DEFAULT ''")?;
    }
    // V29(P8.1 项目化升级):folders → projects,加 model_override + shared_context
    // sessions.folder_id → sessions.project_id。
    // 幂等:先检查表实际状态再决定执行哪条 DDL。
    let has_folders: bool = tx
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='folders'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;
    let has_projects: bool = tx
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='projects'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;
    let has_sessions_project_id: bool = tx
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='project_id'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;
    let has_sessions_folder_id: bool = tx
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='folder_id'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;

    // 1) folders 表存在 + projects 表不存在 → rename
    if has_folders && !has_projects {
        tx.execute_batch("ALTER TABLE folders RENAME TO projects;")?;
    }
    // 2) sessions.folder_id 存在 + project_id 不存在 → rename column
    if has_sessions_folder_id && !has_sessions_project_id {
        tx.execute_batch(
            "ALTER TABLE sessions RENAME COLUMN folder_id TO project_id;",
        )?;
    }
    // 3) projects 表存在且缺 model_override / shared_context → 加列
    if has_projects {
        let cols: Vec<String> = tx
            .prepare("SELECT name FROM pragma_table_info('projects')")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|c| c.ok())
            .collect();
        if !cols.contains(&"model_override".to_string()) {
            tx.execute_batch(
                "ALTER TABLE projects ADD COLUMN model_override TEXT;",
            )?;
        }
        if !cols.contains(&"shared_context".to_string()) {
            tx.execute_batch(
                "ALTER TABLE projects ADD COLUMN shared_context TEXT;",
            )?;
        }
    }
    // 4) 索引:projects 存在就建新的,sessions 上就建
    tx.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_sessions_project ON sessions(project_id, updated);",
    )?;
    tx.execute_batch("DROP INDEX IF EXISTS idx_sessions_folder;")?;
    // 旧数据回填:main 分支内按 id 顺序形成线性链。
    // 仅回填 main:其他分支首条消息的 parent_id 可能合法为 NULL(从会话开头分叉),
    // 且 fork 消息的 parent 在写入时已显式设置,不在此 touched。
    tx.execute_batch(
        "UPDATE messages SET parent_id = (
           SELECT MAX(m2.id) FROM messages m2
           WHERE m2.session_id = messages.session_id AND m2.id < messages.id
         ) WHERE parent_id IS NULL AND branch_id = 'main';",
    )?;
    // V30(P8.2 审计日志):MCP 工具调用审计表(写入/执行类工具每次调用记录一条)。
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS tool_call_log (
           id           INTEGER PRIMARY KEY AUTOINCREMENT,
           session_id   TEXT NOT NULL,
           project_id   TEXT,
           tool_name    TEXT NOT NULL,
           target_path  TEXT,
           args_preview TEXT,
           result       TEXT,
           created      TEXT NOT NULL DEFAULT (datetime('now','localtime'))
         );
         CREATE INDEX IF NOT EXISTS idx_tool_call_log_session ON tool_call_log(session_id, created);
         CREATE INDEX IF NOT EXISTS idx_tool_call_log_project ON tool_call_log(project_id, created);",
    )?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}
