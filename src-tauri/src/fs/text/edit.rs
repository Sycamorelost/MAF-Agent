//! 精确编辑(局部替换)。

use serde::Serialize;
use std::path::Path;

use crate::fs::{backup, paths};

use super::common::{audit, count_lines, write_atomic};
use super::io::root_for_session;

// ---------------- 精确编辑(局部替换) ----------------

#[derive(Debug, Clone, Serialize)]
pub struct EditResult {
    pub rel: String,
    /// 实际替换处数
    pub replaced: usize,
    pub bytes: u64,
    /// 编辑后的总行数(供模型核对)
    pub lines: u64,
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
    edit_text_in(&root, rel, old, new, replace_all, &backup::backup_dir())
}

pub(crate) fn edit_text_in(
    root: &Path,
    rel: &str,
    old: &str,
    new: &str,
    replace_all: bool,
    backup_base: &Path,
) -> Result<EditResult, String> {
    if old.is_empty() {
        return Err("old 不能为空:请给出待替换的原文片段".into());
    }
    let path = paths::resolve_guarded(root, rel)?;
    if !path.is_file() {
        return Err(format!(
            "`{rel}` 不是文件(精确编辑需目标已存在,新建请用 fs_write)"
        ));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("读取失败: {e}"))?;
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return Err(format!("`{rel}` 看起来是二进制文件,无法精确编辑"));
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| format!("`{rel}` 不是合法 UTF-8 文本,无法精确替换;请用 fs_write 整体重写"))?;
    let count = text.matches(old).count();
    let (updated, replaced) = if count > 0 {
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
        (updated, if replace_all { count } else { 1 })
    } else {
        // 精确匹配失败:退化为「整行宽松匹配」(忽略行尾空白/CRLF/弯引号/破折号差异),
        // 避免模型因排版字符差异陷入反复重试。
        let windows = fuzzy_windows(&text, old);
        if windows.is_empty() {
            return Err(format!(
                "未在 `{rel}` 中找到待替换片段(区分大小写与空白);{}。请先用 fs_read 读取原文,再逐字给出完全一致的片段",
                nearest_line_hint(&text, old)
            ));
        }
        if windows.len() > 1 && !replace_all {
            return Err(format!(
                "待替换片段在 `{rel}` 中宽松匹配到 {} 处(行尾空白/引号/破折号差异已忽略);请提供更长、更唯一的上下文,或设置 replace_all=true",
                windows.len()
            ));
        }
        let picked: &[(usize, usize)] = if replace_all {
            &windows
        } else {
            &windows[..1]
        };
        (splice_windows(&text, picked, new), picked.len())
    };
    let backup_id = backup::snapshot_in(backup_base, root, &path, None, None)?;
    write_atomic(&path, updated.as_bytes())?;
    audit("fs_edit", rel, updated.len());
    Ok(EditResult {
        rel: rel.to_string(),
        replaced,
        bytes: updated.len() as u64,
        lines: count_lines(&updated),
        backup_id,
    })
}

/// 行尾空白、CRLF、弯引号与破折号归一化(只用于宽松匹配,不回写归一化结果)。
fn norm_line(s: &str) -> String {
    s.trim_end()
        .chars()
        .map(|c| match c {
            '“' | '”' => '"',
            '‘' | '’' => '\'',
            '—' | '–' | '－' => '-',
            '\u{00A0}' => ' ',
            _ => c,
        })
        .collect()
}

/// 按「整行窗口」在原文中宽松定位,返回匹配到的字节区间(含行尾换行)。
fn fuzzy_windows(text: &str, old: &str) -> Vec<(usize, usize)> {
    let old_lines: Vec<String> = old
        .split_inclusive('\n')
        .map(|l| norm_line(l.trim_end_matches(['\r', '\n'])))
        .collect();
    if old_lines.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<(usize, usize, String)> = Vec::new();
    let mut off = 0usize;
    for l in text.split_inclusive('\n') {
        let end = off + l.len();
        lines.push((off, end, norm_line(l.trim_end_matches(['\r', '\n']))));
        off = end;
    }
    if old_lines.len() > lines.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + old_lines.len() <= lines.len() {
        if (0..old_lines.len()).all(|j| lines[i + j].2 == old_lines[j]) {
            let start = lines[i].0;
            let mut end = lines[i + old_lines.len() - 1].1;
            // old 本身不含结尾换行时,窗口也排除行尾换行,避免替换后把下一行并上来
            if !old.ends_with('\n') {
                if text[..end].ends_with("\r\n") {
                    end -= 2;
                } else if text[..end].ends_with('\n') {
                    end -= 1;
                }
            }
            out.push((start, end));
            // 窗口互不重叠,避免 replace_all 时拼接越界
            i += old_lines.len();
            continue;
        }
        i += 1;
    }
    out
}

/// 用 `new` 替换若干个已定位的区间(游标顺序拼接,兼容多窗口)。
/// 原文区间是 CRLF 时,把 `new` 的换行也统一成 CRLF。
fn splice_windows(text: &str, windows: &[(usize, usize)], new: &str) -> String {
    let crlf = windows
        .iter()
        .any(|&(s, e)| text[s..e].contains("\r\n"));
    let block = if crlf {
        new.replace("\r\n", "\n").replace('\n', "\r\n")
    } else {
        new.to_string()
    };
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    for &(s, e) in windows {
        out.push_str(&text[last..s]);
        out.push_str(&block);
        last = e;
    }
    out.push_str(&text[last..]);
    out
}

/// 匹配失败时给出最接近的行号与内容,减少模型盲试。
fn nearest_line_hint(text: &str, old: &str) -> String {
    let target = old
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or(old)
        .trim()
        .to_string();
    let mut best: Option<(usize, usize, &str)> = None;
    for (i, line) in text.lines().enumerate() {
        let score = common_prefix_chars(&target, line.trim());
        if best.map_or(true, |(_, b, _)| score > b) {
            best = Some((i + 1, score, line));
        }
    }
    match best {
        Some((n, score, line)) if score >= 3 => {
            format!("最接近的是第 {n} 行:`{}`", truncate_chars(line.trim(), 80))
        }
        Some((n, _, line)) => format!(
            "可先查看第 {n} 行附近:`{}`",
            truncate_chars(line.trim(), 80)
        ),
        None => "文件为空".into(),
    }
}

fn common_prefix_chars(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}
