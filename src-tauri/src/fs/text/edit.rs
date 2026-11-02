//! 精确编辑(局部替换)。

use serde::Serialize;
use std::path::Path;

use crate::fs::{backup, paths};

use super::common::{audit, session_id_is_bound_to_project};
use super::io::root_for_session;

// ---------------- 精确编辑(局部替换) ----------------

#[derive(Debug, Clone, Serialize)]
pub struct EditResult {
    pub rel: String,
    /// 实际替换处数
    pub replaced: usize,
    pub bytes: u64,
    pub backup_id: Option<String>,
}

/// 精确替换文件中的一段原文。比 fs_write 整体重写更省 token、也更不容易误伤其他内容。
/// 片段出现 0 次或多次(未开 replace_all)时直接报错,要求模型给出更精确的上下文。
pub fn edit_text(
    rel: &str,
    old: &str,
    new: &str,
    replace_all: bool,
    session_id: Option<&str>,
) -> Result<EditResult, String> {
    let root = root_for_session(session_id)?;
    let enforce = session_id_is_bound_to_project(session_id);
    edit_text_in(
        &root,
        rel,
        old,
        new,
        replace_all,
        &backup::backup_dir(),
        enforce,
    )
}

pub(crate) fn edit_text_in(
    root: &Path,
    rel: &str,
    old: &str,
    new: &str,
    replace_all: bool,
    backup_base: &Path,
    enforce: bool,
) -> Result<EditResult, String> {
    if old.is_empty() {
        return Err("old 不能为空:请给出待替换的原文片段".into());
    }
    let path = if enforce {
        paths::ensure_within(root, Path::new(rel))?
    } else {
        paths::resolve(root, rel, true)?
    };
    if !path.is_file() {
        return Err(format!(
            "`{rel}` 不是文件(精确编辑需目标已存在,新建请用 fs_write)"
        ));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("读取失败: {e}"))?;
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return Err(format!("`{rel}` 看起来是二进制文件,无法精确编辑"));
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        format!("`{rel}` 不是合法 UTF-8 文本,无法精确替换;请用 fs_write 整体重写")
    })?;
    let count = text.matches(old).count();
    if count == 0 {
        return Err(format!(
            "未在 `{rel}` 中找到待替换片段(区分大小写与空白)。请先用 fs_read 读取原文,再逐字给出完全一致的片段"
        ));
    }
    if count > 1 && !replace_all {
        return Err(format!(
            "待替换片段在 `{rel}` 中出现 {count} 次;请提供更长、更唯一的上下文,或设置 replace_all=true 全部替换"
        ));
    }
    let updated = if replace_all {
        text.replace(old, new)
    } else {
        text.replacen(old, new, 1)
    };
    let backup_id = backup::snapshot_in(backup_base, root, &path, None, None)?;
    std::fs::write(&path, updated.as_bytes()).map_err(|e| format!("写入失败: {e}"))?;
    audit("fs_edit", rel, updated.len());
    Ok(EditResult {
        rel: rel.to_string(),
        replaced: if replace_all { count } else { 1 },
        bytes: updated.len() as u64,
        backup_id,
    })
}

