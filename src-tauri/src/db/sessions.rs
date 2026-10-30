#![allow(dead_code)]

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
    /// 会话级工作台模式覆盖(V24 / D16):inherit = 跟随全局默认;chat / work = 强制
    pub workbench_mode: String,
    /// 会话级权限档覆盖(V25 / D11):inherit = 跟随全局审批策略;ask / workspace / full = 强制
    pub permission_mode: String,
    /// 会话级运行时预设钉住(V26 / D19):None = 继承(Agent 模板默认 → 全局默认)
    pub preset_id: Option<String>,
    /// 钉住时的内置预设清单版本;未钉住为 None
    pub preset_version: Option<i64>,
    /// 所属文件夹(V28 / P8):None = 未分类
    pub project_id: Option<String>,
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
        workbench_mode: r.get(12)?,
        permission_mode: r.get(13)?,
        preset_id: r.get(14)?,
        preset_version: r.get(15)?,
        project_id: r.get(16)?,
    })
}

const SELECT_COLS: &str = "id, title, created, updated, model_id, memory_version, system_prompt, max_context_messages, web_search_enabled, memory_injection_strategy, agent_id, active_branch, workbench_mode, permission_mode, preset_id, preset_version, project_id";

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

/// 合法的工作台模式覆盖值(V24 / D16)。
pub const WORKBENCH_MODES: [&str; 3] = ["inherit", "chat", "work"];

/// 更新会话级工作台模式覆盖(V24 / D16):inherit / chat / work。非法值报错。
pub fn set_workbench_mode(id: &str, mode: &str) -> Result<(), String> {
    if !WORKBENCH_MODES.contains(&mode) {
        return Err(format!(
            "invalid workbench_mode: {mode}(可选:{})",
            WORKBENCH_MODES.join("/")
        ));
    }
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sessions SET workbench_mode=?1 WHERE id=?2",
        rusqlite::params![mode, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 合法的权限档覆盖值(V25 / D11):inherit = 跟随全局 mcp-approval.json。
pub const PERMISSION_MODES: [&str; 4] = ["inherit", "ask", "workspace", "full"];

/// 更新会话级权限档覆盖(V25 / D11)。非法值报错。
pub fn set_permission_mode(id: &str, mode: &str) -> Result<(), String> {
    if !PERMISSION_MODES.contains(&mode) {
        return Err(format!(
            "invalid permission_mode: {mode}(可选:{})",
            PERMISSION_MODES.join("/")
        ));
    }
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sessions SET permission_mode=?1 WHERE id=?2",
        rusqlite::params![mode, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 读取会话级权限档覆盖(供工具调度器逐调用查询);会话不存在或 inherit 都返回 None。
pub fn permission_mode_override(id: &str) -> Option<String> {
    let conn = db::conn().lock().ok()?;
    let mode: String = conn
        .query_row(
            "SELECT permission_mode FROM sessions WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .ok()?;
    (mode != "inherit").then_some(mode)
}

/// 钉住/解除会话级运行时预设(V26 / D19)。
/// Some(id, version) = 钉住;None = 恢复继承(版本一并清空)。
pub fn set_preset(id: &str, preset: Option<(&str, i64)>) -> Result<(), String> {
    let (pid, ver): (Option<&str>, Option<i64>) = match preset {
        Some((p, v)) => (Some(p), Some(v)),
        None => (None, None),
    };
    let conn = db::conn().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sessions SET preset_id=?1, preset_version=?2 WHERE id=?3",
        rusqlite::params![pid, ver, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 把会话归入指定项目(V29 / P8.1)。None / 空串 = 移出项目(归入未分类)。
/// 绑定成功后自动:有 project → 注入 guard prompt;无 project → 移除旧 guard。
#[allow(dead_code)]
pub fn set_project(id: &str, project_id: Option<&str>) -> Result<(), String> {
    let pid = project_id.and_then(|s| if s.is_empty() { None } else { Some(s) });
    // 先在独立作用域里完成 UPDATE,释放锁后再调 guard 函数(它们内部也会 lock)
    {
        let conn = db::conn().lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE sessions SET project_id=?1, updated=?2 WHERE id=?3",
            rusqlite::params![pid, chrono::Local::now().to_rfc3339(), id],
        )
        .map_err(|e| e.to_string())?;
    }
    // 提示词 guard 注入 / 清除
    match pid {
        Some(pid) => {
            if let Some(proj) = crate::db::projects::get(pid)? {
                // 项目必须是有效目录才注入(项目可能被删但引用未解绑)
                if std::path::Path::new(&proj.path).is_dir() {
                    let _ = crate::db::projects::append_project_guard(id, &proj);
                }
            }
        }
        None => {
            let _ = crate::db::projects::remove_project_guard(id);
        }
    }
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
    // 先清 FTS(messages/doc_chunks 由外键级联删除,FTS 需手动同步)
    let _ = conn.execute(
        "DELETE FROM messages_fts WHERE rowid IN (SELECT id FROM messages WHERE session_id=?1)",
        [id],
    );
    let _ = conn.execute(
        "DELETE FROM doc_chunks_fts WHERE rowid IN (SELECT id FROM doc_chunks WHERE session_id=?1)",
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
