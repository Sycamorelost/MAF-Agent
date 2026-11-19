//! 管道表格解析:对齐标记、转义竖线、列补齐。
//!
//! 只产出中立的行/列数据,不关心列宽与渲染。

use super::{Align, Block};

pub(super) fn is_table_line(t: &str) -> bool {
    t.starts_with('|') && t.matches('|').count() >= 2
}

pub(super) fn parse_table(lines: &[&str], i: &mut usize, caption: Option<String>) -> Block {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut aligns: Option<Vec<Align>> = None;
    let mut idx = *i;
    while idx < lines.len() && is_table_line(lines[idx].trim()) {
        let cells = split_row(lines[idx].trim());
        if is_divider(&cells) {
            if aligns.is_none() {
                aligns = Some(cells.iter().map(|c| align_of(c)).collect());
            }
        } else {
            rows.push(cells);
        }
        idx += 1;
    }
    *i = idx;
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    for row in &mut rows {
        row.resize(cols, String::new());
    }
    let header = if rows.is_empty() {
        Vec::new()
    } else {
        rows.remove(0)
    };
    let mut aligns = aligns.unwrap_or_else(|| vec![Align::Left; cols]);
    aligns.resize(cols, Align::Left);
    Block::Table {
        caption,
        header,
        rows,
        aligns,
    }
}

fn split_row(line: &str) -> Vec<String> {
    let mut s = line.trim();
    if let Some(rest) = s.strip_prefix('|') {
        s = rest;
    }
    if s.ends_with('|') && !s.ends_with("\\|") {
        s = &s[..s.len() - 1];
    }
    let mut cells = Vec::new();
    let mut buf = String::new();
    let mut escaped = false;
    for c in s.chars() {
        if escaped {
            buf.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '|' => {
                cells.push(buf.trim().to_string());
                buf.clear();
            }
            _ => buf.push(c),
        }
    }
    cells.push(buf.trim().to_string());
    cells
}

fn is_divider(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|c| {
            let c = c.trim();
            if c.matches('-').count() < 3 {
                return false;
            }
            let body = c.trim_start_matches(':').trim_end_matches(':');
            !body.is_empty() && body.chars().all(|ch| ch == '-')
        })
}

fn align_of(cell: &str) -> Align {
    let c = cell.trim();
    match (c.starts_with(':'), c.ends_with(':')) {
        (true, true) => Align::Center,
        (false, true) => Align::Right,
        _ => Align::Left,
    }
}
