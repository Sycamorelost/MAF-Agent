#![allow(dead_code)]

//! 会话 / 消息 / 分支与相关设置的 commands。

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::{messages, sessions};
use crate::memory::{self, watcher::WatcherState};

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub memory_path: String,
}

// ---------- session commands ----------

/// 创建新会话:生成 8 位 hex id、建目录、写 memory.md 骨架、插入 sessions 表。
/// agent_id 非空时绑定 Agent 模板并同步记忆注入策略(迭代 B2);模板不存在则报错且不建会话。
#[tauri::command]
pub async fn create_session(
    title: String,
    model_id: String,
    agent_id: Option<String>,
) -> Result<SessionInfo, String> {
    // 先校验模板存在,避免会话建了一半才失败
    let tpl = match &agent_id {
        Some(aid) => {
            Some(crate::agents::find(aid).ok_or_else(|| format!("Agent 模板不存在:{aid}"))?)
        }
        None => None,
    };
    let (id, path) = memory::create_session(&title, &model_id)?;
    if let (Some(aid), Some(t)) = (&agent_id, &tpl) {
        sessions::set_agent(&id, Some(aid))?;
        sessions::set_memory_strategy(&id, &t.memory_strategy)?;
    }
    // P5 / D19:创建时钉住生效预设(Agent 模板默认 > 全局默认)与当前清单版本,
    // 老会话不随内置预设升级漂移;id 无效(如设置被手工改坏)则不钉,运行时回落 standard。
    let pin = tpl
        .and_then(|t| t.preset)
        .unwrap_or_else(|| crate::settings::load().default_preset);
    if crate::presets::get(&crate::paths::data_dir(), &pin).is_some() {
        sessions::set_preset(&id, Some((&pin, crate::presets::PRESETS_VERSION)))?;
    }
    Ok(SessionInfo {
        id,
        memory_path: path.to_string_lossy().to_string(),
    })
}

/// 列出所有会话,按最近更新排序。
#[tauri::command]
pub async fn list_sessions() -> Result<Vec<sessions::SessionRow>, String> {
    sessions::list()
}

/// 获取单个会话元数据。
#[tauri::command]
pub async fn get_session(id: String) -> Result<Option<sessions::SessionRow>, String> {
    sessions::get(&id)
}

/// 更新会话级权限档覆盖(V25 / D11):inherit / ask / workspace / full。
#[tauri::command]
pub async fn set_session_permission_mode(id: String, mode: String) -> Result<(), String> {
    sessions::set_permission_mode(&id, &mode)
}

/// 读取会话任务清单(harness todo 面板初始拉取;后续变更由 `session-todos-{id}` 事件推送)。
#[tauri::command]
pub async fn session_get_todos(session_id: String) -> Result<Vec<crate::db::todos::TodoItem>, String> {
    let conn = crate::db::conn()
        .lock()
        .map_err(|e| format!("db lock 失败: {e}"))?;
    crate::db::todos::list(&conn, &session_id)
}

/// 改会话标题(同时同步 memory.md frontmatter 的 conversation)。
#[tauri::command]
pub async fn rename_session(id: String, title: String) -> Result<(), String> {
    sessions::update_title(&id, &title)?;
    let _ = memory::sync_title(&id, &title);
    Ok(())
}

/// 克隆会话:新建「副本-{原标题}」,复制 project_id / settings / 全部消息。
/// 不复制 branches(保持线性新会话),不复制 tool_call_log(新会话会重新记录)。
#[tauri::command]
pub async fn clone_session(id: String) -> Result<String, String> {
    let orig = sessions::get(&id)?
        .ok_or_else(|| format!("源会话不存在:{id}"))?;
    let new_title = format!("副本-{}", orig.title);
    let model_id = orig.model_id.clone().unwrap_or_default();
    let agent_id = orig.agent_id.clone();
    let SessionInfo { id: new_id, memory_path: _ } =
        create_session(new_title, model_id, agent_id).await?;
    // 复制 project_id
    if let Some(pid) = orig.project_id.as_deref() {
        sessions::set_project(&new_id, Some(pid))?;
    }
    // 复制 settings(仅在源值非默认时覆盖,create_session 已设了默认 preset/strategy)
    if let Some(sp) = orig.system_prompt.as_deref() {
        sessions::set_system_prompt(&new_id, Some(sp))?;
    }
    if orig.web_search_enabled {
        sessions::set_web_search(&new_id, true)?;
    }
    if !orig.memory_injection_strategy.is_empty() {
        sessions::set_memory_strategy(&new_id, &orig.memory_injection_strategy)?;
    }
    if !orig.workbench_mode.is_empty() {
        sessions::set_workbench_mode(&new_id, &orig.workbench_mode)?;
    }
    if !orig.permission_mode.is_empty() {
        sessions::set_permission_mode(&new_id, &orig.permission_mode)?;
    }
    // 复制全部消息(保留 role / content / branch_id=active / 清空 tool_call_id 避免冲突)
    let conn = crate::db::conn()
        .lock()
        .map_err(|e| format!("db lock 失败: {e}"))?;
    let active_branch = sessions::active_branch_on(&conn, &new_id)?;
    let orig_active = sessions::active_branch_on(&conn, &id)?;
    crate::db::messages::copy_active_branch(&conn, &id, &new_id, &orig_active, &active_branch)?;
    Ok(new_id)
}

