//! 试题库接口层:测验生成自动入库、手动加题、筛选、编辑与删除。
//!
//! 存储落在 `<learningcenter>/bank/<scope>/<id>.json`(引擎见 `bank_store`):
//! - 生成题以 (plan_id, stem) 去重(同一计划内重复题干覆盖更新,不重复堆叠);
//! - 手动题 plan_id 为空(scope = manual),允许跨计划复用;
//! - 试卷以题目快照存储,题库后续修改/删除不影响已出试卷。
//!
//! `export_legacy` 负责把 db 旧表 question_bank 一次性导出为文件后删表(学习中心文件化)。

use serde::{Deserialize, Serialize};

use super::bank_store::{self, BankFs};
use super::now_str;
use super::quiz::QuizQuestion;

/// 题库题目(与磁盘 JSON 一一对应)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankQuestion {
    pub id: i64,
    pub plan_id: Option<String>,
    pub node_id: Option<i64>,
    pub node_name: String,
    pub subject: String,
    pub qtype: String,
    pub stem: String,
    pub options: Vec<String>,
    pub answer: String,
    pub explanation: String,
    pub difficulty: i64,
    pub source: String,
    pub created: String,
    pub updated: String,
}

/// 题库筛选与分页结果。
#[derive(Debug, Clone, Serialize)]
pub struct BankListResult {
    pub items: Vec<BankQuestion>,
    pub total: i64,
}

/// 手动加题入参。
#[derive(Debug, Clone, Deserialize)]
pub struct BankAddInput {
    pub plan_id: Option<String>,
    pub node_id: Option<i64>,
    #[serde(default)]
    pub node_name: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub qtype: String,
    pub stem: String,
    #[serde(default)]
    pub options: Vec<String>,
    pub answer: String,
    #[serde(default)]
    pub explanation: String,
    #[serde(default)]
    pub difficulty: i64,
}

/// 题库编辑入参(只允许改内容,不改归属)。
#[derive(Debug, Clone, Deserialize)]
pub struct BankUpdateInput {
    pub stem: String,
    #[serde(default)]
    pub options: Vec<String>,
    pub answer: String,
    #[serde(default)]
    pub explanation: String,
    #[serde(default)]
    pub difficulty: i64,
}

/// 允许的题型(与 STUDY_QUIZ 契约一致)。
pub fn normalize_qtype(raw: &str) -> &'static str {
    match raw.trim() {
        "choice" => "choice",
        "judge" => "judge",
        "calc" => "calc",
        "proof" => "proof",
        "cloze" => "cloze",
        "translate" => "translate",
        "reading" => "reading",
        "recite" => "recite",
        _ => "short",
    }
}

fn scope_of(plan_id: Option<&str>) -> String {
    plan_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(bank_store::MANUAL_SCOPE)
        .to_string()
}

/// 当前引擎:生产 = learningcenter/bank;测试 = target/test-tmp 下隔离目录(不污染真实数据)。
fn fs() -> &'static BankFs {
    #[cfg(test)]
    {
        static TEST_FS: std::sync::OnceLock<BankFs> = std::sync::OnceLock::new();
        return TEST_FS.get_or_init(|| {
            BankFs::new(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("target")
                    .join("test-tmp")
                    .join("bank-api"),
            )
        });
    }
    #[cfg(not(test))]
    {
        bank_store::default_fs()
    }
}

