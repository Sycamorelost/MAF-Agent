//! messages 表的 CRUD(含分支树,迭代 B6 / F4)。
//!
//! 分支模型(git 式链):每条消息带 parent_id 构成链;分支是挂在链上的命名指针,
//! 分支可见历史 = 从分支头沿 parent_id 回溯到根的整条链(共享前缀 + 分支自有消息)。
//! 分支元数据(分叉点 / 命名)见 db::branches。
//!
//! 所有核心逻辑以 `*_on(&Connection, ..)` 形式实现,便于内存库单测;
//! 同名全局函数仅负责锁全局连接后委托。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;

/// 消息行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRow {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub model_id: Option<String>,
    pub created: String,
    pub tokens: i64,
    /// 附件元数据 JSON 数组(V4;无附件为 NULL)。元素结构见 commands::attachment::AttachmentMeta。
    pub attachments: Option<String>,
    /// 联网来源 JSON 数组(V5;assistant 消息,无联网为 NULL)。元素结构见 research::SourceItem。
    pub sources: Option<String>,
    /// 链上父消息 id(V9;分支可见历史的回溯指针,会话/分支首条可能为 NULL)
    pub parent_id: Option<i64>,
    /// 所属分支(V9;'main' 或 8 位 hex)
    pub branch_id: String,
    /// 思考过程文本(V14;assistant 专用,与 content 分离;不进 FTS,避免搜索噪声)
    #[serde(default)]
    pub reasoning: String,
}

fn row_to_message(r: &rusqlite::Row) -> rusqlite::Result<MessageRow> {
    Ok(MessageRow {
        id: r.get(0)?,
        session_id: r.get(1)?,
        role: r.get(2)?,
        content: r.get(3)?,
        model_id: r.get(4)?,
        created: r.get(5)?,
        tokens: r.get(6)?,
        attachments: r.get(7)?,
        sources: r.get(8)?,
        parent_id: r.get(9)?,
        branch_id: r.get(10)?,
        reasoning: r.get(11)?,
    })
}

const SELECT_COLS: &str =
    "id, session_id, role, content, model_id, created, tokens, attachments, sources, parent_id, branch_id, reasoning";

/// 分支可见链 CTE:从分支尾(分支内 MAX id;空分支取分叉点)沿 parent_id 回溯到根。
/// 空 main(分叉点 NULL)锚点为 NULL,链为空集,查询结果自然为空。
const CHAIN_CTE: &str = "WITH RECURSIVE chain(id) AS (
    SELECT COALESCE(
        (SELECT MAX(id) FROM messages WHERE session_id=?1 AND branch_id=?2),
        (SELECT fork_parent_id FROM branches WHERE session_id=?1 AND branch_id=?2)
    )
    UNION ALL
    SELECT m.parent_id FROM messages m JOIN chain c ON m.id = c.id WHERE m.parent_id IS NOT NULL
)";

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

/// 列出分支可见的全部消息(沿链回溯,按时间正序)。
pub fn list_on(conn: &Connection, session_id: &str, branch_id: &str) -> Result<Vec<MessageRow>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "{} SELECT {} FROM messages WHERE id IN (SELECT id FROM chain) ORDER BY id ASC",
            CHAIN_CTE, SELECT_COLS
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![session_id, branch_id], row_to_message)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// 列出分支可见的全部消息(全局连接包装)。
pub fn list(session_id: &str, branch_id: &str) -> Result<Vec<MessageRow>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    list_on(&conn, session_id, branch_id)
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

/// 分支可见链的最近 N 条消息(用于拼接 LLM 上下文 / 记忆提炼)。
pub fn recent_on(
    conn: &Connection,
    session_id: &str,
    branch_id: &str,
    limit: i64,
) -> Result<Vec<MessageRow>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "{} SELECT {} FROM messages WHERE id IN (SELECT id FROM chain) ORDER BY id DESC LIMIT ?3",
            CHAIN_CTE, SELECT_COLS
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![session_id, branch_id, limit], row_to_message)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    out.reverse(); // 反回正序
    Ok(out)
}

