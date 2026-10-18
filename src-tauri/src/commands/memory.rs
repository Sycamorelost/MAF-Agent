//! memory / session / message 相关 Tauri commands。

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::{messages, sessions};
use crate::memory::{self, watcher::WatcherState};

// ---------- DTO ----------

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub memory_path: String,
}

#[derive(Debug, Serialize)]
pub struct MemoryDto {
    pub frontmatter: memory::frontmatter::Frontmatter,
    pub body: String,
}

#[derive(Debug, Serialize)]
pub struct VersionItem {
    pub id: i64,
    pub created: String,
    pub note: String,
}

// ---------- session commands ----------

/// 创建新会话:生成 8 位 hex id、建目录、写 memory.md 骨架、插入 sessions 表。
/// agent_id 非空时绑定 Agent 模板并同步记忆注入策略(迭代 B2);模板不存在则报错且不建会话。
#[tauri::command]
pub fn create_session(
    title: String,
    model_id: String,
    agent_id: Option<String>,
) -> Result<SessionInfo, String> {
    // 先校验模板存在,避免会话建了一半才失败
    let strategy = match &agent_id {
        Some(aid) => {
            let tpl =
                crate::agents::find(aid).ok_or_else(|| format!("Agent 模板不存在:{aid}"))?;
            Some(tpl.memory_strategy)
        }
        None => None,
    };
    let (id, path) = memory::create_session(&title, &model_id)?;
    if let Some(aid) = &agent_id {
        sessions::set_agent(&id, Some(aid))?;
        if let Some(s) = strategy {
            sessions::set_memory_strategy(&id, &s)?;
        }
    }
    Ok(SessionInfo {
        id,
        memory_path: path.to_string_lossy().to_string(),
    })
}

/// 列出所有会话,按最近更新排序。
#[tauri::command]
pub fn list_sessions() -> Result<Vec<sessions::SessionRow>, String> {
    sessions::list()
}

/// 获取单个会话元数据。
#[tauri::command]
pub fn get_session(id: String) -> Result<Option<sessions::SessionRow>, String> {
    sessions::get(&id)
}

/// 改会话标题(同时同步 memory.md frontmatter 的 conversation)。
#[tauri::command]
pub fn rename_session(id: String, title: String) -> Result<(), String> {
    sessions::update_title(&id, &title)?;
    let _ = memory::sync_title(&id, &title);
    Ok(())
}

/// 改会话当前模型。
#[tauri::command]
pub fn set_session_model(id: String, model_id: String) -> Result<(), String> {
    sessions::update_model(&id, &model_id)
}

/// 删除会话:删 db 记录(CASCADE 删 messages/memory_versions)+ 删磁盘目录。
#[tauri::command]
pub fn delete_session(id: String, state: State<'_, WatcherState>) -> Result<(), String> {
    memory::validate_id(&id)?;
    // N-1:先解除 memory watcher 再删目录,避免孤儿 watcher/去抖线程残留(及 Windows 句柄占用)
    let _ = state.registry.unwatch(&id);
    sessions::delete(&id)?;
    let dir = memory::session_dir(&id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("remove {} failed: {e}", dir.display()))?;
    }
    Ok(())
}

