//! 试卷:自选/随机组卷、整卷作答与批改。
//!
//! 存储落在 `<learningcenter>/papers/<id>.json`(引擎见 `store`,判分见 `grade`):
//! - 试卷以题目快照存储,题库后续修改不影响已出试卷;
//! - 客观题(choice/judge/calc/cloze)由 Rust 确定性判分,主观题由模型按 rubric 批改;
//! - 整卷提交时先落作答再批改,批改失败可保留答案后重试;
//! - 主观题单题批改(study_question_grade)由用户点按钮触发,不做输入中自动批改。

mod grade;
mod store;

use std::collections::HashSet;

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::provider::Provider;

use super::bank;
use super::now_str;
use store::{PaperFile, PaperFs};

pub use grade::{GradeOutcome, is_objective, question_grade};
pub(crate) use store::{default_fs, write_lock};

/// 试卷行(列表用;answered 由文件内作答即时统计)。
#[derive(Debug, Clone, Serialize)]
pub struct PaperRow {
    pub id: String,
    pub plan_id: Option<String>,
    pub title: String,
    /// quiz(小测) / exam(整卷)
    pub kind: String,
    /// draft(未提交) / graded(已批改)
    pub status: String,
    pub score: Option<f64>,
    pub total: i64,
    pub created: String,
    pub submitted_at: Option<String>,
    pub graded_at: Option<String>,
    /// 已作答题目数
    pub answered: i64,
}

/// 试卷内的一道题(含作答与批改结果)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperItem {
    pub id: i64,
    pub question_id: i64,
    pub idx: i64,
    pub qtype: String,
    pub stem: String,
    #[serde(default)]
    pub options: Vec<String>,
    pub answer: String,
    #[serde(default)]
    pub explanation: String,
    #[serde(default)]
    pub node_name: String,
    pub user_answer: Option<String>,
    pub correct: Option<bool>,
    pub score: Option<f64>,
    pub feedback: Option<String>,
    #[serde(default)]
    pub graded: bool,
}

/// 试卷详情。
#[derive(Debug, Clone, Serialize)]
pub struct PaperDetail {
    pub paper: PaperRow,
    pub items: Vec<PaperItem>,
}

/// 整卷作答入参。
#[derive(Debug, Clone, Deserialize)]
pub struct PaperAnswerIn {
    pub idx: i64,
    #[serde(default)]
    pub user_answer: String,
}

/// 整卷批改结果。
#[derive(Debug, Clone, Serialize)]
pub struct PaperResult {
    /// 0-1
    pub score: f64,
    pub total: i64,
    pub correct_count: i64,
    pub results: Vec<PaperItemResult>,
}

/// 单题整卷批改结果。
#[derive(Debug, Clone, Serialize)]
pub struct PaperItemResult {
    pub question_idx: i64,
    pub correct: bool,
    pub score: f64,
    pub expected: String,
    pub explanation: String,
    pub feedback: String,
    pub user_answer: String,
}

/// 当前引擎:生产 = learningcenter/papers;测试 = target/test-tmp 下隔离目录。
fn fs() -> &'static PaperFs {
    #[cfg(test)]
    {
        static TEST_FS: std::sync::OnceLock<PaperFs> = std::sync::OnceLock::new();
        return TEST_FS.get_or_init(|| {
            PaperFs::new(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("target")
                    .join("test-tmp")
                    .join("paper-api"),
            )
        });
    }
    #[cfg(not(test))]
    {
        store::default_fs()
    }
}

fn norm_kind(raw: Option<&str>) -> &'static str {
    match raw.map(str::trim) {
        Some("quiz") => "quiz",
        _ => "exam",
    }
}

fn default_title() -> String {
    format!("整卷测试 · {}", chrono::Local::now().format("%Y-%m-%d %H:%M"))
}

/// 文件 → 列表行(answered 即时统计)。
fn to_row(f: &PaperFile) -> PaperRow {
    let answered = f
        .items
        .iter()
        .filter(|i| {
            i.user_answer
                .as_deref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false)
        })
        .count() as i64;
    PaperRow {
        id: f.id.clone(),
        plan_id: f.plan_id.clone(),
        title: f.title.clone(),
        kind: f.kind.clone(),
        status: f.status.clone(),
        score: f.score,
        total: f.total,
        created: f.created.clone(),
        submitted_at: f.submitted_at.clone(),
        graded_at: f.graded_at.clone(),
        answered,
    }
}