/// 生成题入库:同计划内按题干去重,已存在则覆盖内容。
pub(crate) fn upsert_quiz_questions(
    plan_id: &str,
    node_id: i64,
    node_name: &str,
    subject: &str,
    difficulty: i64,
    source: &str,
    questions: &[QuizQuestion],
) -> Result<Vec<i64>, String> {
    let _g = bank_store::write_lock()?;
    let fs = fs();
    let scope = scope_of(Some(plan_id));
    let existing = fs.load_scope(&scope);
    let now = now_str();
    let mut ids = Vec::new();
    for q in questions {
        let hit = existing.iter().find(|e| e.stem == q.stem);
        let item = match hit {
            Some(old) => BankQuestion {
                id: old.id,
                plan_id: old.plan_id.clone(),
                node_id: Some(node_id),
                node_name: node_name.to_string(),
                subject: subject.to_string(),
                qtype: normalize_qtype(&q.qtype).to_string(),
                stem: q.stem.clone(),
                options: q.options.clone(),
                answer: q.answer.clone(),
                explanation: q.explanation.clone(),
                difficulty,
                source: source.to_string(),
                created: old.created.clone(),
                updated: now.clone(),
            },
            None => BankQuestion {
                id: fs.next_id()?,
                plan_id: Some(plan_id.to_string()),
                node_id: Some(node_id),
                node_name: node_name.to_string(),
                subject: subject.to_string(),
                qtype: normalize_qtype(&q.qtype).to_string(),
                stem: q.stem.clone(),
                options: q.options.clone(),
                answer: q.answer.clone(),
                explanation: q.explanation.clone(),
                difficulty,
                source: source.to_string(),
                created: now.clone(),
                updated: now.clone(),
            },
        };
        fs.write(&item)?;
        ids.push(item.id);
    }
    Ok(ids)
}

/// 读取单题。
pub fn get(id: i64) -> Result<BankQuestion, String> {
    fs().find(id).ok_or_else(|| "题目不存在".to_string())
}

/// 筛选列表(plan_id / qtype / 关键词,按新到旧分页)。
pub fn list(
    plan_id: Option<&str>,
    qtype: Option<&str>,
    keyword: Option<&str>,
    limit: u32,
    offset: u32,
) -> Result<BankListResult, String> {
    let fs = fs();
    let plan_filter = plan_id.map(str::trim).filter(|s| !s.is_empty());
    let qtype_filter = qtype.map(str::trim).filter(|s| !s.is_empty());
    let kw = keyword
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase);
    let mut items: Vec<BankQuestion> = fs
        .load_all()
        .into_iter()
        .filter(|q| {
            plan_filter
                .map(|p| q.plan_id.as_deref() == Some(p))
                .unwrap_or(true)
        })
        .filter(|q| qtype_filter.map(|t| q.qtype == t).unwrap_or(true))
        .filter(|q| {
            kw.as_ref()
                .map(|k| {
                    q.stem.to_lowercase().contains(k.as_str())
                        || q.node_name.to_lowercase().contains(k.as_str())
                })
                .unwrap_or(true)
        })
        .collect();
    items.sort_by(|a, b| b.id.cmp(&a.id));
    let total = items.len() as i64;
    let limit = limit.clamp(1, 200) as usize;
    let offset = offset as usize;
    let items = items.into_iter().skip(offset).take(limit).collect();
    Ok(BankListResult { items, total })
}

/// 手动加题(同 scope 同题干去重:重复点击返回已有题目)。
pub fn add(input: &BankAddInput) -> Result<BankQuestion, String> {
    let stem = input.stem.trim();
    let answer = input.answer.trim();
    if stem.is_empty() || answer.is_empty() {
        return Err("题干与参考答案不能为空".into());
    }
    let _g = bank_store::write_lock()?;
    let fs = fs();
    let scope = scope_of(input.plan_id.as_deref());
    if let Some(old) = fs.load_scope(&scope).into_iter().find(|q| q.stem == stem) {
        return Ok(old);
    }
    let now = now_str();
    let q = BankQuestion {
        id: fs.next_id()?,
        plan_id: input
            .plan_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from),
        node_id: input.node_id,
        node_name: input.node_name.trim().to_string(),
        subject: input.subject.trim().to_string(),
        qtype: normalize_qtype(&input.qtype).to_string(),
        stem: stem.to_string(),
        options: input.options.clone(),
        answer: answer.to_string(),
        explanation: input.explanation.trim().to_string(),
        difficulty: input.difficulty.clamp(0, 5),
        source: "manual".into(),
        created: now.clone(),
        updated: now,
    };
    fs.write(&q)?;
    Ok(q)
}

