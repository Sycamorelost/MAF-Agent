//! 自学材料与测验生成(命中缓存 / 每次新卷)。

use serde::Serialize;

use crate::provider::Provider;
use crate::study::pipeline::{llm_plain, sys_msg, user_msg};
use crate::study::schedule::level_label;
use crate::study::store;
use crate::study::template;

use super::conn;
use super::now_str;
use super::quiz::{QuizRow, parse_quiz};

/// 自学单元行。
#[derive(Debug, Clone, Serialize)]
pub struct UnitRow {
    pub id: i64,
    pub plan_id: String,
    pub node_id: i64,
    pub node_name: String,
    pub title: String,
    pub est_minutes: i64,
    pub content_md: String,
    pub created: String,
}

/// 单元生成上下文:(topic, level, lang, 节点名, 难度, 选址原因, 已缓存单元)。
type UnitContext = (String, String, String, String, i64, String, Option<UnitRow>);

/// 读取计划与知识点的生成上下文;已缓存单元直接返回。
pub(crate) fn unit_context(
    conn: &rusqlite::Connection,
    plan_id: &str,
    node_id: i64,
) -> Result<UnitContext, String> {
    let (topic, level, lang): (String, String, String) = conn
        .query_row(
            "SELECT topic, level, lang FROM plans WHERE id=?1",
            [plan_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| "学习计划不存在".to_string())?;
    let (name, difficulty, why): (String, i64, String) = conn
        .query_row(
            "SELECT name, difficulty, why FROM knowledge_nodes WHERE plan_id=?1 AND id=?2",
            rusqlite::params![plan_id, node_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| "知识点不存在".to_string())?;
    let existing = unit_get_conn(conn, plan_id, node_id)?;
    Ok((topic, level, lang, name, difficulty, why, existing))
}

fn unit_get_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
    node_id: i64,
) -> Result<Option<UnitRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, plan_id, node_id, node_name, title, est_minutes, content_md, created
             FROM study_units WHERE plan_id=?1 AND node_id=?2",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map(rusqlite::params![plan_id, node_id], |r| {
            Ok(UnitRow {
                id: r.get(0)?,
                plan_id: r.get(1)?,
                node_id: r.get(2)?,
                node_name: r.get(3)?,
                title: r.get(4)?,
                est_minutes: r.get(5)?,
                content_md: r.get(6)?,
                created: r.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    match rows.next() {
        Some(Ok(u)) => Ok(Some(u)),
        Some(Err(e)) => Err(e.to_string()),
        None => Ok(None),
    }
}

/// 生成(或命中缓存)一个知识点的自学材料。free_form = 不套固定小节结构。
pub async fn generate_unit(
    provider: Box<dyn Provider>,
    model: &str,
    plan_id: &str,
    node_id: i64,
    free_form: bool,
    force: bool,
) -> Result<UnitRow, String> {
    let (topic, level, lang, name, difficulty, why, existing) = {
        let c = conn()?;
        unit_context(&c, plan_id, node_id)?
    };
    if let Some(u) = existing {
        if !force {
            return Ok(u);
        }
        let c = conn()?;
        c.execute(
            "DELETE FROM study_units WHERE plan_id=?1 AND node_id=?2",
            rusqlite::params![plan_id, node_id],
        )
        .map_err(|e| e.to_string())?;
    }

    let lang_label = if lang == "en" { "English" } else { "中文" };
    let mut sys = template::effective("study_guide");
    if free_form {
        sys.push_str(
            "\n\n补充要求:不要套用固定小节结构,按内容自然组织行文,但仍使用 Markdown 格式与代码围栏规范。",
        );
    }
    let user = format!(
        "学习主题:{}\n当前水平:{}\n知识点:{}(难度 {}/5)\n为什么重要:{}\n输出语言:{}",
        topic,
        level_label(&level),
        name,
        difficulty,
        if why.is_empty() { "未说明" } else { &why },
        lang_label
    );
    let text = llm_plain(provider.as_ref(), model, vec![sys_msg(sys), user_msg(user)]).await?;
    let content = strip_outer_fence(&text);
    if content.trim().is_empty() {
        return Err("模型未返回有效内容".into());
    }

    let c = conn()?;
    let mut row = UnitRow {
        id: 0,
        plan_id: plan_id.to_string(),
        node_id,
        node_name: name.clone(),
        title: name.clone(),
        est_minutes: 20 + difficulty * 10,
        content_md: content,
        created: now_str(),
    };
    c.execute(
        "INSERT INTO study_units(plan_id, node_id, node_name, title, est_minutes, content_md, created)
         VALUES(?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(plan_id, node_id) DO UPDATE SET
           node_name=?3, title=?4, est_minutes=?5, content_md=?6, created=?7",
        rusqlite::params![
            row.plan_id,
            row.node_id,
            row.node_name,
            row.title,
            row.est_minutes,
            row.content_md,
            row.created,
        ],
    )
    .map_err(|e| format!("save unit failed: {e}"))?;
    row.id = c.last_insert_rowid();
    store::add_log(&c, plan_id, Some(node_id), "learn", 0)?;
    Ok(row)
}

/// 生成(不缓存,每次新卷)一个知识点的测验;解析失败带反馈重试 1 次。
pub async fn new_quiz(
    provider: Box<dyn Provider>,
    model: &str,
    plan_id: &str,
    node_id: i64,
) -> Result<QuizRow, String> {
    let (topic, level, lang, name, difficulty, why, _) = {
        let c = conn()?;
        unit_context(&c, plan_id, node_id)?
    };
    let lang_label = if lang == "en" { "English" } else { "中文" };
    let sys = template::effective("study_quiz");
    let base_user = format!(
        "学习主题:{}\n当前水平:{}\n知识点:{}(难度 {}/5)\n要点:{}\n输出语言:{}",
        topic,
        level_label(&level),
        name,
        difficulty,
        if why.is_empty() { "未说明" } else { &why },
        lang_label
    );

    let mut last_err = String::new();
    let mut questions = None;
    for attempt in 1..=2u32 {
        let user = if attempt == 1 {
            base_user.clone()
        } else {
            format!(
                "{base_user}\n\n上一轮输出存在问题:{last_err}\n必须只输出约定 Schema 的 JSON,恰好 5 题。"
            )
        };
        let text = llm_plain(
            provider.as_ref(),
            model,
            vec![sys_msg(sys.clone()), user_msg(user)],
        )
        .await?;
        match parse_quiz(&text) {
            Some(qs) => {
                questions = Some(qs);
                break;
            }
            None => last_err = "不是合法的 {\"questions\":[...]} JSON 或有效题目不足 3 道".into(),
        }
    }
    let questions = questions.ok_or_else(|| format!("测验生成失败:{last_err}"))?;

    let c = conn()?;
    let payload = serde_json::to_string(&questions).map_err(|e| e.to_string())?;
    c.execute(
        "INSERT INTO quizzes(plan_id, node_id, node_name, payload_json, created) VALUES(?1,?2,?3,?4,?5)",
        rusqlite::params![plan_id, node_id, name, payload, now_str()],
    )
    .map_err(|e| format!("save quiz failed: {e}"))?;
    Ok(QuizRow {
        id: c.last_insert_rowid(),
        plan_id: plan_id.to_string(),
        node_id,
        node_name: name,
        questions,
        created: now_str(),
    })
}

/// 剥掉包裹全文的最外层代码围栏(不碰正文内的代码块)。
pub fn strip_outer_fence(text: &str) -> String {
    let s = text.trim();
    let first = s.lines().next().unwrap_or("");
    if !first.starts_with("```") || !s.trim_end().ends_with("```") {
        return s.to_string();
    }
    let body = s.strip_prefix(first).unwrap_or(s);
    let body = body.strip_suffix("```").unwrap_or(body);
    body.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_outer_fence_keeps_inner_code_blocks() {
        assert_eq!(strip_outer_fence("```markdown\n# 标题\n```"), "# 标题");
        assert_eq!(strip_outer_fence("# 标题\n\n正文"), "# 标题\n\n正文");
        let with_code = "# 标题\n\n```rust\nfn main() {}\n```\n\n结尾";
        assert_eq!(strip_outer_fence(with_code), with_code, "未包裹的正文不动");
    }
}
