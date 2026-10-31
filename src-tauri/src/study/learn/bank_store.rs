//! 试题库文件引擎(BankFs):题目以 `<learningcenter>/bank/<scope>/<id>.json` 落盘。
//!
//! - scope = plan_id(测验生成题归属计划) 或 `manual`(手动加题,跨计划复用);
//! - id 由 `_seq` 计数器单调分配(全局唯一,删除不复用);
//! - 写盘统一走原子写(tmp + rename);读时按文件名数字 id 排序,全量入内存筛选
//!   (个人学习场景题量有限,代价可接受,换来免索引的透明文件结构)。

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use super::bank::BankQuestion;
use crate::study::lc;

/// 手动加题的归属目录名。
pub const MANUAL_SCOPE: &str = "manual";

/// 题库文件引擎(目录绑定,便于测试注入独立目录)。
pub(crate) struct BankFs {
    pub(crate) root: PathBuf,
}

impl BankFs {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn scope_dir(&self, scope: &str) -> PathBuf {
        self.root.join(scope)
    }

    fn file_of(&self, scope: &str, id: i64) -> PathBuf {
        self.scope_dir(scope).join(format!("{id}.json"))
    }

    fn seq_file(&self) -> PathBuf {
        self.root.join("_seq")
    }

    /// 读取 id 计数器(缺失/损坏视为 0)。
    pub(crate) fn read_seq(&self) -> i64 {
        fs::read_to_string(self.seq_file())
            .ok()
            .and_then(|t| t.trim().parse::<i64>().ok())
            .unwrap_or(0)
    }

    /// 分配下一个 id 并落计数器。调用方须持有写锁。
    pub(crate) fn next_id(&self) -> Result<i64, String> {
        let next = self.read_seq() + 1;
        fs::create_dir_all(&self.root).map_err(|e| format!("mkdir bank failed: {e}"))?;
        fs::write(self.seq_file(), next.to_string()).map_err(|e| format!("write _seq failed: {e}"))?;
        Ok(next)
    }

    /// 导入迁移时直接抬高计数器(不小于现有最大值)。
    pub(crate) fn bump_seq(&self, at_least: i64) -> Result<(), String> {
        if at_least > self.read_seq() {
            fs::create_dir_all(&self.root).map_err(|e| format!("mkdir bank failed: {e}"))?;
            fs::write(self.seq_file(), at_least.to_string())
                .map_err(|e| format!("write _seq failed: {e}"))?;
        }
        Ok(())
    }

    /// 读取一个 scope 目录下全部题目(忽略损坏文件,按 id 升序)。
    pub(crate) fn load_scope(&self, scope: &str) -> Vec<BankQuestion> {
        let dir = self.scope_dir(scope);
        let Ok(rd) = fs::read_dir(&dir) else {
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
            if let Ok(q) = serde_json::from_str::<BankQuestion>(&text) {
                out.push(q);
            } else {
                log::warn!("skip corrupted bank file: {}", p.display());
            }
        }
        out.sort_by_key(|q| q.id);
        out
    }

    /// 全部 scope 目录名(排除 `_seq` 等文件项)。
    fn scopes(&self) -> Vec<String> {
        let Ok(rd) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in rd.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                out.push(entry.file_name().to_string_lossy().to_string());
            }
        }
        out
    }

    /// 读取题库全部题目(按 id 升序)。
    pub(crate) fn load_all(&self) -> Vec<BankQuestion> {
        let mut out = Vec::new();
        for scope in self.scopes() {
            out.extend(self.load_scope(&scope));
        }
        out.sort_by_key(|q| q.id);
        out
    }

    /// 按 id 定位题目(扫描各 scope)。
    pub(crate) fn find(&self, id: i64) -> Option<BankQuestion> {
        self.load_all().into_iter().find(|q| q.id == id)
    }

    /// 写一道题(按 plan_id 决定 scope 目录)。
    pub(crate) fn write(&self, q: &BankQuestion) -> Result<(), String> {
        let scope = q
            .plan_id
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(MANUAL_SCOPE);
        let text = serde_json::to_string_pretty(q).map_err(|e| e.to_string())?;
        lc::atomic_write(&self.file_of(scope, q.id), &text)
    }

    /// 删除一道题(按题目自身归属定位文件)。
    pub(crate) fn remove(&self, q: &BankQuestion) -> Result<(), String> {
        let scope = q
            .plan_id
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(MANUAL_SCOPE);
        let f = self.file_of(scope, q.id);
        if f.exists() {
            fs::remove_file(&f).map_err(|e| format!("remove {} failed: {e}", f.display()))?;
        }
        Ok(())
    }

    /// 清空一个 scope 目录(删除计划时清理其全部题目)。
    pub(crate) fn remove_scope(&self, scope: &str) -> Result<(), String> {
        let dir = self.scope_dir(scope);
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| format!("remove {} failed: {e}", dir.display()))?;
        }
        Ok(())
    }
}

/// 默认引擎(生产路径 = learningcenter/bank)。
static DEFAULT: OnceLock<BankFs> = OnceLock::new();
pub(crate) fn default_fs() -> &'static BankFs {
    DEFAULT.get_or_init(|| BankFs::new(lc::bank_dir()))
}

/// 写操作互斥(保护 _seq 与「查重-写入」两步骤)。测试注入独立目录时也共享此锁,代价可忽略。
static WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
pub(crate) fn write_lock() -> Result<MutexGuard<'static, ()>, String> {
    WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::study::learn::bank::BankQuestion;

    fn tmp(case: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-tmp")
            .join("bankfs")
            .join(case);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn q(id: i64, plan_id: Option<&str>, stem: &str) -> BankQuestion {
        BankQuestion {
            id,
            plan_id: plan_id.map(String::from),
            node_id: None,
            node_name: "n".into(),
            subject: "s".into(),
            qtype: "choice".into(),
            stem: stem.into(),
            options: vec![],
            answer: "A".into(),
            explanation: String::new(),
            difficulty: 1,
            source: "quiz".into(),
            created: "2026-01-01".into(),
            updated: "2026-01-01".into(),
        }
    }

    #[test]
    fn write_find_remove_roundtrip() {
        let fs = BankFs::new(tmp("roundtrip"));
        let _g = write_lock().unwrap();
        let mut a = q(0, Some("p0000001"), "题一");
        a.id = fs.next_id().unwrap();
        fs.write(&a).unwrap();
        let b = q(fs.next_id().unwrap(), None, "题二");
        fs.write(&b).unwrap();

        assert_eq!(fs.read_seq(), 2);
        assert_eq!(fs.find(a.id).unwrap().stem, "题一");
        // 手动题落 manual 目录,计划题落计划目录
        assert!(fs.root.join("p0000001").join("1.json").exists());
        assert!(fs.root.join(MANUAL_SCOPE).join("2.json").exists());
        assert_eq!(fs.load_all().len(), 2);
        assert_eq!(fs.load_scope("p0000001").len(), 1);

        fs.remove(&a).unwrap();
        assert!(fs.find(a.id).is_none());
        // 删除不复用 id
        assert_eq!(fs.next_id().unwrap(), 3);

        fs.remove_scope("p0000001").unwrap();
        assert!(!fs.root.join("p0000001").exists());
        assert_eq!(fs.load_all().len(), 1);
    }

    #[test]
    fn corrupted_file_skipped() {
        let fs = BankFs::new(tmp("corrupt"));
        let dir = fs.root.join(MANUAL_SCOPE);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("9.json"), b"not json").unwrap();
        assert!(fs.load_all().is_empty(), "损坏文件被跳过而不是报错");
    }
}
