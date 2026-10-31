//! 学习中心数据根(`learningcenter`)的目录事实源与旧版数据迁移。
//!
//! 布局(全部在 `<data_dir>/learningcenter/` 下):
//!   plans/<plan_id>/{plan.json, trail.jsonl}   学习计划载荷与过程事件
//!   bank/<scope>/<id>.json                     试题库(scope = plan_id 或 manual)
//!   bank/_seq                                  题库 id 计数器(文本,单调递增)
//!   papers/<paper_id>.json                     试卷(元信息 + 题目快照 + 作答 + 批改)
//!   templates/templates.json                   提示词模板自定义覆盖
//!
//! 旧版位置(`<data_dir>/study/`,以及 db 中的 question_bank / study_papers /
//! study_paper_items 三表)在启动时一次性合并进来:目录改名、表数据导出为文件后删表。
//! 迁移幂等:已迁移过的环境重复执行是空操作。

use std::fs;
use std::path::{Path, PathBuf};

/// 学习中心根目录。
pub fn root() -> PathBuf {
    crate::paths::data_dir().join("learningcenter")
}

/// 学习计划目录 / 单个计划目录。
pub fn plans_dir() -> PathBuf {
    root().join("plans")
}
pub fn plan_dir(id: &str) -> PathBuf {
    plans_dir().join(id)
}

/// 试题库目录;其下按 scope(plan_id 或 `manual`,手动加题跨计划复用)各建子目录。
pub fn bank_dir() -> PathBuf {
    root().join("bank")
}

/// 试卷目录(单卷文件 <id>.json 由 store 拼装)。
pub fn papers_dir() -> PathBuf {
    root().join("papers")
}

/// 模板自定义覆盖文件。
pub fn templates_file() -> PathBuf {
    root().join("templates").join("templates.json")
}

/// 原子写:先写临时文件再 rename,避免读写并发时读到半截内容。
pub fn atomic_write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| format!("write {} failed: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("rename to {} failed: {e}", path.display()))
}

/// 移动文件/目录:同卷 rename 原子;跨卷回落复制 + 删除。
fn move_tree(from: &Path, to: &Path) -> Result<(), String> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if from.is_dir() {
        fs::create_dir_all(to).map_err(|e| format!("mkdir {} failed: {e}", to.display()))?;
        let rd = fs::read_dir(from).map_err(|e| format!("read {} failed: {e}", from.display()))?;
        for entry in rd {
            let entry = entry.map_err(|e| e.to_string())?;
            move_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        fs::remove_dir(from).map_err(|e| format!("remove {} failed: {e}", from.display()))?;
    } else {
        fs::copy(from, to).map_err(|e| format!("copy {} failed: {e}", from.display()))?;
        fs::remove_file(from).map_err(|e| format!("remove {} failed: {e}", from.display()))?;
    }
    Ok(())
}

/// 旧版目录迁移:`data/study/` → `learningcenter/`。幂等,启动时调用一次。
///
/// - `study/<id>/` → `learningcenter/plans/<id>/`(目标已存在则并集合并,新文件不覆盖旧);
/// - `study/templates.json` → `learningcenter/templates/templates.json`;
/// - 全部搬完后删除空的 `study/` 目录。
pub fn migrate_legacy_dirs() {
    let legacy = crate::paths::data_dir().join("study");
    if !legacy.is_dir() {
        return;
    }
    let rd = match fs::read_dir(&legacy) {
        Ok(r) => r,
        Err(e) => {
            log::error!("read legacy study dir failed: {e}");
            return;
        }
    };
    for entry in rd.flatten() {
        let src = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let dst = if src.is_dir() {
            plan_dir(&name)
        } else if name == "templates.json" {
            templates_file()
        } else {
            // 未知文件不猜不删,留在原处由用户处理
            log::warn!("legacy study entry kept in place: {}", src.display());
            continue;
        };
        if dst.exists() {
            // 目标已存在:目录做并集合并(不覆盖),文件保留目标、删除来源
            let r = if src.is_dir() {
                merge_dir(&src, &dst).and_then(|_| {
                    fs::remove_dir_all(&src).map_err(|e| format!("remove {} failed: {e}", src.display()))
                })
            } else {
                fs::remove_file(&src).map_err(|e| format!("remove {} failed: {e}", src.display()))
            };
            if let Err(e) = r {
                log::error!("merge legacy {} failed: {e}", src.display());
            }
            continue;
        }
        if let Err(e) = move_tree(&src, &dst) {
            log::error!("migrate legacy {} failed: {e}", src.display());
        }
    }
    // 只剩空目录(或无法识别的残留文件)时尝试清理;非空则保留
    let _ = fs::remove_dir(&legacy);
}

/// 并集合并 src 目录内容到 dst(不覆盖已存在文件)。
fn merge_dir(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("mkdir {} failed: {e}", dst.display()))?;
    let rd = fs::read_dir(src).map_err(|e| format!("read {} failed: {e}", src.display()))?;
    for entry in rd {
        let entry = entry.map_err(|e| e.to_string())?;
        let s = entry.path();
        let d = dst.join(entry.file_name());
        if d.exists() {
            continue;
        }
        move_tree(&s, &d)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(case: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-tmp")
            .join("lc")
            .join(case);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_roundtrip_and_no_tmp_left() {
        let dir = tmp("atomic");
        let f = dir.join("a.json");
        atomic_write(&f, "{\"x\":1}").unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), "{\"x\":1}");
        atomic_write(&f, "{\"x\":2}").unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), "{\"x\":2}");
        assert!(!dir.join("a.json.tmp").exists(), "临时文件不应残留");
    }

    #[test]
    fn move_tree_moves_dir() {
        let dir = tmp("mv");
        let from = dir.join("from");
        fs::create_dir_all(from.join("sub")).unwrap();
        fs::write(from.join("sub").join("f.txt"), b"hi").unwrap();
        let to = dir.join("to");
        move_tree(&from, &to).unwrap();
        assert!(!from.exists());
        assert_eq!(fs::read(to.join("sub").join("f.txt")).unwrap(), b"hi");
    }
}
