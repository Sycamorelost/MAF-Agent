//! 试卷文件引擎(PaperFs):每份试卷一个 `<learningcenter>/papers/<id>.json`。
//!
//! 文件内含元信息 + 题目快照 + 作答 + 批改结果(自包含,题库改动不影响);
//! 列表/详情都直接读文件,写盘走原子写。

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use serde::{Deserialize, Serialize};

use super::PaperItem;
use crate::study::lc;

/// 试卷磁盘格式(自包含快照)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PaperFile {
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
    #[serde(default)]
    pub items: Vec<PaperItem>,
}

/// 试卷文件引擎(目录绑定,便于测试注入独立目录)。
pub(crate) struct PaperFs {
    root: PathBuf,
}

impl PaperFs {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn file(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }

    /// 读取一份试卷(不存在/损坏返回 None)。
    pub(crate) fn read(&self, id: &str) -> Option<PaperFile> {
        let text = fs::read_to_string(self.file(id)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// 写一份试卷(原子写)。
    pub(crate) fn write(&self, p: &PaperFile) -> Result<(), String> {
        let text = serde_json::to_string_pretty(p).map_err(|e| e.to_string())?;
        lc::atomic_write(&self.file(&p.id), &text)
    }

    /// 删除一份试卷;不存在返回 false。
    pub(crate) fn remove(&self, id: &str) -> Result<bool, String> {
        let f = self.file(id);
        if !f.exists() {
            return Ok(false);
        }
        fs::remove_file(&f).map_err(|e| format!("remove {} failed: {e}", f.display()))?;
        Ok(true)
    }

    /// 全部试卷(按创建时间倒序)。
    pub(crate) fn list_all(&self) -> Vec<PaperFile> {
        let Ok(rd) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in rd.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&p) else {
                continue;
            };
            match serde_json::from_str::<PaperFile>(&text) {
                Ok(f) => out.push(f),
                Err(_) => log::warn!("skip corrupted paper file: {}", p.display()),
            }
        }
        out.sort_by(|a, b| b.created.cmp(&a.created));
        out
    }
}

/// 默认引擎(生产路径 = learningcenter/papers)。
static DEFAULT: OnceLock<PaperFs> = OnceLock::new();
pub(crate) fn default_fs() -> &'static PaperFs {
    DEFAULT.get_or_init(|| PaperFs::new(lc::papers_dir()))
}

/// 写操作互斥(组卷 / 作答 / 批改的读-改-写串行化)。
static WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
pub(crate) fn write_lock() -> Result<MutexGuard<'static, ()>, String> {
    WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|e| e.to_string())
}