/// 导出会话文件:.md / .html 两种纯文本格式。
/// 内容由前端按当前分支视图组装,后端只做路径/令牌校验与写盘。
/// 扩展名取自令牌路径(另存为对话框已让用户选定),仅放行 md / html。
/// 与研究报告导出同一套安全模型(N-6 令牌 + H-2 路径规范与覆盖校验)。
#[tauri::command]
pub async fn session_export_file(
    id: String,
    content: String,
    token: String,
    overwrite: bool,
    tokens: State<'_, crate::export::ExportPathTokens>,
) -> Result<String, String> {
    memory::validate_id(&id)?;
    // 会话必须存在,避免向孤儿 id 写文件
    sessions::get(&id)?.ok_or("会话不存在")?;
    let raw = tokens.resolve(&token)?.to_string_lossy().into_owned();
    let ext = std::path::Path::new(&raw)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ext != "md" && ext != "html" {
        return Err("会话导出仅支持 .md / .html".into());
    }
    let target = crate::export::ensure_export_path(&raw, &ext)?;
    crate::export::ensure_writable(&target, overwrite)?;
    std::fs::write(&target, content).map_err(|e| format!("写入导出文件失败: {e}"))?;
    Ok(target.to_string_lossy().into_owned())
}

/// 改会话当前模型。
#[tauri::command]
pub async fn set_session_model(id: String, model_id: String) -> Result<(), String> {
    sessions::update_model(&id, &model_id)
}

/// 删除会话:删 db 记录(CASCADE 删 messages/memory_versions)+ 删磁盘目录。
#[tauri::command]
pub async fn delete_session(id: String, state: State<'_, WatcherState>) -> Result<(), String> {
    memory::validate_id(&id)?;
    // N-1:先解除 memory watcher 再删目录,避免孤儿 watcher/去抖线程残留(及 Windows 句柄占用)
    let _ = state.registry.unwatch(&id);
    sessions::delete(&id)?;
    let dir = memory::session_dir(&id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| format!("remove {} failed: {e}", dir.display()))?;
    }
    Ok(())
}

/// 仅当会话没有任何消息时删除(空对话默认不保留),返回是否已删。
#[tauri::command]
pub async fn delete_session_if_empty(id: String, state: State<'_, WatcherState>) -> Result<bool, String> {
    memory::validate_id(&id)?;
    let deleted = sessions::delete_if_empty(&id)?;
    if deleted {
        let _ = state.registry.unwatch(&id);
        let dir = memory::session_dir(&id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .map_err(|e| format!("remove {} failed: {e}", dir.display()))?;
        }
    }
    Ok(deleted)
}