fn read_or_err(id: &str) -> Result<PaperFile, String> {
    fs().read(id).ok_or_else(|| "试卷不存在".to_string())
}

/// 试卷列表(plan_id 为空 = 全部,按创建时间倒序,最多 200 份)。
pub fn list(plan_id: Option<&str>) -> Result<Vec<PaperRow>, String> {
    let filter = plan_id.map(str::trim).filter(|s| !s.is_empty());
    let out = fs()
        .list_all()
        .iter()
        .filter(|f| filter.map(|p| f.plan_id.as_deref() == Some(p)).unwrap_or(true))
        .take(200)
        .map(to_row)
        .collect();
    Ok(out)
}

/// 试卷详情。
pub fn detail(id: &str) -> Result<PaperDetail, String> {
    let f = read_or_err(id)?;
    Ok(PaperDetail {
        paper: to_row(&f),
        items: f.items,
    })
}

/// 按题目 id 组卷(题目来自题库,组卷时快照题干/答案)。
pub fn create_manual(
    plan_id: Option<&str>,
    title: Option<&str>,
    kind: Option<&str>,
    question_ids: &[i64],
) -> Result<PaperDetail, String> {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for id in question_ids {
        if seen.insert(*id) {
            ids.push(*id);
        }
    }
    if ids.is_empty() {
        return Err("请至少选择 1 道题".into());
    }
    let mut items = Vec::new();
    for (idx, id) in ids.iter().enumerate() {
        let q = bank::get(*id)?;
        items.push(PaperItem {
            id: 0,
            question_id: q.id,
            idx: idx as i64,
            qtype: q.qtype,
            stem: q.stem,
            options: q.options,
            answer: q.answer,
            explanation: q.explanation,
            node_name: q.node_name,
            user_answer: None,
            correct: None,
            score: None,
            feedback: None,
            graded: false,
        });
    }
    let _g = write_lock()?;
    let f = PaperFile {
        id: format!("pp{}", crate::memory::new_session_id()),
        plan_id: plan_id
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from),
        title: title
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or_else(default_title),
        kind: norm_kind(kind).to_string(),
        status: "draft".into(),
        score: None,
        total: items.len() as i64,
        created: now_str(),
        submitted_at: None,
        graded_at: None,
        items,
    };
    fs().write(&f)?;
    detail(&f.id)
}

/// 随机组卷:按计划/知识点筛选;不足时明确报错,不凑题。
pub fn create_auto(
    plan_id: Option<&str>,
    node_ids: &[i64],
    count: u32,
    title: Option<&str>,
    kind: Option<&str>,
) -> Result<PaperDetail, String> {
    let want = count.clamp(1, 50);
    let plan_filter = plan_id.map(str::trim).filter(|s| !s.is_empty());
    let node_set: HashSet<i64> = node_ids.iter().copied().collect();
    // 题库全量内存筛选(个人题量有限,代价可接受);分页上限 200 不够用,这里绕过 list 直接取
    let pool: Vec<i64> = bank::list(None, None, None, 200, 0)
        .map_err(|e| e.to_string())?
        .items
        .into_iter()
        .filter(|q| plan_filter.map(|p| q.plan_id.as_deref() == Some(p)).unwrap_or(true))
        .filter(|q| node_set.is_empty() || q.node_id.map(|n| node_set.contains(&n)).unwrap_or(false))
        .map(|q| q.id)
        .collect();
    let avail = pool.len();
    if avail < want as usize {
        return Err(format!(
            "题库中符合条件的题目只有 {avail} 道,不足 {want} 道;请先在学习计划中多做小测或手动加题"
        ));
    }
    let mut pool = pool;
    pool.shuffle(&mut rand::thread_rng());
    let ids: Vec<i64> = pool.into_iter().take(want as usize).collect();
    create_manual(plan_id, title, kind, &ids)
}

/// 保存作答(重新作答会清空该题旧批改结果)。内部使用;整卷提交时统一调用。
fn save_answers(f: &mut PaperFile, answers: &[PaperAnswerIn]) {
    for a in answers {
        if let Some(it) = f.items.iter_mut().find(|i| i.idx == a.idx) {
            it.user_answer = Some(a.user_answer.trim().to_string());
            it.graded = false;
            it.correct = None;
            it.score = None;
            it.feedback = None;
        }
    }
}

