//! sessions 表的 CRUD。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;

/// 会话行(与表结构对齐)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRow {
    pub id: String,
    pub title: String,
    pub created: String,
    pub updated: String,
    pub model_id: Option<String>,
    pub memory_version: i64,
    /// 会话级系统提示词(V4;None/空 = 用默认)
    pub system_prompt: Option<String>,
    /// 上下文滑窗:注入 LLM 的最近消息条数上限(V4;0 = 全部)
    pub max_context_messages: i64,
    /// 对话内联网开关(V5)
    pub web_search_enabled: bool,
    /// 切换模型时的记忆注入策略(V6):full / summary / hybrid / none
    pub memory_injection_strategy: String,
    /// 绑定的 Agent 模板 id(V7;None = 空白会话)
    pub agent_id: Option<String>,
    /// 当前激活分支(V9;'main' 或 8 位 hex)
    pub active_branch: String,
}

fn row_to_session(r: &rusqlite::Row) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: r.get(0)?,
        title: r.get(1)?,
        created: r.get(2)?,
        updated: r.get(3)?,
        model_id: r.get(4)?,
        memory_version: r.get(5)?,
        system_prompt: r.get(6)?,
        max_context_messages: r.get(7)?,
        web_search_enabled: r.get::<_, i64>(8)? != 0,
        memory_injection_strategy: r.get(9)?,
        agent_id: r.get(10)?,
        active_branch: r.get(11)?,
    })
}

const SELECT_COLS: &str = "id, title, created, updated, model_id, memory_version, system_prompt, max_context_messages, web_search_enabled, memory_injection_strategy, agent_id, active_branch";

pub fn get(id: &str) -> Result<Option<SessionRow>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!("SELECT {} FROM sessions WHERE id=?1", SELECT_COLS))
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map([id], row_to_session)
        .map_err(|e| e.to_string())?;
    match rows.next() {
        Some(Ok(r)) => Ok(Some(r)),
        Some(Err(e)) => Err(e.to_string()),
        None => Ok(None),
    }
}

pub fn list() -> Result<Vec<SessionRow>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {} FROM sessions ORDER BY updated DESC",
            SELECT_COLS
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_session)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

