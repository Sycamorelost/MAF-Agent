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

/// 读取计划的学习者配置(V31 起;旧计划三列为 NULL → generic,行为同现状)。
/// plans.subject 落库时已被归一为 key,这里对历史原文数据再做一次归一兜底。
pub(crate) fn plan_profile(
    conn: &rusqlite::Connection,
    plan_id: &str,
    difficulty: Option<i64>,
) -> template::LearnerProfile {
    let row: Option<(Option<String>, Option<String>, Option<String>, i64, Option<String>)> = conn
        .query_row(
            "SELECT subject, grade_band, exam_target, daily_minutes, preferences FROM plans WHERE id=?1",
            [plan_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .ok();
    let Some((subject, band, exam, daily, prefs)) = row else {
        return template::LearnerProfile::default();
    };
    let raw = subject.unwrap_or_default();
    let key = crate::study::subject::normalize_subject(&raw);
    // 落库值已是归一 key 时,展示名交给 config_block 用标签兜底;原文则保留
    let display = if !raw.is_empty() && raw.eq_ignore_ascii_case(key.as_str()) {
        String::new()
    } else {
        raw
    };
    template::LearnerProfile {
        subject: display,
        subject_key: key.as_str().to_string(),
        grade_band: band.unwrap_or_default(),
        exam_target: exam.unwrap_or_default(),
        daily_minutes: daily.max(0) as u32,
        preferences: prefs.unwrap_or_default(),
        difficulty,
    }
}

fn unit_get_conn(
    conn: &rusqlite::Connection,
    plan_id: &str,
    node_id: i64,
) -> Result<Option<UnitRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, plan_id, node_id, node_name, est_minutes, content_md, created
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
                est_minutes: r.get(4)?,
                content_md: r.get(5)?,
                created: r.get(6)?,
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
    provider: &dyn Provider,
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
    let profile = {
        let c = conn()?;
        plan_profile(&c, plan_id, Some(difficulty))
    };
    let mut sys = format!("{}{}", template::effective("study_guide"), profile.appendix());
    if free_form {
        sys.push_str(
            "\n\n补充要求:不要套用固定小节结构,按内容自然组织行文,仍使用 Markdown 格式;呈现方式仍按学科附录要求(表格/引用/加粗重点优先,代码块只用于确需逐字的代码与输出)。",
        );
    }
    let user = format!(
        "{}学习主题:{}\n当前水平:{}\n知识点:{}(难度 {}/5)\n为什么重要:{}\n输出语言:{}",
        profile.config_block(),
        topic,
        level_label(&level),
        name,
        difficulty,
        if why.is_empty() { "未说明" } else { &why },
        lang_label
    );
    let text = llm_plain(provider, model, vec![sys_msg(sys), user_msg(user)]).await?;
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
        est_minutes: 20 + difficulty * 10,
        content_md: content,
        created: now_str(),
    };
    c.execute(
        "INSERT INTO study_units(plan_id, node_id, node_name, est_minutes, content_md, created)
         VALUES(?1,?2,?3,?4,?5,?6)
         ON CONFLICT(plan_id, node_id) DO UPDATE SET
           node_name=?3, est_minutes=?4, content_md=?5, created=?6",
        rusqlite::params![
            row.plan_id,
            row.node_id,
            row.node_name,
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
/// wrong_context:错题变式再练(P3)——上一轮答错题的题干与答案,出同知识点不同题面的变式。
pub async fn new_quiz(
    provider: Box<dyn Provider>,
    model: &str,
    plan_id: &str,
    node_id: i64,
    wrong_context: Option<&str>,
) -> Result<QuizRow, String> {
    let (topic, level, lang, name, difficulty, why, _) = {
        let c = conn()?;
        unit_context(&c, plan_id, node_id)?
    };
    let lang_label = if lang == "en" { "English" } else { "中文" };
    let profile = {
        let c = conn()?;
        plan_profile(&c, plan_id, Some(difficulty))
    };
    let sys = format!("{}{}", template::effective("study_quiz"), profile.appendix());
    let base_user = format!(
        "{}学习主题:{}\n当前水平:{}\n知识点:{}(难度 {}/5)\n要点:{}\n输出语言:{}",
        profile.config_block(),
        topic,
        level_label(&level),
        name,
        difficulty,
        if why.is_empty() { "未说明" } else { &why },
        lang_label
    );
    // 错题变式:围绕同一要点换题面/情境/数值,严禁照抄原题
    let base_user = match wrong_context.map(str::trim).filter(|s| !s.is_empty()) {
        Some(wc) => format!(
            "{base_user}\n\n以下题目学习者上一轮答错,请围绕相同知识点出变式题:考查同一要点,但必须改变题面、情境或数值,不得照抄原题:\n{wc}"
        ),
        None => base_user,
    };

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
    let quiz_id = c.last_insert_rowid();
    // 每个生成的题目都入库,供试题库筛选与自定义组卷(同计划同题干去重)
    let subject = if profile.subject.trim().is_empty() {
        profile.subject_key.clone()
    } else {
        profile.subject.clone()
    };
    super::bank::upsert_quiz_questions(plan_id, node_id, &name, &subject, difficulty, "quiz", &questions)?;
    Ok(QuizRow {
        id: quiz_id,
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

    #[test]
    fn plan_profile_reads_subject_columns() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        crate::study::learn::test_util::seed_plan(&conn, "p0000001", "2026-01-01T00:00:00Z");

        // 旧计划(三列 NULL)→ generic,无配置块无附录(行为同现状)
        let p = plan_profile(&conn, "p0000001", Some(2));
        assert!(p.is_generic());
        assert!(p.config_block().is_empty());
        assert!(p.appendix().is_empty());

        // 落库归一 key + 义教学段 → 配置块用展示名 + 附录含义教总要求
        conn.execute(
            "UPDATE plans SET subject='math', grade_band='junior', exam_target='中考' WHERE id='p0000001'",
            [],
        )
        .unwrap();
        let p2 = plan_profile(&conn, "p0000001", Some(3));
        assert!(!p2.is_generic());
        let block = p2.config_block();
        assert!(block.contains("学科=数学"), "{block}");
        assert!(block.contains("难度=3"));
        let ap = p2.appendix();
        assert!(ap.contains("数学抽象"));
        assert!(ap.contains("义务教育课程方案和课程标准(2022年版)"));

        // 历史原文值兜底归一,展示名保留原文
        conn.execute(
            "UPDATE plans SET subject='高中数学' WHERE id='p0000001'",
            [],
        )
        .unwrap();
        let p3 = plan_profile(&conn, "p0000001", None);
        assert_eq!(p3.subject_key, "math");
        assert_eq!(p3.subject, "高中数学");

        // 用户明确要求(preferences)随计划落库注入配置块
        conn.execute(
            "UPDATE plans SET preferences='多给表格,不要代码块' WHERE id='p0000001'",
            [],
        )
        .unwrap();
        let p4 = plan_profile(&conn, "p0000001", None);
        assert!(
            p4.config_block().contains("用户明确要求=多给表格,不要代码块"),
            "{}",
            p4.config_block()
        );
        assert!(!p4.is_generic());
    }
}
