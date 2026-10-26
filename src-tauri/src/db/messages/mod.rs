//! messages 表的 CRUD(含分支树,迭代 B6 / F4)。
//!
//! 分支模型(git 式链):每条消息带 parent_id 构成链;分支是挂在链上的命名指针,
//! 分支可见历史 = 从分支头沿 parent_id 回溯到根的整条链(共享前缀 + 分支自有消息)。
//! 分支元数据(分叉点 / 命名)见 db::branches。
//!
//! 所有核心逻辑以 `*_on(&Connection, ..)` 形式实现,便于内存库单测;
//! 同名全局函数仅负责锁全局连接后委托。
//!
//! 拆分(§10.2.1):`crud` 负责写入/删除,`search` 负责链式读取;
//! 本文件保留行结构与链 CTE,并再导出原 `crate::db::messages::*` 路径。

mod crud;
mod search;

#[allow(unused_imports)]
pub use crud::{
    append, append_on, clear, clear_on, delete_from, delete_from_on, delete_one, delete_one_on,
    update_content,
};
#[allow(unused_imports)]
pub use search::{list, list_on, list_page, list_page_on, recent, recent_on};

use serde::{Deserialize, Serialize};

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

const SELECT_COLS: &str = "id, session_id, role, content, model_id, created, tokens, attachments, sources, parent_id, branch_id, reasoning";

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{branches, schema};
    use rusqlite::Connection;

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
        append_on(
            &conn,
            "s1",
            "assistant",
            "回答",
            None,
            10,
            None,
            None,
            Some("我先想想"),
            "main",
        )
        .unwrap();
        let rows = list_on(&conn, "s1", "main").unwrap();
        assert_eq!(rows[0].reasoning, "我先想想");
        // 无思考消息默认空串(而非 NULL,前端类型简单)
        append_on(
            &conn,
            "s1",
            "assistant",
            "第二条",
            None,
            5,
            None,
            None,
            None,
            "main",
        )
        .unwrap();
        let rows = list_on(&conn, "s1", "main").unwrap();
        assert_eq!(rows[1].reasoning, "");
    }

    /// V14 验收:思考文本不进 FTS 全文索引(避免搜索噪声)。
    #[test]
    fn reasoning_not_indexed_by_fts() {
        let conn = setup();
        append_on(
            &conn,
            "s1",
            "assistant",
            "正常回答",
            None,
            1,
            None,
            None,
            Some("内部独有思考词"),
            "main",
        )
        .unwrap();
        // 正文命中
        let hit: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH '正常回答'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hit, 1);
        // 思考词不命中
        let miss: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH '内部独有思考词'",
                [],
                |r| r.get(0),
            )
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
    fn list_page_walks_backwards_with_has_more() {
        let conn = setup();
        let a = add(&conn, "user", "问一", "main");
        let b = add(&conn, "assistant", "答一", "main");
        let c = add(&conn, "user", "问二", "main");
        let d = add(&conn, "assistant", "答二", "main");
        let e = add(&conn, "user", "问三", "main");

        // 最近一页:升序返回
        let (page1, more1) = list_page_on(&conn, "s1", "main", None, 2).unwrap();
        assert_eq!(ids(&page1), vec![d, e]);
        assert!(more1);
        // 以本页第一条为游标,继续向前
        let (page2, more2) = list_page_on(&conn, "s1", "main", Some(page1[0].id), 2).unwrap();
        assert_eq!(ids(&page2), vec![b, c]);
        assert!(more2);
        let (page3, more3) = list_page_on(&conn, "s1", "main", Some(page2[0].id), 2).unwrap();
        assert_eq!(ids(&page3), vec![a]);
        assert!(!more3);
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
