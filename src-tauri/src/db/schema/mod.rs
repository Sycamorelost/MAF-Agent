//! SQLite schema 定义与迁移。
//!
//! 对应 plan/01-phase1-mvp.md §3.1。
//! 通过 PRAGMA user_version 做版本迁移,当前版本 = SCHEMA_VERSION(20)。
//!
//! 拆分(§10.2.1):`migrate_v1` 收敛 V1–V15 的迁移 SQL 与自愈式加列助手,
//! `v16` 收敛 V16–V17 的迁移 SQL;版本执行顺序仍由本文件统一编排。

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
    MIGRATION_V20_INDEX,
};

/// 当前 schema 版本。每次结构变更 +1 并追加迁移 SQL。
pub const SCHEMA_VERSION: i32 = 20;

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
        "messages",
        "messages_fts",
        "memory_versions",
        "research_sessions",
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
    ensure_column(
        &tx,
        "sessions",
        "memory_injection_strategy",
        "TEXT NOT NULL DEFAULT 'full'",
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
    // V12(专项研究):研究模式 / 当前阶段 / 来源方案名
    ensure_column(
        &tx,
        "research_sessions",
        "mode",
        "TEXT NOT NULL DEFAULT 'legacy'",
    )?;
    ensure_column(
        &tx,
        "research_sessions",
        "current_stage",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(&tx, "research_sessions", "blueprint_name", "TEXT")?;
    // V13(专项研究 M4):阶段进度回写列,列表页零文件 IO(StageDone/StageSkipped 时更新)
    ensure_column(
        &tx,
        "research_sessions",
        "stages_done",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        &tx,
        "research_sessions",
        "stages_total",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
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