/// 仅当会话没有任何消息时删除(空对话默认不保留),返回是否已删。
#[tauri::command]
pub fn delete_session_if_empty(id: String, state: State<'_, WatcherState>) -> Result<bool, String> {
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
pub fn prune_empty_sessions(
    grace_secs: Option<i64>,
    state: State<'_, WatcherState>,
) -> Result<Vec<String>, String> {
    let ids = sessions::prune_empty(grace_secs.unwrap_or(300))?;
    // prune 内部 WHERE updated < cutoff 只查库,返回的 id 由数据库保证合法,
    // 但清空磁盘目录前统一校验,防御纵深
    for id in &ids { memory::validate_id(id)?; }
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
    pub role: String,               // user / assistant / system
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
pub fn append_message(input: AppendMessageInput) -> Result<i64, String> {
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
pub fn list_messages(
    session_id: String,
    branch_id: Option<String>,
) -> Result<Vec<messages::MessageRow>, String> {
    let branch = match branch_id {
        Some(b) if !b.trim().is_empty() => b,
        _ => sessions::active_branch(&session_id)?,
    };
    messages::list(&session_id, &branch)
}

/// 清空当前激活分支的自有消息(其他分支不受影响)。
#[tauri::command]
pub fn clear_messages(session_id: String) -> Result<(), String> {
    let branch = sessions::active_branch(&session_id)?;
    messages::clear(&session_id, &branch)
}

/// 删除单条消息(链上子消息自动重挂到其父消息)。
#[tauri::command]
pub fn delete_message(id: i64) -> Result<(), String> {
    messages::delete_one(id)
}

/// 删除当前激活分支中 id >= from_id 的全部自有消息(放弃分支尾部),返回删除条数。
#[tauri::command]
pub fn delete_messages_from(session_id: String, from_id: i64) -> Result<u64, String> {
    let branch = sessions::active_branch(&session_id)?;
    messages::delete_from(&session_id, &branch, from_id)
}

// ---------- 会话分支树(B6 / F4) ----------

/// 列出会话全部分支(自动确保 main 存在,清理无消息的非激活空 fork)。
#[tauri::command]
pub fn list_branches(session_id: String) -> Result<Vec<crate::db::branches::BranchInfo>, String> {
    memory::validate_id(&session_id)?;
    crate::db::branches::list(&session_id)
}

/// 从指定消息处分叉并切换激活分支,返回新 branch_id。
/// 语义:新分支替换目标消息及其后续(目标消息的 parent 成为新分支分叉点),旧分支完整保留。
#[tauri::command]
pub fn fork_branch(session_id: String, from_message_id: i64) -> Result<String, String> {
    memory::validate_id(&session_id)?;
    let id = crate::db::branches::create(&session_id, from_message_id, None)?;
    sessions::set_active_branch(&session_id, &id)?;
    Ok(id)
}

/// 切换激活分支。
#[tauri::command]
pub fn set_active_branch(session_id: String, branch_id: String) -> Result<(), String> {
    memory::validate_id(&session_id)?;
    sessions::set_active_branch(&session_id, &branch_id)
}

/// 重命名分支(空名 = 恢复默认显示名)。
#[tauri::command]
pub fn rename_branch(session_id: String, branch_id: String, name: String) -> Result<(), String> {
    memory::validate_id(&session_id)?;
    crate::db::branches::rename(&session_id, &branch_id, &name)
}

/// 删除分支(main 不可删;若删除的是激活分支,自动回退 main)。
#[tauri::command]
pub fn delete_branch(session_id: String, branch_id: String) -> Result<(), String> {
    memory::validate_id(&session_id)?;
    crate::db::branches::delete(&session_id, &branch_id)
}

/// 更新消息内容(编辑)。
#[tauri::command]
pub fn update_message(id: i64, content: String) -> Result<(), String> {
    messages::update_content(id, &content)
}

/// 更新会话级设置(系统提示词 / 上下文滑窗)。system_prompt 空字符串 = 清除。
#[tauri::command]
pub fn update_session_settings(
    id: String,
    system_prompt: Option<String>,
    max_context_messages: i64,
) -> Result<(), String> {
    sessions::update_settings(&id, system_prompt.as_deref(), max_context_messages)
}

/// 更新会话的对话内联网开关(V5)。
#[tauri::command]
pub fn set_session_web_search(id: String, enabled: bool) -> Result<(), String> {
    sessions::set_web_search(&id, enabled)
}

/// 更新切换模型时的记忆注入策略(V6,迭代 D3.1):full / summary / hybrid / none。
#[tauri::command]
pub fn set_session_memory_strategy(id: String, strategy: String) -> Result<(), String> {
    sessions::set_memory_strategy(&id, &strategy)
}

// ---------- memory commands ----------

/// 读取 memory.md(frontmatter + body)。
#[tauri::command]
pub fn read_memory(session_id: String) -> Result<MemoryDto, String> {
    let (fm, body) = memory::read_memory(&session_id)?;
    Ok(MemoryDto {
        frontmatter: fm,
        body,
    })
}

/// 写入 memory.md(自动快照旧版到 memory.versions/ 并写表)。
#[tauri::command]
pub fn write_memory(session_id: String, content: String, note: Option<String>) -> Result<(), String> {
    memory::write_memory(&session_id, &content, note.as_deref().unwrap_or("manual"))
}

/// 追加文本到 memory.md 指定节(缺省「资料」,节名限定 KNOWN_SECTIONS 白名单;自动补标题)。
/// note 统一记为 pin_message(消息钉入 / 来源存入 / 研究报告存入均属钉入行为)。
#[tauri::command]
pub fn append_memory(session_id: String, text: String, section: Option<String>) -> Result<(), String> {
    let sec = resolve_section(section.as_deref())?;
    let (fm, body) = memory::read_memory(&session_id)?;
    let new_body = if body.trim().is_empty() {
        format!("# {sec}\n\n{}", text.trim())
    } else {
        append_to_section(&body, sec, &text)
    };
    memory::write_memory(&session_id, &memory::frontmatter::render(&fm, &new_body)?, "pin_message")
}

/// 解析目标节名:空白缺省「资料」;非 KNOWN_SECTIONS 白名单直接报错。
fn resolve_section(section: Option<&str>) -> Result<&str, String> {
    let sec = section.map(str::trim).filter(|s| !s.is_empty()).unwrap_or("资料");
    if memory::template::KNOWN_SECTIONS.contains(&sec) {
        Ok(sec)
    } else {
        Err(format!("未知记忆节: {sec}(仅支持 6 个标准节)"))
    }
}

/// 把 text 追加到 body 中第一个以 `# heading` 开头的一级/二级/三级标题下方。
/// 若不存在该标题则在末尾追加 `\n\n# heading\n\ntext`。
fn append_to_section(body: &str, heading: &str, text: &str) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut insert_idx = lines.len();
    let mut found = false;
    for (i, line) in lines.iter().enumerate() {
        if line.trim() == format!("# {}", heading)
            || line.trim() == format!("## {}", heading)
            || line.trim() == format!("### {}", heading)
        {
            insert_idx = i + 1;
            found = true;
        } else if found {
            // 找到下一节标题行时,在该行之前插入
            let trimmed = line.trim_start();
            if trimmed.starts_with("# ")
                || trimmed.starts_with("## ")
                || trimmed.starts_with("### ")
            {
                insert_idx = i;
                break;
            }
        }
    }
    if !found {
        // 标题不存在,直接追加
        let sep = if body.ends_with('\n') { "" } else { "\n\n" };
        return format!("{}{}# {}\n\n{}", body, sep, heading, text.trim());
    }
    let mut out = lines[..insert_idx].join("\n");
    out.push_str(&format!("\n\n{}", text.trim()));
    if insert_idx < lines.len() {
        out.push('\n');
        out.push_str(&lines[insert_idx..].join("\n"));
    }
    out
}

/// 手动触发一次版本快照。
#[tauri::command]
pub fn snapshot_memory(session_id: String, note: Option<String>) -> Result<String, String> {
    let p = memory::snapshot_version(&session_id, note.as_deref().unwrap_or("manual"))?;
    Ok(p.to_string_lossy().to_string())
}

/// 列出会话的历史版本(倒序)。
#[tauri::command]
pub fn list_memory_versions(session_id: String) -> Result<Vec<VersionItem>, String> {
    let rows = memory::list_versions(&session_id)?;
    Ok(rows
        .into_iter()
        .map(|(id, created, note)| VersionItem { id, created, note })
        .collect())
}

/// 回滚到指定版本。
#[tauri::command]
pub fn rollback_memory(session_id: String, version_id: i64) -> Result<(), String> {
    memory::rollback_to(&session_id, version_id)
}

/// 读取指定历史版本的完整快照内容(迭代 D3.3 版本 diff 用)。
#[tauri::command]
pub fn read_memory_version(session_id: String, version_id: i64) -> Result<String, String> {
    memory::read_version(&session_id, version_id)
}

/// 记忆注入预算状态(迭代 D3.5):当前正文估算 tokens / 预算上限 / 是否超限。
#[derive(Debug, Serialize)]
pub struct BudgetStatus {
    pub tokens: usize,
    pub limit: usize,
    pub over: bool,
}

#[tauri::command]
pub fn memory_budget_status(session_id: String) -> Result<BudgetStatus, String> {
    let (_fm, body) = memory::read_memory(&session_id)?;
    let tokens = memory::budget::estimate_tokens(&body);
    Ok(BudgetStatus {
        tokens,
        limit: memory::budget::MEMORY_BUDGET_TOKENS,
        over: tokens > memory::budget::MEMORY_BUDGET_TOKENS,
    })
}

/// 开始监听 memory.md 外部改动(前端拿到 `memory_changed` 事件)。
#[tauri::command]
pub fn watch_memory(
    session_id: String,
    app: tauri::AppHandle,
    state: State<'_, WatcherState>,
) -> Result<(), String> {
    memory::validate_id(&session_id)?;
    state.registry.watch(app, &session_id)
}

/// 停止监听。
#[tauri::command]
pub fn unwatch_memory(session_id: String, state: State<'_, WatcherState>) -> Result<(), String> {
    memory::validate_id(&session_id)?;
    state.registry.unwatch(&session_id)
}

/// 判断是否需要提炼(规则:距上次 N 轮 / token 超阈值)。
#[tauri::command]
pub fn needs_summarize(
    session_id: String,
    every_n_rounds: Option<i64>,
    token_threshold: Option<i64>,
) -> Result<bool, String> {
    memory::needs_summarize(
        &session_id,
        every_n_rounds.unwrap_or(2),
        token_threshold.unwrap_or(8000),
    )
}

// ---------- summarize ----------

/// 提炼器系统提示(对应 idea/03-memory-flow.md §2.2 增量提炼)。
const SUMMARIZE_SYSTEM: &str = r#"你是 MAF Agent 的记忆提炼器。根据「现有 memory.md 正文」和「新增对话消息」,输出**更新后的完整 memory.md 正文**。

规则:
1. 只输出 Markdown 正文本身——不要 frontmatter(--- 标记)、不要代码块包裹、不要任何解释性前言。
2. 默认维护以下已知章节(没有的可以新增,空的可以省略):# 会话摘要 / # 关键事实与结论 / # 用户偏好与约束 / # 待办 / 未决问题 / # 文档摘要 / # 资料
3. 现有正文中的用户自定义章节必须原样保留,不得删除或改写。
4. 「会话摘要」3-6 句滚动更新;「关键事实与结论」用列表;「待办 / 未决问题」用 - [ ] 复选框。
5. 只记录长期有价值的信息(事实、结论、偏好、约束、待办);寒暄、闲聊、一次性细节不记录。
6. 总长度控制在 2000 tokens 以内;逼近上限时优先压缩低价值细节,保留事实与待办。"#;

#[derive(Debug, Deserialize)]
pub struct SummarizeMemoryInput {
    pub session_id: String,
    pub provider_id: String,
    pub model: String,
    pub custom_base_url: Option<String>,
    pub custom_display_name: Option<String>,
}

/// 调 LLM 提炼记忆:读现有 memory.md + 最近消息 → 返回更新后的完整 memory.md(含 frontmatter)。
/// 注意:本命令只生成内容,不落盘;由前端拿到结果后再调 write_memory(note="auto_summarize")。
#[tauri::command]
pub async fn summarize_memory(input: SummarizeMemoryInput) -> Result<String, String> {
    use futures::StreamExt;

    // 1. 读现有 memory.md
    let (fm, body) = memory::read_memory(&input.session_id)?;

    // 2. 最近 50 条消息
    let recent = messages::recent(&input.session_id, 50)?;
    if recent.is_empty() {
        return Err("会话无消息,无需提炼".into());
    }
    let history_text = recent
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n\n");

    // 3. 组请求
    let user = format!(
        "【现有 memory.md 正文】\n{}\n\n【新增对话消息】\n{}",
        body.trim(),
        history_text
    );
    let provider = crate::commands::provider::build_provider(
        &input.provider_id,
        &input.model,
        input.custom_base_url.as_deref(),
        input.custom_display_name.as_deref(),
    )?;
    let req = crate::provider::ChatRequest {
        model: input.model.clone(),
        messages: vec![
            crate::provider::ChatMessage {
                role: "system".into(),
                content: SUMMARIZE_SYSTEM.into(),
                images: vec![],
            },
            crate::provider::ChatMessage {
                role: "user".into(),
                content: user,
                images: vec![],
            },
        ],
        temperature: Some(0.3),
        max_tokens: None,
        top_p: None,
        reasoning: None, // 记忆提炼走自有口径,不启用思考
        stream: true,
    };

    // 4. 流式收全量文本
    let mut stream = provider
        .chat_stream(req)
        .await
        .map_err(|e| e.to_string())?;
    let mut out = String::new();
    while let Some(chunk) = stream.next().await {
        match chunk {
            crate::provider::StreamChunk::Delta { content } => out.push_str(&content),
            crate::provider::StreamChunk::Reasoning { .. } => {} // 提炼只取正文,忽略思考流
            crate::provider::StreamChunk::Error { message } => return Err(message),
            crate::provider::StreamChunk::Usage { .. } => {}
            crate::provider::StreamChunk::Done { .. } => break,
        }
    }

    // 5. 清洗输出:去代码块包裹 / 误带的 frontmatter
    let new_body = clean_summarized_body(&out);
    if new_body.trim().is_empty() {
        return Err("提炼结果为空".into());
    }

    // 6. 用现有 frontmatter 重新渲染完整文件内容
    memory::frontmatter::render(&fm, &new_body)
}

/// 清洗 LLM 输出:去掉 ```markdown 包裹和误带的 frontmatter,只留正文。
fn clean_summarized_body(raw: &str) -> String {
    let mut text = raw.trim().to_string();

    // 去整段代码块包裹 ```markdown ... ```
    if text.starts_with("```") {
        if let Some(first_nl) = text.find('\n') {
            text = text[first_nl + 1..].to_string();
        }
        if let Some(stripped) = text.strip_suffix("```") {
            text = stripped.trim_end().to_string();
        }
    }

    // 去误带的 frontmatter(--- ... ---)
    if text.starts_with("---") {
        if let Ok((_fm, body)) = memory::frontmatter::parse(&text) {
            if !body.trim().is_empty() {
                text = body;
            }
        }
    }

    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::{append_to_section, resolve_section};

    #[test]
    fn resolve_section_defaults_to_ziliao() {
        assert_eq!(resolve_section(None).unwrap(), "资料");
        assert_eq!(resolve_section(Some("")).unwrap(), "资料");
        assert_eq!(resolve_section(Some("  ")).unwrap(), "资料");
    }

    #[test]
    fn resolve_section_accepts_all_known_sections() {
        for sec in crate::memory::template::KNOWN_SECTIONS {
            assert_eq!(resolve_section(Some(sec)).unwrap(), sec);
        }
    }

    #[test]
    fn resolve_section_rejects_unknown() {
        assert!(resolve_section(Some("不存在的节")).is_err());
        assert!(resolve_section(Some("资料; DROP TABLE")).is_err());
    }

    #[test]
    fn append_to_section_creates_heading_when_missing() {
        let out = append_to_section("# 会话摘要\n\nhello", "资料", "- [a](http://x)");
        assert!(out.contains("# 会话摘要"), "原内容保留");
        assert!(out.contains("# 资料\n\n- [a](http://x)"), "自动补标题");
    }

    #[test]
    fn append_to_section_inserts_under_existing_heading() {
        let body = "# 资料\n\n- 旧条目\n\n# 待办\n\n- [ ] x";
        let out = append_to_section(body, "资料", "- 新条目");
        let zi = out.find("- 新条目").unwrap();
        let todo = out.find("# 待办").unwrap();
        assert!(zi < todo, "新条目应插在「待办」之前");
        assert!(out.contains("- 旧条目"), "旧条目保留");
    }

    #[test]
    fn append_to_section_at_end_when_no_following_heading() {
        let body = "# 摘要\n\ns\n\n# 资料\n\n- 旧";
        let out = append_to_section(body, "资料", "- 新");
        assert!(out.contains("# 资料\n\n\n- 新") || out.contains("# 资料\n\n- 新"), "插入到资料节");
        assert!(out.contains("- 旧"), "旧内容保留");
    }
}
