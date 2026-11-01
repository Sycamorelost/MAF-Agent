//! D7:会话事件追加日志(只追加、不改写,不动既有消息表)。
//!
//! 目的:回答「模型当时到底看到了什么」。压缩、项目约定注入、网页抓取、每轮请求
//! 的指纹与 token 估算都是运行时的内存态改写,此前不留痕迹;这里把它们各自追加一条,
//! 供事后排查与成本归因。不做完整 replay,也不影响会话渲染。

#![allow(dead_code)]

use rusqlite::params;

use crate::db;

/// 单条 payload 上限(字符)。事件只存摘要,不存全文,避免把日志写成第二份数据。
const PAYLOAD_MAX_CHARS: usize = 4_000;
/// 单会话保留的事件条数上限(超出后自动裁剪最旧的)。
const KEEP_PER_SESSION: i64 = 500;

/// 追加一条事件;失败不阻断主流程,但计入诊断计数并打 warn 日志(P2)。
/// `session_id` 为空(未绑定会话)时不记录 —— 无主事件没有排查价值。
pub fn append(session_id: Option<&str>, kind: &str, payload: &str) -> Option<i64> {
    let sid = session_id?.trim();
    if sid.is_empty() || !db::is_initialized() {
        return None;
    }
    let conn = match db::conn().lock() {
        Ok(c) => c,
        Err(_) => {
            crate::diagnostics::record_failure(
                crate::diagnostics::KIND_EVENT_LOG,
                &format!("lock failed (kind={kind})"),
            );
            return None;
        }
    };
    let text = crate::mcp::safety::truncate_chars(payload, PAYLOAD_MAX_CHARS);
    if let Err(e) = conn.execute(
        "INSERT INTO session_events(session_id, kind, payload) VALUES (?1, ?2, ?3)",
        params![sid, kind, text],
    ) {
        crate::diagnostics::record_failure(
            crate::diagnostics::KIND_EVENT_LOG,
            &format!("insert failed (kind={kind}): {e}"),
        );
        return None;
    }
    let id = conn.last_insert_rowid();
    // 轻量裁剪:只清自己会话的超限旧行(每次一次索引范围删除,代价可忽略)。
    let _ = conn.execute(
        "DELETE FROM session_events WHERE session_id=?1 AND id NOT IN (
           SELECT id FROM session_events WHERE session_id=?1 ORDER BY id DESC LIMIT ?2
         )",
        params![sid, KEEP_PER_SESSION],
    );
    Some(id)
}

/// 按会话查最近事件(默认 100 条,倒序)。
pub fn list_by_session(session_id: &str, limit: Option<i64>) -> Vec<SessionEvent> {
    if !db::is_initialized() {
        return Vec::new();
    }
    let conn = match db::conn().lock() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let limit = limit.unwrap_or(100).clamp(1, 1_000);
    let mut stmt = match conn.prepare(
        "SELECT id, session_id, kind, payload, created FROM session_events \
         WHERE session_id=?1 ORDER BY id DESC LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt
        .query_map(params![session_id, limit], |r| {
            Ok(SessionEvent {
                id: r.get(0)?,
                session_id: r.get(1)?,
                kind: r.get(2)?,
                payload: r.get(3)?,
                created: r.get(4)?,
            })
        })
        .ok();
    rows.into_iter().flatten().flatten().collect()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionEvent {
    pub id: i64,
    pub session_id: String,
    /// 事件类型:request / compress / rules / web_fetch
    pub kind: String,
    /// JSON 文本(结构随 kind 而定,向前兼容:读方忽略未知字段)
    pub payload: String,
    pub created: String,
}