/// 编辑题目内容。
pub fn update(id: i64, input: &BankUpdateInput) -> Result<BankQuestion, String> {
    let stem = input.stem.trim();
    let answer = input.answer.trim();
    if stem.is_empty() || answer.is_empty() {
        return Err("题干与参考答案不能为空".into());
    }
    let _g = bank_store::write_lock()?;
    let fs = fs();
    let old = fs.find(id).ok_or_else(|| "题目不存在".to_string())?;
    let q = BankQuestion {
        stem: stem.to_string(),
        options: input.options.clone(),
        answer: answer.to_string(),
        explanation: input.explanation.trim().to_string(),
        difficulty: input.difficulty.clamp(0, 5),
        updated: now_str(),
        ..old
    };
    fs.write(&q)?;
    Ok(q)
}

/// 删除题目(试卷使用快照,不受影响)。
pub fn delete(id: i64) -> Result<(), String> {
    let _g = bank_store::write_lock()?;
    let fs = fs();
    let q = fs.find(id).ok_or_else(|| "题目不存在".to_string())?;
    fs.remove(&q)
}

/// 删除某计划名下的全部题目(删除学习计划时调用)。
pub(crate) fn delete_scope(plan_id: &str) -> Result<(), String> {
    let _g = bank_store::write_lock()?;
    fs().remove_scope(&scope_of(Some(plan_id)))
}

// ---------------- 旧表迁移 ----------------