/// 整卷提交:先落作答,再逐题批改,最后汇总(批改失败保留作答可重试)。
pub async fn submit(
    provider: &dyn Provider,
    model: &str,
    paper_id: &str,
    answers: &[PaperAnswerIn],
) -> Result<PaperResult, String> {
    // 落作答(锁内读-改-写;此后即使批改失败,作答已保存)
    let file = {
        let _g = write_lock()?;
        let mut f = read_or_err(paper_id)?;
        save_answers(&mut f, answers);
        fs().write(&f)?;
        f
    };
    if file.items.is_empty() {
        return Err("试卷没有题目".into());
    }

    let mut results = Vec::new();
    for it in &file.items {
        let ua = answers
            .iter()
            .find(|a| a.idx == it.idx)
            .map(|a| a.user_answer.trim().to_string())
            .or_else(|| it.user_answer.clone())
            .unwrap_or_default();
        let (score, feedback) = if is_objective(&it.qtype) {
            let g = grade::grade_objective(&it.qtype, &it.answer, &ua);
            (g.score, g.feedback)
        } else {
            grade::grade_subjective(provider, model, &it.qtype, &it.stem, &it.answer, &it.explanation, &ua)
                .await?
        };
        results.push(PaperItemResult {
            question_idx: it.idx,
            correct: score >= 0.6,
            score,
            expected: it.answer.clone(),
            explanation: it.explanation.clone(),
            feedback,
            user_answer: ua,
        });
    }

    let total = results.len() as i64;
    let correct_count = results.iter().filter(|r| r.correct).count() as i64;
    let score = results.iter().map(|r| r.score).sum::<f64>() / total as f64;
    {
        let _g = write_lock()?;
        let mut f = read_or_err(paper_id)?;
        for r in &results {
            if let Some(it) = f.items.iter_mut().find(|i| i.idx == r.question_idx) {
                it.correct = Some(r.correct);
                it.score = Some(r.score);
                it.feedback = Some(r.feedback.clone());
                it.graded = true;
            }
        }
        let now = now_str();
        f.status = "graded".into();
        f.score = Some(score);
        f.submitted_at = Some(now.clone());
        f.graded_at = Some(now);
        fs().write(&f)?;
    }
    Ok(PaperResult {
        score,
        total,
        correct_count,
        results,
    })
}

/// 删除试卷。
pub fn delete(id: &str) -> Result<(), String> {
    let _g = write_lock()?;
    if fs().remove(id)? {
        Ok(())
    } else {
        Err("试卷不存在".into())
    }
}

/// 删除某计划名下全部试卷(删除学习计划时调用)。
pub(crate) fn delete_by_plan(plan_id: &str) -> Result<(), String> {
    let _g = write_lock()?;
    for f in fs().list_all() {
        if f.plan_id.as_deref() == Some(plan_id) {
            fs().remove(&f.id)?;
        }
    }
    Ok(())
}

// ---------------- 旧表迁移 ----------------

