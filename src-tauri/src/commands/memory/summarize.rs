//! memory.md 的 LLM 提炼命令(按水位线增量提炼)。
//!
//! 从 `diff.rs` 拆出:提炼输入口径 / 输出清洗 / 水位线推进 + 单测,
//! 保持 memory 命令子模块单文件 ≤400 行。

use serde::Deserialize;

use crate::db::messages;
use crate::memory;

/// 提炼器系统提示(对应 idea/03-memory-flow.md §2.2 增量提炼)。
const SUMMARIZE_SYSTEM: &str = r#"你是 myAgent 的记忆提炼器。根据「现有 memory.md 正文」和「新增对话消息」,输出**更新后的完整 memory.md 正文**。

规则:
1. 只输出 Markdown 正文本身——不要 frontmatter(--- 标记)、不要代码块包裹、不要任何解释性前言。
2. 默认维护以下已知章节(没有的可以新增,空的可以省略):# 会话摘要 / # 关键事实与结论 / # 用户偏好与约束 / # 待办 / 未决问题 / # 文档摘要 / # 资料
3. 现有正文中的用户自定义章节必须原样保留,不得删除或改写。
4. 「会话摘要」3-6 句滚动更新;「关键事实与结论」用列表;「待办 / 未决问题」用 - [ ] 复选框。
5. 只记录长期有价值的信息(事实、结论、偏好、约束、待办);寒暄、闲聊、一次性细节不记录。
6. 总长度控制在 2000 tokens 以内;逼近上限时优先压缩低价值细节,保留事实与待办。
7. 「文档摘要」节只依据输入中给出的文档骨架与对话结论来写:每份文档 1-2 行(讲什么、关键结论、后续去哪查)。没有正文依据时不要编造数字、页码或原文。"#;

#[derive(Debug, Deserialize)]
pub struct SummarizeMemoryInput {
    pub session_id: String,
    pub provider_id: String,
    pub model: String,
    pub custom_base_url: Option<String>,
    pub custom_display_name: Option<String>,
}

/// 调 LLM 提炼记忆:读现有 memory.md + 水位线之后的新增消息 → 返回更新后的完整 memory.md(含 frontmatter)。
/// 注意:本命令只生成内容,不落盘;由前端拿到结果后再调 write_memory(note="auto_summarize")。
#[tauri::command]
pub async fn summarize_memory(input: SummarizeMemoryInput) -> Result<String, String> {
    use futures::StreamExt;

    // 1. 读现有 memory.md(frontmatter 用于回写水位线)
    let (mut fm, body) = memory::read_memory(&input.session_id)?;

    // 2. 从水位线之后「按时间正序」取一批,而不是"最近 20 条":
    //    旧实现取最近 20 条却把水位线推到最新一条,中间消息会被永久跳过。
    const BATCH: i64 = 40;
    let batch = messages::after_id(&input.session_id, fm.summarized_msg_id, BATCH)?;
    if batch.is_empty() {
        return Err("水位线之后无新增消息,无需提炼".into());
    }
    // 单条截断 1000 字符、正文总量 ≤ 8000 字符;超预算时只取连续前缀,
    // 水位线停在最后一条已提炼的消息上,剩余消息下轮继续,不产生空洞。
    let picked = pick_summary_batch(&batch, PER_MSG_CHARS, TOTAL_CHARS);
    let history_text = picked
        .iter()
        .map(|(_, line)| line.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");

    // 3. 组请求(附带文档骨架:文件名 / 规模 / 章节,不含正文)
    let docs = documents_block(&input.session_id);
    let user = format!(
        "【现有 memory.md 正文】\n{}\n\n【新增对话消息】\n{}\n{}",
        body.trim(),
        history_text,
        docs
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
                ..Default::default()
            },
            crate::provider::ChatMessage {
                role: "user".into(),
                content: user,
                images: vec![],
                ..Default::default()
            },
        ],
        temperature: Some(0.3),
        max_tokens: None,
        top_p: None,
        reasoning: None, // 记忆提炼走自有口径,不启用思考
        stream: true,
        ..Default::default()
    };

    // 4. 流式收全量文本
    let mut stream = provider.chat_stream(req).await.map_err(|e| e.to_string())?;
    let mut out = String::new();
    while let Some(chunk) = stream.next().await {
        match chunk {
            crate::provider::StreamChunk::Delta { content } => out.push_str(&content),
            crate::provider::StreamChunk::Reasoning { .. } => {} // 提炼只取正文,忽略思考流
            crate::provider::StreamChunk::Error { message } => return Err(message),
            crate::provider::StreamChunk::Usage { .. } => {}
            crate::provider::StreamChunk::Done { .. } => break,
            _ => {}
        }
    }

    // 5. 清洗输出:去代码块包裹 / 误带的 frontmatter
    let new_body = clean_summarized_body(&out);
    if new_body.trim().is_empty() {
        return Err("提炼结果为空".into());
    }

    // 6. 用现有 frontmatter 重新渲染完整文件内容(水位线推进到本批最后一条)
    if let Some((last_id, _)) = picked.last() {
        fm.summarized_msg_id = fm.summarized_msg_id.max(*last_id);
    }
    fm.updated = chrono::Local::now().to_rfc3339();
    memory::frontmatter::render(&fm, &new_body)
}

/// 单条消息入提炼输入时的正文上限(字符)。
const PER_MSG_CHARS: usize = 1_000;
/// 提炼输入正文总量上限(字符)。
const TOTAL_CHARS: usize = 8_000;

