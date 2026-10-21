//! memory.md 读写 / 版本 diff / 提炼 commands。

use serde::Deserialize;
use tauri::State;

use super::{BudgetStatus, MemoryDto, VersionItem};
use crate::db::messages;
use crate::memory::{self, watcher::WatcherState};

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
pub(crate) fn resolve_section(section: Option<&str>) -> Result<&str, String> {
    let sec = section.map(str::trim).filter(|s| !s.is_empty()).unwrap_or("资料");
    if memory::template::KNOWN_SECTIONS.contains(&sec) {
        Ok(sec)
    } else {
        Err(format!("未知记忆节: {sec}(仅支持 6 个标准节)"))
    }
}

/// 把 text 追加到 body 中第一个以 `# heading` 开头的一级/二级/三级标题下方。
/// 若不存在该标题则在末尾追加 `\n\n# heading\n\ntext`。
pub(crate) fn append_to_section(body: &str, heading: &str, text: &str) -> String {
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
