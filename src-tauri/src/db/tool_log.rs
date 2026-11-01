//! MCP 工具调用审计日志(V30 / P8.2)。
//!
//! 每次 MCP 工具调用(尤其是写入/执行类)落一条,供事后追溯 Agent 行为。
//! 轻量表:args_preview 截断到 500 字符,避免大参数撑爆行。
//! 保留策略:单会话最近 [`KEEP_PER_SESSION`] 条,超出后随写入顺手裁剪最旧的
//! (与 `db::events` 的 500 条策略对齐,避免长期使用无限增长)。

#![allow(dead_code)]

use crate::db;

/// 单会话保留的调用记录条数上限。
const KEEP_PER_SESSION: i64 = 500;

/// 写入审计日志;静默吞掉错误(审计失败不影响主流程)。
/// 返回新行 id(供前端「查看完整输出」按 id 拉全文);无会话 / 库未就绪时返回 None。
pub fn record(
    session_id: &str,
    project_id: Option<&str>,
    tool_name: &str,
    target_path: Option<&str>,
    args_preview: Option<&str>,
    result: &str,
) -> Option<i64> {
    if !db::is_initialized() {
        return None;
    }
    let conn = match db::conn().lock() {
        Ok(c) => c,
        Err(_) => return None,
    };
    // 按字符截断:字节切片(&s[..500])会在中文等多字节字符中间 panic,
    // 而参数里几乎必然带中文(文件内容/路径),这条旧写法会直接把会话打断。
    let preview = args_preview
        .map(|s| crate::mcp::safety::truncate_chars(s, 500))
        .filter(|s| !s.is_empty());
    // P4:成功结果也存全文(此前只存 "ok"),否则工具卡片无法「查看完整输出」
    let stored = if result.trim().is_empty() {
        "ok"
    } else {
        result
    };
    match conn.execute(
        "INSERT INTO tool_call_log(session_id, project_id, tool_name, target_path, args_preview, result) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            session_id,
            project_id,
            tool_name,
            target_path,
            preview,
            stored
        ],
    ) {
        Ok(_) => {
            let id = conn.last_insert_rowid();
            // 轻量裁剪:只清自己会话的超限旧行(一次索引范围删除,代价可忽略)。
            // 失败不影响本次写入结果,下次写入会再次尝试。
            let _ = trim_session(&conn, session_id);
            Some(id)
        }
        Err(_) => None,
    }
}

/// 裁剪某会话的旧记录,只保留最近 [`KEEP_PER_SESSION`] 条。
pub(crate) fn trim_session(conn: &rusqlite::Connection, session_id: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM tool_call_log WHERE session_id=?1 AND id NOT IN (
           SELECT id FROM tool_call_log WHERE session_id=?1 ORDER BY id DESC LIMIT ?2
         )",
        rusqlite::params![session_id, KEEP_PER_SESSION],
    )
}

/// 按 id 读取单条调用记录(全文),供「查看完整输出」按需加载。
pub fn get(id: i64) -> Option<ToolCallLog> {
    if !db::is_initialized() {
        return None;
    }
    let conn = db::conn().lock().ok()?;
    conn.query_row(
        "SELECT id, session_id, project_id, tool_name, target_path, args_preview, result, created \
         FROM tool_call_log WHERE id=?1",
        rusqlite::params![id],
        |r| {
            Ok(ToolCallLog {
                id: r.get(0)?,
                session_id: r.get(1)?,
                project_id: r.get(2)?,
                tool_name: r.get(3)?,
                target_path: r.get(4)?,
                args_preview: r.get(5)?,
                result: r.get(6)?,
                created: r.get(7)?,
            })
        },
    )
    .ok()
}

/// 按会话查最近调用(最多 200 条,按时间倒序)。
pub fn list_by_session(session_id: &str, limit: Option<i64>) -> Vec<ToolCallLog> {
    let conn = match db::conn().lock() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let limit = limit.unwrap_or(200).clamp(1, 1000);
    let mut stmt = match conn.prepare(
        "SELECT id, session_id, project_id, tool_name, target_path, args_preview, result, created \
         FROM tool_call_log WHERE session_id=?1 ORDER BY id DESC LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt
        .query_map(rusqlite::params![session_id, limit], |r| {
            Ok(ToolCallLog {
                id: r.get(0)?,
                session_id: r.get(1)?,
                project_id: r.get(2)?,
                tool_name: r.get(3)?,
                target_path: r.get(4)?,
                args_preview: r.get(5)?,
                result: r.get(6)?,
                created: r.get(7)?,
            })
        })
        .ok();
    rows.into_iter().flatten().flatten().collect()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolCallLog {
    pub id: i64,
    pub session_id: String,
    pub project_id: Option<String>,
    pub tool_name: String,
    pub target_path: Option<String>,
    pub args_preview: Option<String>,
    pub result: String,
    pub created: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P2 保留策略:超出上限的旧行被裁掉,且只影响本会话。
    #[test]
    fn trim_keeps_latest_rows_per_session_only() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        let insert = |sid: &str| {
            conn.execute(
                "INSERT INTO tool_call_log(session_id, tool_name, result) VALUES (?1, 'fs_read', 'ok')",
                rusqlite::params![sid],
            )
            .unwrap();
        };
        for _ in 0..(KEEP_PER_SESSION + 5) {
            insert("s1");
        }
        for _ in 0..3 {
            insert("s2");
        }

        let removed = trim_session(&conn, "s1").unwrap();
        assert_eq!(removed as i64, 5, "应只裁剪超出的 5 条");

        let count = |sid: &str| -> i64 {
            conn.query_row(
                "SELECT COUNT(*) FROM tool_call_log WHERE session_id=?1",
                rusqlite::params![sid],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(count("s1"), KEEP_PER_SESSION);
        assert_eq!(count("s2"), 3, "其他会话的记录不应被误删");

        // 保留的必须是最近的:最旧行(id 最小)已被删除
        let min_id: i64 = conn
            .query_row(
                "SELECT MIN(id) FROM tool_call_log WHERE session_id='s1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(min_id, 6);
    }
}