/// 批量清理空会话(0 条消息且 updated 早于 grace_secs 秒前),返回被删 id 列表。
#[tauri::command]
pub async fn prune_empty_sessions(
    grace_secs: Option<i64>,
    state: State<'_, WatcherState>,
) -> Result<Vec<String>, String> {
    let ids = sessions::prune_empty(grace_secs.unwrap_or(300))?;
    // prune 内部 WHERE updated < cutoff 只查库,返回的 id 由数据库保证合法,
    // 但清空磁盘目录前统一校验,防御纵深
    for id in &ids {
        memory::validate_id(id)?;
    }
    for id in &ids {
        let _ = state.registry.unwatch(id);
        let dir = memory::session_dir(id);
        if dir.exists() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    Ok(ids)
}

// ---------- message commands ----------

#[derive(Debug, Deserialize)]
pub struct AppendMessageInput {
    pub session_id: String,
    pub role: String, // user / assistant / system
    pub content: String,
    pub model_id: Option<String>,
    #[serde(default)]
    pub tokens: i64,
    /// 附件元数据 JSON 数组字符串(无附件为 None)
    pub attachments: Option<String>,
    /// 联网来源 JSON 数组字符串(无联网为 None)
    pub sources: Option<String>,
    /// 思考过程文本(V14;assistant 消息可选,默认空串)
    #[serde(default)]
    pub reasoning: Option<String>,
}

/// 追加一条消息(写入会话当前激活分支)。
#[tauri::command]
pub async fn append_message(input: AppendMessageInput) -> Result<i64, String> {
    let branch = sessions::active_branch(&input.session_id)?;
    messages::append(
        &input.session_id,
        &input.role,
        &input.content,
        input.model_id.as_deref(),
        input.tokens,
        input.attachments.as_deref(),
        input.sources.as_deref(),
        input.reasoning.as_deref(),
        &branch,
    )
}

/// 列出分支可见的全部消息(branch_id 缺省 = 会话当前激活分支)。
#[tauri::command]
pub async fn list_messages(
    session_id: String,
    branch_id: Option<String>,
) -> Result<Vec<messages::MessageRow>, String> {
    let branch = match branch_id {
        Some(b) if !b.trim().is_empty() => b,
        _ => sessions::active_branch(&session_id)?,
    };
    messages::list(&session_id, &branch)
}

/// 分页消息(默认每页 100 条;`before_id` 用于「加载更早」)。
#[derive(serde::Serialize)]
pub struct MessagePage {
    pub messages: Vec<messages::MessageRow>,
    pub has_more: bool,
}

#[tauri::command]
pub async fn list_messages_page(
    session_id: String,
    branch_id: Option<String>,
    before_id: Option<i64>,
    limit: Option<usize>,
) -> Result<MessagePage, String> {
    let branch = match branch_id {
        Some(b) if !b.trim().is_empty() => b,
        _ => sessions::active_branch(&session_id)?,
    };
    let (msgs, has_more) =
        messages::list_page(&session_id, &branch, before_id, limit.unwrap_or(100))?;
    Ok(MessagePage {
        messages: msgs,
        has_more,
    })
}

/// 清空当前激活分支的自有消息(其他分支不受影响)。
#[tauri::command]
pub async fn clear_messages(session_id: String) -> Result<(), String> {
    let branch = sessions::active_branch(&session_id)?;
    messages::clear(&session_id, &branch)
}

/// 删除单条消息(链上子消息自动重挂到其父消息)。
#[tauri::command]
pub async fn delete_message(id: i64) -> Result<(), String> {
    messages::delete_one(id)
}

/// 删除当前激活分支中 id >= from_id 的全部自有消息(放弃分支尾部),返回删除条数。
#[tauri::command]
pub async fn delete_messages_from(session_id: String, from_id: i64) -> Result<u64, String> {
    let branch = sessions::active_branch(&session_id)?;
    messages::delete_from(&session_id, &branch, from_id)
}

// ---------- 会话分支树(B6 / F4) ----------

/// 列出会话全部分支(自动确保 main 存在,清理无消息的非激活空 fork)。
#[tauri::command]
pub async fn list_branches(session_id: String) -> Result<Vec<crate::db::branches::BranchInfo>, String> {
    memory::validate_id(&session_id)?;
    crate::db::branches::list(&session_id)
}

/// 从指定消息处分叉并切换激活分支,返回新 branch_id。
/// 语义:新分支替换目标消息及其后续(目标消息的 parent 成为新分支分叉点),旧分支完整保留。
#[tauri::command]
pub async fn fork_branch(session_id: String, from_message_id: i64) -> Result<String, String> {
    memory::validate_id(&session_id)?;
    let id = crate::db::branches::create(&session_id, from_message_id, None)?;
    sessions::set_active_branch(&session_id, &id)?;
    Ok(id)
}

/// 切换激活分支。
#[tauri::command]
pub async fn set_active_branch(session_id: String, branch_id: String) -> Result<(), String> {
    memory::validate_id(&session_id)?;
    sessions::set_active_branch(&session_id, &branch_id)
}

/// 重命名分支(空名 = 恢复默认显示名)。
#[tauri::command]
pub async fn rename_branch(session_id: String, branch_id: String, name: String) -> Result<(), String> {
    memory::validate_id(&session_id)?;
    crate::db::branches::rename(&session_id, &branch_id, &name)
}

/// 删除分支(main 不可删;若删除的是激活分支,自动回退 main)。
#[tauri::command]
pub async fn delete_branch(session_id: String, branch_id: String) -> Result<(), String> {
    memory::validate_id(&session_id)?;
    crate::db::branches::delete(&session_id, &branch_id)
}

/// 更新消息内容(编辑)。
#[tauri::command]
pub async fn update_message(id: i64, content: String) -> Result<(), String> {
    messages::update_content(id, &content)
}

/// 更新会话级设置(系统提示词 / 上下文滑窗)。system_prompt 空字符串 = 清除。
#[tauri::command]
pub async fn update_session_settings(
    id: String,
    system_prompt: Option<String>,
    max_context_messages: i64,
) -> Result<(), String> {
    sessions::update_settings(&id, system_prompt.as_deref(), max_context_messages)
}

/// 更新会话的对话内联网开关(V5)。
#[tauri::command]
pub async fn set_session_web_search(id: String, enabled: bool) -> Result<(), String> {
    sessions::set_web_search(&id, enabled)
}

/// 更新切换模型时的记忆注入策略(V6,迭代 D3.1):full / summary / hybrid / none。
#[tauri::command]
pub async fn set_session_memory_strategy(id: String, strategy: String) -> Result<(), String> {
    sessions::set_memory_strategy(&id, &strategy)
}

/// 更新会话级工作台模式覆盖(V24 / D16):inherit / chat / work。
#[tauri::command]
pub async fn set_session_workbench_mode(id: String, mode: String) -> Result<(), String> {
    sessions::set_workbench_mode(&id, &mode)
}
