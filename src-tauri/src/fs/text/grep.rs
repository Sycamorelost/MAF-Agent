//! 文本检索(仅文本类扩展名,单文件上限 2MB),返回「文件:行号:内容」。

use std::path::{Path, PathBuf};

use crate::fs::paths;

use super::common::display_path;

/// 文本检索(仅文本类扩展名,单文件上限 2MB),返回「文件:行号:内容」。
/// 绝对路径直接检索;相对路径以工作区为基准。
pub fn grep(
    pattern: &str,
    rel: &str,
    limit: usize,
    session_id: Option<&str>,
) -> Result<Vec<String>, String> {
    grep_in(&paths::resolve_workspace_root(session_id)?, pattern, rel, limit)
}

pub(crate) fn grep_in(root: &Path, pattern: &str, rel: &str, limit: usize) -> Result<Vec<String>, String> {
    let needle = pattern.trim().to_lowercase();
    if needle.is_empty() {
        return Err("pattern 不能为空".into());
    }
    let start = if rel.trim().is_empty() {
        root.to_path_buf()
    } else {
        paths::resolve(root, rel, true)?
    };
    let limit = limit.clamp(1, 200);
    let mut out = Vec::new();
    if start.is_file() {
        scan_file(&start, root, &needle, limit, &mut out);
    } else {
        walk_grep(&start, root, &needle, limit, &mut out)?;
    }
    Ok(out)
}

/// 单文件检索(跳过 >2MB 与含 NUL 的二进制内容)。
fn scan_file(path: &Path, root: &Path, needle: &str, limit: usize, out: &mut Vec<String>) {
    if out.len() >= limit {
        return;
    }
    let Ok(meta) = path.metadata() else {
        return;
    };
    if meta.len() > 2 * 1024 * 1024 {
        return;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    if bytes[..bytes.len().min(4096)].contains(&0) {
        return;
    }
    let text = String::from_utf8_lossy(&bytes);
    let rel_path = paths::rel_of(root, path).unwrap_or_else(|_| display_path(path));
    for (i, line) in text.lines().enumerate() {
        if line.to_lowercase().contains(needle) {
            let line = crate::mcp::safety::truncate_chars(line.trim(), 300);
            out.push(format!("{rel_path}:{}:{}", i + 1, line));
            if out.len() >= limit {
                break;
            }
        }
    }
}

/// 递归检索:跳过超过 2MB 的文件与已知二进制扩展名。
fn walk_grep(
    dir: &Path,
    root: &Path,
    needle: &str,
    limit: usize,
    out: &mut Vec<String>,
) -> Result<(), String> {
    if out.len() >= limit {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("读取目录失败: {e}"))?
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if out.len() >= limit {
            break;
        }
        if path.is_dir() {
            walk_grep(&path, root, needle, limit, out)?;
            continue;
        }
        scan_file(&path, root, needle, limit, out);
    }
    Ok(())
}

