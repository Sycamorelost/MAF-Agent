//! branches 表的 CRUD(迭代 B6 / F4 会话分支树)。
//!
//! 分支 = 挂在消息链上的命名指针;fork_parent_id 记录分叉点
//! (新分支首条消息的 parent_id,即被替换消息的前一条;NULL = 从会话开头分叉)。
//! 消息链本体见 db::messages。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;

/// 主分支 id(每个会话恒有)。
pub const MAIN_BRANCH: &str = "main";

/// 分支信息(列表展示用)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub branch_id: String,
    /// 用户自定义名(空 = 前端显示默认名「主分支 / 分支 xxxx」)
    pub name: String,
    pub created: String,
    /// 分叉点(main 为 NULL)
    pub fork_parent_id: Option<i64>,
    /// 分支自有消息数(不含共享前缀)
    pub message_count: i64,
    /// 首条自有消息内容摘要(换行压平,截 40 字符)
    pub first_summary: String,
}

/// 确保 main 元数据行存在(懒建;旧库升级后首次列分支时补)。
pub fn ensure_main_on(conn: &Connection, session_id: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR IGNORE INTO branches(session_id, branch_id, name, fork_parent_id, created)
         VALUES(?1, 'main', '', NULL, ?2)",
        rusqlite::params![session_id, chrono::Local::now().to_rfc3339()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 分支追加起点:分支头(分支内最大消息 id);分支尚无消息时取分叉点(main 缺失行 = None)。
pub fn tail_parent_on(
    conn: &Connection,
    session_id: &str,
    branch_id: &str,
) -> Result<Option<i64>, String> {
    let head: Option<i64> = conn
        .query_row(
            "SELECT MAX(id) FROM messages WHERE session_id=?1 AND branch_id=?2",
            rusqlite::params![session_id, branch_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if head.is_some() {
        return Ok(head);
    }
    let fp: Option<i64> = conn
        .query_row(
            "SELECT fork_parent_id FROM branches WHERE session_id=?1 AND branch_id=?2",
            rusqlite::params![session_id, branch_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    Ok(fp)
}

/// 列出会话全部分支(按创建时间正序)。
/// 自动确保 main 存在;顺带清理「无自有消息且非当前激活」的空 fork
/// (fork 后未发送就离开会留下空壳;激活中的空 fork 保留,可能正在等待首条写入)。
pub fn list_on(conn: &Connection, session_id: &str) -> Result<Vec<BranchInfo>, String> {
    ensure_main_on(conn, session_id)?;
    conn.execute(
        "DELETE FROM branches WHERE session_id=?1 AND branch_id<>'main'
         AND branch_id <> COALESCE((SELECT active_branch FROM sessions WHERE id=?1),'main')
         AND branch_id NOT IN (SELECT DISTINCT branch_id FROM messages WHERE session_id=?1)",
        [session_id],
    )
    .map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT b.branch_id, b.name, b.created, b.fork_parent_id,
                    (SELECT COUNT(*) FROM messages m WHERE m.session_id=b.session_id AND m.branch_id=b.branch_id),
                    COALESCE((SELECT m.content FROM messages m WHERE m.session_id=b.session_id AND m.branch_id=b.branch_id ORDER BY m.id ASC LIMIT 1),'')
             FROM branches b WHERE b.session_id=?1
             ORDER BY b.created ASC, b.branch_id ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([session_id], |r| {
            let first: String = r.get(5)?;
            let flat = first.replace('\n', " ");
            let mut summary: String = flat.chars().take(40).collect();
            if flat.chars().count() > 40 {
                summary.push('…');
            }
            Ok(BranchInfo {
                branch_id: r.get(0)?,
                name: r.get(1)?,
                created: r.get(2)?,
                fork_parent_id: r.get(3)?,
                message_count: r.get(4)?,
                first_summary: summary,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// fork:从 from_message_id 处分叉,返回新 branch_id(8 位 hex,会话内唯一)。
/// 新分支首条消息的 parent = 目标消息的 parent(即目标消息本身被新内容替换)。
pub fn create_on(
    conn: &Connection,
    session_id: &str,
    from_message_id: i64,
    name: Option<&str>,
) -> Result<String, String> {
    ensure_main_on(conn, session_id)?;
    let parent: Option<i64> = conn
        .query_row(
            "SELECT parent_id FROM messages WHERE id=?1 AND session_id=?2",
            rusqlite::params![from_message_id, session_id],
            |r| r.get(0),
        )
        .map_err(|_| format!("消息不存在或不属于该会话:{from_message_id}"))?;
    let id = loop {
        let cand = crate::memory::new_session_id();
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM branches WHERE session_id=?1 AND branch_id=?2",
                rusqlite::params![session_id, cand],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists == 0 {
            break cand;
        }
    };
    conn.execute(
        "INSERT INTO branches(session_id, branch_id, name, fork_parent_id, created) VALUES(?1,?2,?3,?4,?5)",
        rusqlite::params![
            session_id,
            id,
            name.unwrap_or("").trim(),
            parent,
            chrono::Local::now().to_rfc3339()
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

/// 重命名分支(空名 = 恢复默认显示名)。
pub fn rename_on(
    conn: &Connection,
    session_id: &str,
    branch_id: &str,
    name: &str,
) -> Result<(), String> {
    let n = conn
        .execute(
            "UPDATE branches SET name=?1 WHERE session_id=?2 AND branch_id=?3",
            rusqlite::params![name.trim(), session_id, branch_id],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("分支不存在:{branch_id}"));
    }
    Ok(())
}

/// 删除分支:删自有消息(同步清 FTS)+ 元数据行;main 不可删除。
/// 跨分支引用本分支消息的统一重挂到本分支分叉点,保持其他分支链连续;
/// 若删除的是当前激活分支,sessions.active_branch 回退 main。
pub fn delete_on(conn: &Connection, session_id: &str, branch_id: &str) -> Result<(), String> {
    if branch_id == MAIN_BRANCH {
        return Err("主分支不可删除".into());
    }
    let root: Option<i64> = conn
        .query_row(
            "SELECT fork_parent_id FROM branches WHERE session_id=?1 AND branch_id=?2",
            rusqlite::params![session_id, branch_id],
            |r| r.get(0),
        )
        .map_err(|_| format!("分支不存在:{branch_id}"))?;
    // M-13:重挂 / FTS / 消息删除 / 分支删除 / 激活回退六步同事务
    // unchecked_transaction:调用方持 &Connection,串行性由全局 db Mutex 保证
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
        log::warn!("messages_fts 分支删除失败: {e}");
    }
    tx.execute(
        "DELETE FROM messages WHERE session_id=?1 AND branch_id=?2",
        rusqlite::params![session_id, branch_id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM branches WHERE session_id=?1 AND branch_id=?2",
        rusqlite::params![session_id, branch_id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE sessions SET active_branch='main' WHERE id=?1 AND active_branch=?2",
        rusqlite::params![session_id, branch_id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- 全局连接包装(命令层用) ----------

pub fn list(session_id: &str) -> Result<Vec<BranchInfo>, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    list_on(&conn, session_id)
}

pub fn create(session_id: &str, from_message_id: i64, name: Option<&str>) -> Result<String, String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    create_on(&conn, session_id, from_message_id, name)
}

pub fn rename(session_id: &str, branch_id: &str, name: &str) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    rename_on(&conn, session_id, branch_id, name)
}

pub fn delete(session_id: &str, branch_id: &str) -> Result<(), String> {
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    delete_on(&conn, session_id, branch_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{messages, schema};

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, title, created, updated) VALUES('s1','新会话','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn
    }

    fn add(conn: &Connection, content: &str, branch: &str) -> i64 {
        messages::append_on(conn, "s1", "user", content, None, 0, None, None, None, branch).unwrap()
    }

    #[test]
    fn list_ensures_main_and_prunes_empty_inactive_forks() {
        let conn = setup();
        let m1 = add(&conn, "问一", "main");
        // 空 fork(无消息且非激活)在 list 时被清理
        let empty = create_on(&conn, "s1", m1, None).unwrap();
        let list = list_on(&conn, "s1").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].branch_id, "main");
        assert_eq!(list[0].message_count, 1);
        assert_eq!(list[0].first_summary, "问一");
        let _ = empty;

        // 有消息的 fork 保留
        let fork = create_on(&conn, "s1", m1, None).unwrap();
        add(&conn, "fork 消息", &fork);
        let list = list_on(&conn, "s1").unwrap();
        assert_eq!(list.len(), 2);
        let f = list.iter().find(|b| b.branch_id == fork).unwrap();
        assert_eq!(f.message_count, 1);
        assert_eq!(f.first_summary, "fork 消息");
        assert_eq!(f.fork_parent_id, None); // m1 是首条,parent 为 NULL
    }

    #[test]
    fn fork_at_first_message_has_null_fork_parent() {
        let conn = setup();
        let m1 = add(&conn, "问一", "main");
        let fork = create_on(&conn, "s1", m1, None).unwrap();
        // 分叉点 = m1.parent = NULL → fork 首条消息也从会话开头起
        let f1 = add(&conn, "重写开头", &fork);
        let rows = messages::list_on(&conn, "s1", &fork).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, f1);
        assert_eq!(rows[0].parent_id, None);
    }

    #[test]
    fn create_rejects_unknown_message() {
        let conn = setup();
        assert!(create_on(&conn, "s1", 9999, None).is_err());
    }

    #[test]
    fn rename_roundtrip() {
        let conn = setup();
        let m1 = add(&conn, "问一", "main");
        let fork = create_on(&conn, "s1", m1, None).unwrap();
        add(&conn, "fork 消息", &fork);
        rename_on(&conn, "s1", &fork, "方案 B").unwrap();
        let list = list_on(&conn, "s1").unwrap();
        let f = list.iter().find(|b| b.branch_id == fork).unwrap();
        assert_eq!(f.name, "方案 B");
        assert!(rename_on(&conn, "s1", "notexist", "x").is_err());
    }

    #[test]
    fn delete_branch_keeps_other_chains_and_falls_back_active() {
        let conn = setup();
        let m1 = add(&conn, "问一", "main");
        let m2 = add(&conn, "答一", "main");
        let fork = create_on(&conn, "s1", m2, None).unwrap();
        let f1 = add(&conn, "fork 消息", &fork);
        // 二级 fork:从 fork 的自有消息再分叉
        let fork2 = create_on(&conn, "s1", f1, None).unwrap();
        add(&conn, "二级 fork 消息", &fork2);
        // 激活 fork,删除后应回退 main
        conn.execute("UPDATE sessions SET active_branch=?1 WHERE id='s1'", [&fork])
            .unwrap();

        delete_on(&conn, "s1", &fork).unwrap();

        // main 不动
        let main_rows = messages::list_on(&conn, "s1", "main").unwrap();
        assert_eq!(main_rows.len(), 2);
        // 二级 fork 的 fork_parent 重挂到 fork 的分叉点(m1):可见链 = [m1, 二级消息]
        let f2_rows = messages::list_on(&conn, "s1", &fork2).unwrap();
        assert_eq!(f2_rows.len(), 2);
        assert_eq!(f2_rows[0].id, m1);
        // 激活分支回退 main
        let active: String = conn
            .query_row("SELECT active_branch FROM sessions WHERE id='s1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(active, "main");
    }

    #[test]
    fn delete_main_rejected() {
        let conn = setup();
        assert!(delete_on(&conn, "s1", "main").is_err());
    }
}
