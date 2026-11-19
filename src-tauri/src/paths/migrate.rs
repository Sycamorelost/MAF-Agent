//! 数据目录迁移:复制 → 校验 → 重写附件路径 → 删除旧目录(幂等,可重试)。

use super::{config_path, resolve};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
};

/// 递归复制目录(跳过符号链接)。
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("mkdir {} failed: {e}", to.display()))?;
    let rd = fs::read_dir(from).map_err(|e| format!("read {} failed: {e}", from.display()))?;
    for entry in rd {
        let entry = entry.map_err(|e| e.to_string())?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        let ft = entry.file_type().map_err(|e| e.to_string())?;
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            fs::copy(&src, &dst).map_err(|e| format!("copy {} failed: {e}", src.display()))?;
        }
    }
    Ok(())
}

/// 目录快照:相对路径(统一 `/` 分隔) → 字节数。
fn snapshot(dir: &Path) -> Result<BTreeMap<String, u64>, String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, u64>) -> Result<(), String> {
        let rd = fs::read_dir(dir).map_err(|e| format!("read {} failed: {e}", dir.display()))?;
        for entry in rd {
            let entry = entry.map_err(|e| e.to_string())?;
            let p = entry.path();
            let ft = entry.file_type().map_err(|e| e.to_string())?;
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                walk(base, &p, out)?;
            } else {
                let rel = p
                    .strip_prefix(base)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                let size = entry.metadata().map_err(|e| e.to_string())?.len();
                out.insert(rel, size);
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    if dir.is_dir() {
        walk(dir, dir, &mut out)?;
    }
    Ok(out)
}

/// 数据目录迁移后,把 `messages.attachments` JSON 里的绝对路径前缀从旧目录改为新目录。
/// 附件路径在库里是绝对路径;不重写的话迁移后图片/附件会全部失效(R8)。
/// 具体改写逻辑在 `doc::service::rewrite_attachment_paths`(文档生命周期统一入口),
/// 这里只负责打开库文件与容错:非 SQLite 文件(如测试占位)静默跳过,不阻断迁移。
fn rewrite_attachment_prefix(db: &Path, old_root: &Path, new_root: &Path) -> Result<usize, String> {
    if !db.is_file() {
        return Ok(0);
    }
    let conn = match rusqlite::Connection::open(db) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("skip attachment path rewrite, open db failed: {e}");
            return Ok(0);
        }
    };
    let old = old_root.to_string_lossy().to_string();
    let new = new_root.to_string_lossy().to_string();
    match crate::doc::service::rewrite_attachment_paths(&conn, &old, &new) {
        Ok(n) => Ok(n),
        Err(e) => {
            log::warn!("skip attachment path rewrite, rewrite failed: {e}");
            Ok(0)
        }
    }
}

/// 把 `from` 的数据迁移到 `to`:复制 → 校验 → 重写附件路径 → 删除旧目录。幂等,可重试。
pub(super) fn migrate_dir(from: &Path, to: &Path) -> Result<(), String> {
    if !from.exists() {
        return Ok(());
    }
    let cf = resolve::canon(from);
    let ct = resolve::canon(to);
    if cf.starts_with(&ct) || ct.starts_with(&cf) {
        return Err("新旧目录互相包含,已中止迁移".into());
    }
    if to.join("metaagent.db").exists() && from.join("metaagent.db").exists() {
        return Err("目标目录已存在数据库文件,已中止迁移".into());
    }

    copy_tree(from, to)?;

    // 校验:源目录每个文件都在目标目录里且大小一致
    let src = snapshot(from)?;
    let dst = snapshot(to)?;
    for (rel, size) in &src {
        match dst.get(rel) {
            Some(s) if s == size => {}
            _ => return Err(format!("迁移校验失败,文件缺失或大小不一致:{rel}")),
        }
    }

    // 附件是绝对路径:迁移后必须重写,否则旧目录一删附件就全部失效
    let rewritten = rewrite_attachment_prefix(&to.join("metaagent.db"), from, to)?;
    if rewritten > 0 {
        log::info!("rewrote attachment paths for {rewritten} messages");
    }

    // 旧目录保护:绝不删除引导配置所在目录或其祖先
    let cp = resolve::canon(&config_path());
    if !cp.starts_with(&cf) {
        fs::remove_dir_all(from).map_err(|e| format!("删除旧目录失败:{e}"))?;
    }
    Ok(())
}
