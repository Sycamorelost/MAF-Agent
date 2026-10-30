//! MCP 工具调用审计日志(V30 / P8.2)。
//!
//! 每次 MCP 工具调用(尤其是写入/执行类)落一条,供事后追溯 Agent 行为。
//! 轻量表:args_preview 截断到 500 字符,避免大参数撑爆行。

#![allow(dead_code)]

use crate::db;

/// 写入审计日志;静默吞掉错误(审计失败不影响主流程)。
pub fn record(
    session_id: &str,
    project_id: Option<&str>,
    tool_name: &str,
    target_path: Option<&str>,
    args_preview: Option<&str>,
    result: &str,
) {
    let conn = match db::conn().lock() {
        Ok(c) => c,
        Err(_) => return,
    };
    let preview = args_preview
        .map(|s| if s.len() > 500 { &s[..500] } else { s })
        .and_then(|s| if s.is_empty() { None } else { Some(s) });
    let _ = conn.execute(
        "INSERT INTO tool_call_log(session_id, project_id, tool_name, target_path, args_preview, result) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            session_id,
            project_id,
            tool_name,
            target_path,
            preview,
            result
        ],
    );
}

/// 按会话查最近调用(最多 200 条,按时间倒序)。
pub fn list_by_session(session_id: &str, limit: Option<i64>) -> Vec<ToolCallLog> {
    let conn = match db::conn().lock() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let limit = limit.unwrap_or(200).max(1).min(1000);
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