/// 从「水位线之后按 id 升序的消息批次」中挑出可入提炼输入的连续前缀。
///
/// 返回 (消息 id, 渲染行) 升序列表,**至少包含一条**:
/// 超出预算时截断的是批次尾部而非中间,水位线只推进到已提炼的最后一条,
/// 从根上避免旧实现「取最近 20 条 + 水位线跳到最新」造成的永久丢失。
pub(crate) fn pick_summary_batch(
    msgs: &[crate::db::messages::MessageRow],
    per_msg_chars: usize,
    total_chars: usize,
) -> Vec<(i64, String)> {
    let mut out: Vec<(i64, String)> = Vec::new();
    let mut used = 0usize;
    for m in msgs {
        let mut text = truncate_chars(&m.content, per_msg_chars);
        // R4:把附件摘要带进提炼输入,否则「文档摘要」节只能靠模型猜
        let digest = attachment_digest(&m.attachments);
        if !digest.is_empty() {
            text.push('\n');
            text.push_str(&digest);
        }
        let line = format!("{}: {}", m.role, text);
        let len = line.chars().count();
        if used + len > total_chars && !out.is_empty() {
            break;
        }
        used += len;
        out.push((m.id, line));
    }
    out
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…(已截断)")
    } else {
        s.to_string()
    }
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
    if text.starts_with("---")
        && let Ok((_fm, body)) = memory::frontmatter::parse(&text)
        && !body.trim().is_empty()
    {
        text = body;
    }

    text.trim().to_string()
}

/// 附件摘要(每份文档一行):提炼器据此知道"这份文档是什么",不读正文也能写文档摘要。
fn attachment_digest(raw: &Option<String>) -> String {
    let Some(raw) = raw else {
        return String::new();
    };
    let Ok(list) = serde_json::from_str::<Vec<serde_json::Value>>(raw) else {
        return String::new();
    };
    let mut out: Vec<String> = Vec::new();
    for m in list {
        let Some(name) = m.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let sum = m.get("summary").and_then(|v| v.as_str()).unwrap_or("");
        let line = if sum.is_empty() {
            format!("[文档:{name}]")
        } else {
            format!("[文档:{name}] {sum}")
        };
        out.push(line.chars().take(360).collect());
    }
    out.join("\n")
}

/// 本会话文档骨架(名称 / 规模 / 章节);无文档时返回空串。
fn documents_block(session_id: &str) -> String {
    let conn = match crate::db::conn().lock() {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    let outline = crate::doc::outline(&conn, session_id).unwrap_or_default();
    if outline.is_empty() {
        return String::new();
    }
    format!(
        "\n\n【本会话已上传文档(仅骨架,正文未在本次输入中)】{outline}\n\
         「文档摘要」节请基于上面的骨架与对话结论写,不要编造具体数据。"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: i64, role: &str, content: &str) -> crate::db::messages::MessageRow {
        crate::db::messages::MessageRow {
            id,
            session_id: "s1".into(),
            role: role.into(),
            content: content.into(),
            model_id: None,
            created: String::new(),
            tokens: 0,
            attachments: None,
            sources: None,
            parent_id: None,
            branch_id: "main".into(),
            reasoning: String::new(),
            partial: false,
        }
    }

    /// P0-1 回归:批次必须按 id 升序排列(旧实现反序取最近 N 条)。
    #[test]
    fn pick_batch_keeps_ascending_order() {
        let rows = vec![msg(1, "user", "一"), msg(2, "assistant", "二"), msg(3, "user", "三")];
        let picked = pick_summary_batch(&rows, 1_000, 8_000);
        assert_eq!(picked.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert!(picked[0].1.starts_with("user: 一"));
    }

    /// P0-1 回归:超预算只截断尾部,水位线推进到最后一条入选消息,
    /// 剩余消息留在下一批 → 不存在"中间消息永不提炼"的空洞。
    #[test]
    fn pick_batch_stops_at_budget_without_hole() {
        let rows = vec![
            msg(10, "user", &"甲".repeat(100)),
            msg(11, "assistant", &"乙".repeat(100)),
            msg(12, "user", &"丙".repeat(100)),
            msg(13, "assistant", &"丁".repeat(100)),
        ];
        // 每条渲染行 ≈ 103 字符,预算 250 只能装 2 条
        let picked = pick_summary_batch(&rows, 1_000, 250);
        assert_eq!(picked.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![10, 11]);
        // 第二批从 11 之后续上,12/13 不会被跳过
        let rest = pick_summary_batch(&rows[2..], 1_000, 250);
        assert_eq!(rest.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![12, 13]);
    }

    /// 即使单条就超总预算,也必须至少提炼一条,否则水位线永远推不动。
    #[test]
    fn pick_batch_keeps_at_least_one_oversized_message() {
        let rows = vec![msg(7, "user", &"长".repeat(5_000)), msg(8, "assistant", "短")];
        let picked = pick_summary_batch(&rows, 1_000, 500);
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].0, 7);
    }

    /// 附件摘要要随消息进入提炼输入(文档摘要节的依据)。
    #[test]
    fn pick_batch_includes_attachment_digest() {
        let mut m = msg(9, "user", "看下这个文档");
        m.attachments = Some(r#"[{"name":"需求.md","summary":"讲双写一致性"}]"#.into());
        let picked = pick_summary_batch(&[m], 1_000, 8_000);
        assert!(picked[0].1.contains("[文档:需求.md] 讲双写一致性"));
    }
}
