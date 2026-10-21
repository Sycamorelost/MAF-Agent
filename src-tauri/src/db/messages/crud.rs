//! messages 写操作:追加 / 删除 / 清空 / 更新正文。

use rusqlite::Connection;

use crate::db;

/// 追加一条消息,返回 message id。attachments / sources 为 JSON 数组字符串(无则传 None)。
/// reasoning 为思考过程文本(assistant 专用,无则传 None/空)。parent_id 自动取分支追加起点
/// (分支头;空分支取分叉点),调用方无需关心。
#[allow(clippy::too_many_arguments)]
pub fn append_on(
    conn: &Connection,
    session_id: &str,
    role: &str,
    content: &str,
    model_id: Option<&str>,
    tokens: i64,
    attachments: Option<&str>,
    sources: Option<&str>,
    reasoning: Option<&str>,
    branch_id: &str,
) -> Result<i64, String> {
    let parent_id = db::branches::tail_parent_on(conn, session_id, branch_id)?;
    let now = chrono::Local::now().to_rfc3339();
    // M-13:消息行 / sessions.updated / FTS / 自动标题同事务,
    // 任一业务写失败整体回滚,避免消息落库但链/索引半成品。
    // unchecked_transaction:调用方持 &Connection,串行性由全局 db Mutex 保证。
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO messages(session_id, role, content, model_id, created, tokens, attachments, sources, reasoning, parent_id, branch_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        rusqlite::params![session_id, role, content, model_id, now, tokens, attachments, sources, reasoning.unwrap_or(""), parent_id, branch_id],
    )
    .map_err(|e| e.to_string())?;
    let id = tx.last_insert_rowid();

    // 同时刷新 sessions.updated
    tx.execute(
        "UPDATE sessions SET updated=?1 WHERE id=?2",
        rusqlite::params![now, session_id],
    )
    .map_err(|e| e.to_string())?;

    // FTS 索引同步(附属索引:失败仅告警不回滚业务数据,避免索引异常阻断发消息)
    if let Err(e) = tx.execute(
        "INSERT INTO messages_fts(rowid, content) VALUES(?1,?2)",
        rusqlite::params![id, content],
    ) {
        log::warn!("messages_fts 索引失败(rowid={id}): {e}");
    }

    // 首条用户消息:若标题仍是占位符,用消息内容自动生成标题
    if role == "user" {
        let _ = auto_title(&tx, session_id, content);
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(id)
}

/// 追加一条消息(全局连接包装)。
#[allow(clippy::too_many_arguments)]
pub fn append(
    session_id: &str,
    role: &str,
    content: &str,
    model_id: Option<&str>,
    tokens: i64,
    attachments: Option<&str>,
    sources: Option<&str>,
    reasoning: Option<&str>,
    branch_id: &str,
) -> Result<i64, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    append_on(&conn, session_id, role, content, model_id, tokens, attachments, sources, reasoning, branch_id)
}

/// 占位标题前缀(前端创建会话时的默认命名,见 Sidebar/MiniChat/SelectionBubble/FloatingBall)。
const PLACEHOLDER_PREFIXES: [&str; 4] = ["新会话", "快捷对话", "划词提问", "拖拽提问"];