/// 把旧表 question_bank 导出为题库文件后删表。表不存在 = 空操作。幂等。
pub(crate) fn export_legacy(conn: &rusqlite::Connection, fs: &BankFs) -> Result<(), String> {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='question_bank'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists == 0 {
        return Ok(());
    }
    let mut stmt = conn
        .prepare(
            "SELECT id, plan_id, node_id, node_name, subject, qtype, stem, answer,
                    options_json, explanation, difficulty, source, created, updated
             FROM question_bank",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let options_json: String = r.get(8)?;
            Ok(BankQuestion {
                id: r.get(0)?,
                plan_id: r.get(1)?,
                node_id: r.get(2)?,
                node_name: r.get(3)?,
                subject: r.get(4)?,
                qtype: r.get(5)?,
                stem: r.get(6)?,
                options: serde_json::from_str(&options_json).unwrap_or_default(),
                answer: r.get(7)?,
                explanation: r.get(9)?,
                difficulty: r.get(10)?,
                source: r.get(11)?,
                created: r.get(12)?,
                updated: r.get(13)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut max_id = 0i64;
    let mut n = 0usize;
    for r in rows {
        let q = r.map_err(|e| e.to_string())?;
        max_id = max_id.max(q.id);
        fs.write(&q)?;
        n += 1;
    }
    fs.bump_seq(max_id)?;
    conn.execute("DROP TABLE question_bank", [])
        .map_err(|e| format!("drop question_bank failed: {e}"))?;
    log::info!("question_bank migrated to files: {n} questions");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// 测试用独立目录引擎;接口层默认引擎与数据目录耦合,这里直接测引擎级逻辑,
    /// 接口层行为由 export/去重的纯逻辑覆盖。
    fn tmp(case: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-tmp")
            .join("bank")
            .join(case);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn qq(stem: &str, answer: &str) -> QuizQuestion {
        QuizQuestion {
            idx: 0,
            qtype: "choice".into(),
            stem: stem.into(),
            options: vec!["A. 甲".into(), "B. 乙".into()],
            answer: answer.into(),
            explanation: "解析".into(),
        }
    }

    #[test]
    fn export_legacy_moves_rows_to_files_and_drops_table() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        // 模拟旧版 db:schema V35 起不再建 question_bank,测试自建旧表结构
        conn.execute_batch(
            "CREATE TABLE question_bank (
               id           INTEGER PRIMARY KEY AUTOINCREMENT,
               plan_id      TEXT,
               node_id      INTEGER,
               node_name    TEXT NOT NULL DEFAULT '',
               subject      TEXT NOT NULL DEFAULT '',
               qtype        TEXT NOT NULL DEFAULT 'short',
               stem         TEXT NOT NULL,
               options_json TEXT NOT NULL DEFAULT '[]',
               answer       TEXT NOT NULL DEFAULT '',
               explanation  TEXT NOT NULL DEFAULT '',
               difficulty   INTEGER NOT NULL DEFAULT 0,
               source       TEXT NOT NULL DEFAULT '',
               created      TEXT NOT NULL DEFAULT '',
               updated      TEXT NOT NULL DEFAULT ''
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO question_bank(plan_id, node_name, subject, qtype, stem, options_json, answer, explanation, difficulty, source, created, updated)
             VALUES('p0000001','节点','数学','choice','题干一','[]','A','解析',2,'quiz','2026-01-01','2026-01-01')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO question_bank(plan_id, node_name, subject, qtype, stem, options_json, answer, explanation, difficulty, source, created, updated)
             VALUES(NULL,'自定义','英语','cloze','I ___ a student.','[]','am','',1,'manual','2026-01-02','2026-01-02')",
            [],
        )
        .unwrap();

        let fs = BankFs::new(tmp("export"));
        export_legacy(&conn, &fs).unwrap();

        // 表已删除
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='question_bank'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
        // 文件落位:计划题进计划目录,手动题进 manual
        let all = fs.load_all();
        assert_eq!(all.len(), 2);
        assert!(fs.root.join("p0000001").join("1.json").exists());
        assert!(fs.root.join(bank_store::MANUAL_SCOPE).join("2.json").exists());
        // id 计数器抬高,新分配不复用旧 id
        assert!(fs.read_seq() >= 2);
        // 幂等:再次执行是空操作
        export_legacy(&conn, &fs).unwrap();
    }

    #[test]
    fn normalize_qtype_whitelist() {
        assert_eq!(normalize_qtype("choice"), "choice");
        assert_eq!(normalize_qtype("recite"), "recite");
        assert_eq!(normalize_qtype("whatever"), "short");
        assert_eq!(qq("s", "a").qtype, "choice");
    }

    // 以下走接口层(测试目录 bank-api);用互不相同的 plan_id 隔离并行测试。

    #[test]
    fn upsert_dedupes_by_plan_and_stem() {
        let plan = "t00000a1";
        let ids = upsert_quiz_questions(plan, 1, "知识点", "数学", 3, "quiz", &[qq("题干一", "A"), qq("题干二", "B")]).unwrap();
        assert_eq!(ids.len(), 2);
        // 第二次同题干 → 覆盖而不是新增
        let ids2 = upsert_quiz_questions(plan, 1, "知识点", "数学", 3, "quiz", &[qq("题干一", "B")]).unwrap();
        assert_eq!(ids2, vec![ids[0]], "同题干复用原 id");
        assert_eq!(get(ids[0]).unwrap().answer, "B", "内容被覆盖");
        assert_eq!(list(Some(plan), None, None, 50, 0).unwrap().total, 2);
    }

    #[test]
    fn manual_add_filter_update_delete() {
        let plan = "t00000b1";
        let input = |stem: &str, answer: &str| BankAddInput {
            plan_id: Some(plan.into()),
            node_id: None,
            node_name: "自定义".into(),
            subject: "英语".into(),
            qtype: "cloze".into(),
            stem: stem.into(),
            options: vec![],
            answer: answer.into(),
            explanation: String::new(),
            difficulty: 2,
        };
        let a = add(&input("I ___ a student.", "am")).unwrap();
        assert_eq!(a.qtype, "cloze");
        // 重复加题 → 返回已有
        let b = add(&input("I ___ a student.", "am")).unwrap();
        assert_eq!(b.id, a.id);
        assert_eq!(list(Some(plan), Some("cloze"), Some("student"), 50, 0).unwrap().total, 1);
        assert_eq!(list(Some(plan), Some("choice"), None, 50, 0).unwrap().total, 0);
        let up = update(
            a.id,
            &BankUpdateInput {
                stem: "He ___ a student.".into(),
                options: vec![],
                answer: "is".into(),
                explanation: String::new(),
                difficulty: 1,
            },
        )
        .unwrap();
        assert_eq!(up.answer, "is");
        delete(a.id).unwrap();
        assert!(get(a.id).is_err());
        assert!(delete(a.id).is_err(), "重复删除报不存在");
    }
}