/// 当前激活分支可见链的最近 N 条消息(全局连接包装)。
pub fn recent(session_id: &str, limit: i64) -> Result<Vec<MessageRow>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    let branch = db::sessions::active_branch_on(&conn, session_id)?;
    recent_on(&conn, session_id, &branch, limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{branches, schema};

    /// 内存库 + 全量迁移 + 一个会话(main 元数据行)。
    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, title, created, updated) VALUES('s1','新会话','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        branches::ensure_main_on(&conn, "s1").unwrap();
        conn
    }

    fn add(conn: &Connection, role: &str, content: &str, branch: &str) -> i64 {
        append_on(conn, "s1", role, content, None, 0, None, None, None, branch).unwrap()
    }

    fn ids(rows: &[MessageRow]) -> Vec<i64> {
        rows.iter().map(|m| m.id).collect()
    }

    /// V14:思考文本随 assistant 消息落库并可回读(默认空串,历史消息不受影响)。
    #[test]
    fn append_persists_reasoning() {
        let conn = setup();
        append_on(&conn, "s1", "assistant", "回答", None, 10, None, None, Some("我先想想"), "main")
            .unwrap();
        let rows = list_on(&conn, "s1", "main").unwrap();
        assert_eq!(rows[0].reasoning, "我先想想");
        // 无思考消息默认空串(而非 NULL,前端类型简单)
        append_on(&conn, "s1", "assistant", "第二条", None, 5, None, None, None, "main").unwrap();
        let rows = list_on(&conn, "s1", "main").unwrap();
        assert_eq!(rows[1].reasoning, "");
    }

    /// V14 验收:思考文本不进 FTS 全文索引(避免搜索噪声)。
    #[test]
    fn reasoning_not_indexed_by_fts() {
        let conn = setup();
        append_on(&conn, "s1", "assistant", "正常回答", None, 1, None, None, Some("内部独有思考词"), "main")
            .unwrap();
        // 正文命中
        let hit: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH '正常回答'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hit, 1);
        // 思考词不命中
        let miss: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH '内部独有思考词'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(miss, 0);
    }

    #[test]
    fn append_forms_linear_chain_in_main() {
        let conn = setup();
        let a = add(&conn, "user", "问一", "main");
        let b = add(&conn, "assistant", "答一", "main");
        let c = add(&conn, "user", "问二", "main");

        let rows = list_on(&conn, "s1", "main").unwrap();
        assert_eq!(ids(&rows), vec![a, b, c]);
        assert_eq!(rows[0].parent_id, None);
        assert_eq!(rows[1].parent_id, Some(a));
        assert_eq!(rows[2].parent_id, Some(b));
        assert!(rows.iter().all(|m| m.branch_id == "main"));
    }

    #[test]
    fn fork_shares_prefix_and_diverges() {
        let conn = setup();
        let a = add(&conn, "user", "问一", "main");
        let b = add(&conn, "assistant", "答一", "main");
        let c = add(&conn, "user", "问二", "main");

        // 在第二条消息(b)处分叉:新分支首条的 parent = b.parent = a
        let fork = branches::create_on(&conn, "s1", b, None).unwrap();
        let f1 = add(&conn, "user", "问二(改)", &fork);
        let f2 = add(&conn, "assistant", "答二(改)", &fork);

        // main 链不受影响
        assert_eq!(ids(&list_on(&conn, "s1", "main").unwrap()), vec![a, b, c]);
        // fork 可见链 = 共享前缀 [a] + 自有 [f1, f2]
        let fork_rows = list_on(&conn, "s1", &fork).unwrap();
        assert_eq!(ids(&fork_rows), vec![a, f1, f2]);
        assert_eq!(fork_rows[1].parent_id, Some(a));
        assert_eq!(fork_rows[1].branch_id, fork);
    }

    #[test]
    fn recent_follows_chain_and_limit() {
        let conn = setup();
        let a = add(&conn, "user", "问一", "main");
        let _b = add(&conn, "assistant", "答一", "main");
        let c = add(&conn, "user", "问二", "main");
        let d = add(&conn, "assistant", "答二", "main");

        let last2 = recent_on(&conn, "s1", "main", 2).unwrap();
        assert_eq!(ids(&last2), vec![c, d]);
        let all = recent_on(&conn, "s1", "main", 99).unwrap();
        assert_eq!(all.len(), 4);
        assert_eq!(all[0].id, a);
    }

    #[test]
    fn delete_one_reparents_children_across_branches() {
        let conn = setup();
        let a = add(&conn, "user", "问一", "main");
        let b = add(&conn, "assistant", "答一", "main");
        let c = add(&conn, "user", "问二", "main");
        let fork = branches::create_on(&conn, "s1", c, None).unwrap();
        let f1 = add(&conn, "user", "问二(改)", &fork);

        // 删除 main 的 c:fork 首条 f1 应重挂到 c 的父(b)
        delete_one_on(&conn, c).unwrap();
        let fork_rows = list_on(&conn, "s1", &fork).unwrap();
        assert_eq!(ids(&fork_rows), vec![a, b, f1]);
        assert_eq!(fork_rows[2].parent_id, Some(b));
        // main 链收缩为 [a, b]
        assert_eq!(ids(&list_on(&conn, "s1", "main").unwrap()), vec![a, b]);
    }

    #[test]
    fn clear_only_empties_target_branch() {
        let conn = setup();
        let a = add(&conn, "user", "问一", "main");
        let _b = add(&conn, "assistant", "答一", "main");
        let c = add(&conn, "user", "问二", "main");
        let fork = branches::create_on(&conn, "s1", c, None).unwrap();
        let _f1 = add(&conn, "user", "fork 消息", &fork);

        // 清空 fork:自有消息删除,共享前缀仍在(main 不受影响)
        clear_on(&conn, "s1", &fork).unwrap();
        assert_eq!(ids(&list_on(&conn, "s1", "main").unwrap()), vec![a, _b, c]);
        // fork 清空后可见链 = 分叉点之前的共享前缀 [a, _b]
        let fork_rows = list_on(&conn, "s1", &fork).unwrap();
        assert_eq!(ids(&fork_rows), vec![a, _b]);
    }

    #[test]
    fn delete_from_scoped_to_branch() {
        let conn = setup();
        let a = add(&conn, "user", "问一", "main");
        let b = add(&conn, "assistant", "答一", "main");
        let c = add(&conn, "user", "问二", "main");
        let fork = branches::create_on(&conn, "s1", c, None).unwrap();
        let f1 = add(&conn, "user", "fork 一", &fork);
        let f2 = add(&conn, "assistant", "fork 二", &fork);

        // 删 fork 中 id >= f1 的消息:main 不动,fork 回到分叉点前
        let n = delete_from_on(&conn, "s1", &fork, f1).unwrap();
        assert_eq!(n, 2);
        assert_eq!(ids(&list_on(&conn, "s1", "main").unwrap()), vec![a, b, c]);
        assert_eq!(ids(&list_on(&conn, "s1", &fork).unwrap()), vec![a, b]);
        let _ = f2;
    }
}
