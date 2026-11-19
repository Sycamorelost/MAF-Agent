//! 列表解析:缩进层级、软换行合并、空行续接。

use super::{Block, ListItem};
use crate::fs::docgen::inline;

pub(super) fn leading_spaces(line: &str) -> usize {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

/// 列表项:(缩进列数, 是否有序, 文本)
pub(super) fn list_marker(line: &str) -> Option<(usize, bool, String)> {
    let indent = leading_spaces(line);
    let rest = line.trim_start();
    if let Some(marker) = ['-', '*', '+']
        .into_iter()
        .find(|m| rest.starts_with(&format!("{m} ")))
    {
        return Some((indent, false, rest[marker.len_utf8()..].trim().to_string()));
    }
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && digits <= 3 {
        let tail = &rest[digits..];
        if tail.starts_with(". ") || tail.starts_with(") ") {
            return Some((indent, true, tail[2..].trim().to_string()));
        }
    }
    None
}

pub(super) fn parse_list(lines: &[&str], i: &mut usize, ordered: bool, base: usize) -> Block {
    let mut items: Vec<ListItem> = Vec::new();
    let mut idx = *i;
    while idx < lines.len() {
        let raw = lines[idx];
        if raw.trim().is_empty() {
            // 空行不立即结束列表:下一行仍是同类列表项则继续
            let mut k = idx + 1;
            while k < lines.len() && lines[k].trim().is_empty() {
                k += 1;
            }
            let continues = k < lines.len()
                && list_marker(lines[k])
                    .map(|(ind, ord, _)| ord == ordered && ind >= base)
                    .unwrap_or(false);
            if !continues {
                break;
            }
            idx = k;
            continue;
        }
        let Some((indent, ord, text)) = list_marker(raw) else {
            break;
        };
        if ord != ordered && indent <= base {
            break;
        }
        let level = indent.saturating_sub(base) / 2;
        let level = level.min(3);
        let mut item = text;
        idx += 1;
        // 缩进更深的普通行并入当前项(软换行)
        while idx < lines.len() {
            let cont = lines[idx];
            let ct = cont.trim();
            if ct.is_empty() || super::is_block_start(ct, cont) {
                break;
            }
            if leading_spaces(cont) <= indent {
                break;
            }
            item = inline::join_soft(&item, ct);
            idx += 1;
        }
        items.push(ListItem { level, text: item });
    }
    *i = idx;
    Block::List { ordered, items }
}