/// 首条用户消息落库后,若会话标题仍是占位符,则用消息内容生成标题(去引用块,截 20 字)。
fn auto_title(conn: &rusqlite::Connection, session_id: &str, content: &str) -> Result<(), String> {
    // 仅首条用户消息触发(此时 count=1)
    let cnt: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE session_id=?1 AND role='user'",
            [session_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if cnt != 1 {
        return Ok(());
    }
    let title: String = conn
        .query_row(
            "SELECT title FROM sessions WHERE id=?1",
            [session_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !PLACEHOLDER_PREFIXES.iter().any(|p| title.starts_with(p)) {
        return Ok(());
    }
    // 跳过前导引用块(划词提问消息以 > 引用开头),取首行非空文本
    let first = content
        .lines()
        .filter(|l| !l.trim_start().starts_with('>'))
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if first.is_empty() {
        return Ok(());
    }
    let mut t: String = first.chars().take(20).collect();
    if first.chars().count() > 20 {
        t.push('…');
    }
    conn.execute(
        "UPDATE sessions SET title=?1 WHERE id=?2",
        rusqlite::params![t, session_id],
    )
    .map_err(|e| e.to_string())?;
    // 同步 memory.md frontmatter 的 conversation(会话与记忆标题保持一致)
    let _ = crate::memory::sync_title(session_id, &t);
    Ok(())
}

/// 删除单条消息(同步清 FTS)。
/// 链完整性:把指向本消息的子消息(含其他分支的首条)重挂到本消息的父消息,
/// 并同步重挂 branches.fork_parent_id 引用,避免删除后其他分支可见历史被截断。
pub fn delete_one_on(conn: &Connection, id: i64) -> Result<(), String> {
    let parent: Option<i64> = conn
        .query_row("SELECT parent_id FROM messages WHERE id=?1", [id], |r| r.get(0))
        .ok()
        .flatten();
    // M-13:链重挂 / 分支指针 / FTS / 删行同事务,中途失败整体回滚
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE messages SET parent_id=?1 WHERE parent_id=?2",
        rusqlite::params![parent, id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE branches SET fork_parent_id=?1 WHERE fork_parent_id=?2",
        rusqlite::params![parent, id],
    )
    .map_err(|e| e.to_string())?;
    if let Err(e) = tx.execute("DELETE FROM messages_fts WHERE rowid=?1", [id]) {
        log::warn!("messages_fts 删除失败(rowid={id}): {e}");
    }
    tx.execute("DELETE FROM messages WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// 删除单条消息(全局连接包装)。
pub fn delete_one(id: i64) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    delete_one_on(&conn, id)
}

/// 删除指定分支中 id >= from_id 的全部自有消息(放弃分支尾部用;同步清 FTS)。
/// 跨分支引用被删区段的消息 / 分叉点统一重挂到 from_id 的父消息,保持链连续。
pub fn delete_from_on(
    conn: &Connection,
    session_id: &str,
    branch_id: &str,
    from_id: i64,
) -> Result<u64, String> {
    let anchor: Option<i64> = conn
        .query_row(
            "SELECT parent_id FROM messages WHERE id=?1 AND session_id=?2",
            rusqlite::params![from_id, session_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    // M-13:跨分支重挂 / 分支指针 / FTS / 区段删行同事务
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE messages SET parent_id=?1 WHERE parent_id IN (
           SELECT id FROM messages WHERE session_id=?2 AND branch_id=?3 AND id>=?4)",
        rusqlite::params![anchor, session_id, branch_id, from_id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE branches SET fork_parent_id=?1 WHERE session_id=?2 AND fork_parent_id IN (
           SELECT id FROM messages WHERE session_id=?2 AND branch_id=?3 AND id>=?4)",
        rusqlite::params![anchor, session_id, branch_id, from_id],
    )
    .map_err(|e| e.to_string())?;
    if let Err(e) = tx.execute(
        "DELETE FROM messages_fts WHERE rowid IN (SELECT id FROM messages WHERE session_id=?1 AND branch_id=?2 AND id>=?3)",
        rusqlite::params![session_id, branch_id, from_id],
    ) {
        log::warn!("messages_fts 区段删除失败: {e}");
    }
    let n = tx
        .execute(
            "DELETE FROM messages WHERE session_id=?1 AND branch_id=?2 AND id>=?3",
            rusqlite::params![session_id, branch_id, from_id],
        )
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(n as u64)
}

/// 删除指定分支中 id >= from_id 的全部自有消息(全局连接包装)。
pub fn delete_from(session_id: &str, branch_id: &str, from_id: i64) -> Result<u64, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    delete_from_on(&conn, session_id, branch_id, from_id)
}

/// 更新消息内容(编辑重发用;同步更新 FTS)。
pub fn update_content(id: i64, content: &str) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    // M-13:正文与 FTS 索引同事务(全局 Mutex 串行,unchecked 安全)
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("UPDATE messages SET content=?1 WHERE id=?2", rusqlite::params![content, id])
        .map_err(|e| e.to_string())?;
    if let Err(e) = tx.execute("DELETE FROM messages_fts WHERE rowid=?1", [id]) {
        log::warn!("messages_fts 旧索引删除失败(rowid={id}): {e}");
    }
    if let Err(e) = tx.execute(
        "INSERT INTO messages_fts(rowid, content) VALUES(?1,?2)",
        rusqlite::params![id, content],
    ) {
        log::warn!("messages_fts 重建失败(rowid={id}): {e}");
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// 清空指定分支的自有消息(同步清 FTS)。
/// 跨分支引用本分支消息的统一重挂到本分支分叉点;本分支保留(回到空分支状态)。
pub fn clear_on(conn: &Connection, session_id: &str, branch_id: &str) -> Result<(), String> {
    let root: Option<i64> = conn
        .query_row(
            "SELECT fork_parent_id FROM branches WHERE session_id=?1 AND branch_id=?2",
            rusqlite::params![session_id, branch_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    // M-13:跨分支重挂 / FTS / 删行同事务
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE messages SET parent_id=?1 WHERE parent_id IN (
           SELECT id FROM messages WHERE session_id=?2 AND branch_id=?3)",
        rusqlite::params![root, session_id, branch_id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE branches SET fork_parent_id=?1 WHERE session_id=?2 AND branch_id<>?3 AND fork_parent_id IN (
           SELECT id FROM messages WHERE session_id=?2 AND branch_id=?3)",
        rusqlite::params![root, session_id, branch_id],
    )
    .map_err(|e| e.to_string())?;
    if let Err(e) = tx.execute(
        "DELETE FROM messages_fts WHERE rowid IN (SELECT id FROM messages WHERE session_id=?1 AND branch_id=?2)",
        rusqlite::params![session_id, branch_id],
    ) {
        log::warn!("messages_fts 清空失败: {e}");
    }
    tx.execute(
        "DELETE FROM messages WHERE session_id=?1 AND branch_id=?2",
        rusqlite::params![session_id, branch_id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// 清空指定分支的自有消息(全局连接包装)。
pub fn clear(session_id: &str, branch_id: &str) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    clear_on(&conn, session_id, branch_id)
}