pub fn update_title(id: &str, title: &str) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sessions SET title=?1, updated=?2 WHERE id=?3",
        rusqlite::params![title, chrono::Local::now().to_rfc3339(), id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn update_model(id: &str, model_id: &str) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sessions SET model_id=?1, updated=?2 WHERE id=?3",
        rusqlite::params![model_id, chrono::Local::now().to_rfc3339(), id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 更新会话级设置(系统提示词 / 上下文滑窗)。
/// system_prompt 传空字符串视为清除(存 NULL);max_context_messages 传负数归一为 0(全部)。
pub fn update_settings(
    id: &str,
    system_prompt: Option<&str>,
    max_context_messages: i64,
) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let sp = system_prompt.map(str::trim).filter(|s| !s.is_empty());
    conn.execute(
        "UPDATE sessions SET system_prompt=?1, max_context_messages=?2, updated=?3 WHERE id=?4",
        rusqlite::params![
            sp,
            max_context_messages.max(0),
            chrono::Local::now().to_rfc3339(),
            id
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 单独更新会话级系统提示词(B4:Agent 渲染写入 / 应用为提示词)。空串视为清除(存 NULL)。
pub fn set_system_prompt(id: &str, system_prompt: Option<&str>) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let sp = system_prompt.map(str::trim).filter(|s| !s.is_empty());
    conn.execute(
        "UPDATE sessions SET system_prompt=?1, updated=?2 WHERE id=?3",
        rusqlite::params![sp, chrono::Local::now().to_rfc3339(), id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 更新对话内联网开关(V5)。
pub fn set_web_search(id: &str, enabled: bool) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sessions SET web_search_enabled=?1 WHERE id=?2",
        rusqlite::params![if enabled { 1 } else { 0 }, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 合法的记忆注入策略(迭代 D3.1)。
pub const INJECTION_STRATEGIES: [&str; 4] = ["full", "summary", "hybrid", "none"];

/// 更新切换模型时的记忆注入策略(V6)。非法策略报错。
pub fn set_memory_strategy(id: &str, strategy: &str) -> Result<(), String> {
    if !INJECTION_STRATEGIES.contains(&strategy) {
        return Err(format!(
            "invalid memory_injection_strategy: {strategy}(可选:{})",
            INJECTION_STRATEGIES.join("/")
        ));
    }
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sessions SET memory_injection_strategy=?1 WHERE id=?2",
        rusqlite::params![strategy, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 更新会话绑定的 Agent 模板(V7,迭代 B2)。agent_id 为 None 表示解除绑定。
pub fn set_agent(id: &str, agent_id: Option<&str>) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sessions SET agent_id=?1 WHERE id=?2",
        rusqlite::params![agent_id, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 读取会话当前激活分支(V9)。
pub fn active_branch_on(conn: &Connection, id: &str) -> Result<String, String> {
    conn.query_row(
        "SELECT active_branch FROM sessions WHERE id=?1",
        [id],
        |r| r.get(0),
    )
    .map_err(|e| format!("读取激活分支失败:{e}"))
}

/// 读取会话当前激活分支(全局连接包装)。
pub fn active_branch(id: &str) -> Result<String, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    active_branch_on(&conn, id)
}

/// 切换激活分支(V9)。分支必须存在于 branches 表(main 懒建后恒有效)。
pub fn set_active_branch(id: &str, branch_id: &str) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    db::branches::ensure_main_on(&conn, id)?;
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM branches WHERE session_id=?1 AND branch_id=?2",
            rusqlite::params![id, branch_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists == 0 {
        return Err(format!("分支不存在:{branch_id}"));
    }
    conn.execute(
        "UPDATE sessions SET active_branch=?1 WHERE id=?2",
        rusqlite::params![branch_id, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete(id: &str) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    // 先清 FTS(messages 由外键级联删除,FTS 需手动同步)
    let _ = conn.execute(
        "DELETE FROM messages_fts WHERE rowid IN (SELECT id FROM messages WHERE session_id=?1)",
        [id],
    );
    conn.execute("DELETE FROM sessions WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 仅当会话没有任何消息且已过宽限期时删除,返回是否已删。空对话默认不保留。
/// 宽限期避免误删「刚创建还没发第一条消息」的会话:
/// React StrictMode 开发模式下 effect 会执行「挂载→清理→再挂载」,
/// 离开页触发的 removeIfEmpty 若无宽限会把当前正在使用的新会话秒删。
pub fn delete_if_empty(id: &str) -> Result<bool, String> {
    const GRACE_SECS: i64 = 60;
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let cutoff = (chrono::Local::now() - chrono::Duration::seconds(GRACE_SECS)).to_rfc3339();
    let cnt: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE session_id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if cnt > 0 {
        return Ok(false);
    }
    let n = conn
        .execute(
            "DELETE FROM sessions WHERE id=?1 AND updated < ?2",
            rusqlite::params![id, cutoff],
        )
        .map_err(|e| e.to_string())?;
    Ok(n > 0)
}

/// 批量清理空会话(0 条消息且 updated 早于 grace_secs 秒前),返回被删会话 id 列表。
/// grace 避免误删「刚创建还没发第一条消息」的会话(划词链路先建会话后发消息)。
pub fn prune_empty(grace_secs: i64) -> Result<Vec<String>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let cutoff = (chrono::Local::now() - chrono::Duration::seconds(grace_secs.max(0))).to_rfc3339();
    let mut stmt = conn
        .prepare(
            "SELECT id FROM sessions WHERE updated < ?1 \
             AND NOT EXISTS (SELECT 1 FROM messages WHERE messages.session_id = sessions.id)",
        )
        .map_err(|e| e.to_string())?;
    let ids: Vec<String> = stmt
        .query_map([cutoff], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for id in &ids {
        conn.execute("DELETE FROM sessions WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
    }
    Ok(ids)
}

/// 仅供内部复用(测试或迁移),不暴露给前端。
#[allow(dead_code)]
pub(crate) fn with_conn<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&Connection) -> Result<R, String>,
{
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    f(&conn)
}