/// 把旧表 study_papers / study_paper_items 导出为试卷文件后删表。表不存在 = 空操作。幂等。
pub(crate) fn export_legacy(conn: &rusqlite::Connection, fs: &PaperFs) -> Result<(), String> {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('study_papers','study_paper_items')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists == 0 {
        return Ok(());
    }
    let mut stmt = conn
        .prepare(
            "SELECT id, plan_id, title, kind, status, score, total, created, submitted_at, graded_at
             FROM study_papers",
        )
        .map_err(|e| e.to_string())?;
    let papers = stmt
        .query_map([], |r| {
            Ok(PaperFile {
                id: r.get(0)?,
                plan_id: r.get(1)?,
                title: r.get(2)?,
                kind: r.get(3)?,
                status: r.get(4)?,
                score: r.get(5)?,
                total: r.get(6)?,
                created: r.get(7)?,
                submitted_at: r.get(8)?,
                graded_at: r.get(9)?,
                items: Vec::new(),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let mut n = 0usize;
    for mut p in papers {
        let mut istmt = conn
            .prepare(
                "SELECT id, question_id, idx, qtype, stem, options_json, answer, explanation,
                        node_name, user_answer, correct, score, feedback, graded
                 FROM study_paper_items WHERE paper_id=?1 ORDER BY idx",
            )
            .map_err(|e| e.to_string())?;
        let items = istmt
            .query_map([&p.id], |r| {
                let options_json: String = r.get(5)?;
                let correct: Option<i64> = r.get(10)?;
                let graded: i64 = r.get(13)?;
                Ok(PaperItem {
                    id: r.get(0)?,
                    question_id: r.get(1)?,
                    idx: r.get(2)?,
                    qtype: r.get(3)?,
                    stem: r.get(4)?,
                    options: serde_json::from_str(&options_json).unwrap_or_default(),
                    answer: r.get(6)?,
                    explanation: r.get(7)?,
                    node_name: r.get(8)?,
                    user_answer: r.get(9)?,
                    correct: correct.map(|v| v != 0),
                    score: r.get(11)?,
                    feedback: r.get(12)?,
                    graded: graded != 0,
                })
            })
            .map_err(|e| e.to_string())?;
        for it in items {
            p.items.push(it.map_err(|e| e.to_string())?);
        }
        fs.write(&p)?;
        n += 1;
    }
    conn.execute_batch("DROP TABLE IF EXISTS study_paper_items; DROP TABLE IF EXISTS study_papers;")
        .map_err(|e| format!("drop paper tables failed: {e}"))?;
    log::info!("study_papers migrated to files: {n} papers");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::study::learn::bank::{BankAddInput, add, update, BankUpdateInput};

    fn seed_bank(plan: &str, stems: &[&str]) -> Vec<i64> {
        stems
            .iter()
            .enumerate()
            .map(|(i, s)| {
                add(&BankAddInput {
                    plan_id: Some(plan.into()),
                    node_id: Some(1),
                    node_name: "节点".into(),
                    subject: "数学".into(),
                    qtype: if i % 2 == 0 { "choice" } else { "short" }.into(),
                    stem: (*s).into(),
                    options: vec!["A. 甲".into(), "B. 乙".into()],
                    answer: "A".into(),
                    explanation: "解析".into(),
                    difficulty: 2,
                })
                .unwrap()
                .id
            })
            .collect()
    }

    #[test]
    fn create_manual_snapshots_and_auto_rejects_shortage() {
        let plan = "t00000c1";
        let ids = seed_bank(plan, &["题一", "题二", "题三"]);
        let d = create_manual(Some(plan), Some("自选卷"), Some("exam"), &[ids[0], ids[2], ids[0]]).unwrap();
        assert_eq!(d.items.len(), 2, "重复题目去重");
        assert_eq!(d.paper.status, "draft");
        assert_eq!(d.paper.total, 2);
        // 快照:题库改题不影响试卷
        update(
            ids[0],
            &BankUpdateInput {
                stem: "改后的题干".into(),
                options: vec![],
                answer: "B".into(),
                explanation: String::new(),
                difficulty: 1,
            },
        )
        .unwrap();
        let d2 = detail(&d.paper.id).unwrap();
        assert_eq!(d2.items[0].stem, "题一", "试卷保存题目快照");

        // 测试目录跨运行复用(题量可能因残留增加),按当前实际题量构造「不足」场景
        let n = bank::list(Some(plan), None, None, 200, 0).unwrap().total;
        let err = create_auto(Some(plan), &[], (n as u32) + 2, None, None).unwrap_err();
        assert!(err.contains(&format!("只有 {n} 道")), "不足时明确提示:{err}");
        let auto = create_auto(Some(plan), &[], 2, Some("随机卷"), None).unwrap();
        assert_eq!(auto.items.len(), 2);
    }

    #[test]
    fn save_answers_invalidates_previous_grade() {
        let plan = "t00000d1";
        let ids = seed_bank(plan, &["题一"]);
        let d = create_manual(Some(plan), None, Some("exam"), &[ids[0]]).unwrap();
        let mut f = fs().read(&d.paper.id).unwrap();
        save_answers(&mut f, &[PaperAnswerIn { idx: 0, user_answer: "A".into() }]);
        fs().write(&f).unwrap();
        let d2 = detail(&d.paper.id).unwrap();
        assert_eq!(d2.items[0].user_answer.as_deref(), Some("A"));
        assert!(!d2.items[0].graded);
        assert_eq!(d2.paper.answered, 1);
    }

    #[test]
    fn delete_by_plan_removes_only_own_papers() {
        let plan = "t00000e1";
        let ids = seed_bank(plan, &["题一"]);
        let keep_plan = "t00000e2";
        let ids2 = seed_bank(keep_plan, &["题二"]);
        let d = create_manual(Some(plan), None, None, &[ids[0]]).unwrap();
        let k = create_manual(Some(keep_plan), None, None, &[ids2[0]]).unwrap();
        delete_by_plan(plan).unwrap();
        assert!(detail(&d.paper.id).is_err());
        assert!(detail(&k.paper.id).is_ok());
    }
}
