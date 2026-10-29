//! Markdown 块级解析层:只产出与 Word 无关的块结构,便于单测与复用。
//!
//! 支持:标题、段落(软换行合并)、有序/无序列表(可嵌套)、管道表格(含对齐与题注)、
//! 围栏代码块、引用、图片、分隔线、分页符。
//! 不支持:HTML 块、脚注、数学公式排版(公式在 `inline` 层做可读化兜底)。

use super::inline;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// 0 起,最多 3 级
    pub level: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: usize,
        text: String,
    },
    Paragraph(String),
    List {
        ordered: bool,
        items: Vec<ListItem>,
    },
    Table {
        caption: Option<String>,
        header: Vec<String>,
        rows: Vec<Vec<String>>,
        aligns: Vec<Align>,
    },
    Code {
        lang: String,
        lines: Vec<String>,
    },
    Quote(String),
    /// 独立题注(表格前的「表 1 …」会被并入 Table,其余保留)
    Caption(String),
    Image {
        alt: String,
        src: String,
    },
    Rule,
    PageBreak,
}

pub fn parse(md: &str) -> Vec<Block> {
    let lines: Vec<&str> = md.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        let raw = lines[i];
        let t = raw.trim();
        if t.is_empty() {
            i += 1;
            continue;
        }

        if is_fence(t) {
            let fence = t.chars().next().unwrap();
            let lang = t.trim_matches(fence).trim().to_string();
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() {
                if lines[i]
                    .trim_start()
                    .starts_with(&fence.to_string().repeat(3))
                {
                    i += 1;
                    break;
                }
                body.push(lines[i].trim_end().to_string());
                i += 1;
            }
            if !body.is_empty() {
                blocks.push(Block::Code { lang, lines: body });
            }
            continue;
        }

        if is_page_break(t) {
            blocks.push(Block::PageBreak);
            i += 1;
            continue;
        }
        if is_rule(t) {
            blocks.push(Block::Rule);
            i += 1;
            continue;
        }
        if let Some((level, text)) = heading(t) {
            blocks.push(Block::Heading { level, text });
            i += 1;
            continue;
        }
        if is_table_line(t) {
            blocks.push(parse_table(&lines, &mut i, None));
            continue;
        }
        if let Some((indent, ordered, _)) = list_marker(raw) {
            blocks.push(parse_list(&lines, &mut i, ordered, indent));
            continue;
        }
        if t.starts_with('>') {
            let mut text = String::new();
            while i < lines.len() {
                let ct = lines[i].trim();
                if !ct.starts_with('>') {
                    break;
                }
                let piece = ct.trim_start_matches('>').trim();
                text = if text.is_empty() {
                    piece.to_string()
                } else {
                    inline::join_soft(&text, piece)
                };
                i += 1;
            }
            if !text.is_empty() {
                blocks.push(Block::Quote(text));
            }
            continue;
        }
        if let Some((alt, src)) = image_line(t) {
            blocks.push(Block::Image { alt, src });
            i += 1;
            continue;
        }

        if is_caption(t) {
            // 「表 1 …」后紧跟表格时作为表题;否则保留为独立题注
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim().is_empty() {
                j += 1;
            }
            if j < lines.len() && is_table_line(lines[j].trim()) {
                let cap = t.to_string();
                i = j;
                blocks.push(parse_table(&lines, &mut i, Some(cap)));
            } else {
                blocks.push(Block::Caption(t.to_string()));
                i += 1;
            }
            continue;
        }

        // 普通段落:合并连续文本行,遇到空行或新的块起始行即结束
        let mut text = String::new();
        while i < lines.len() {
            let cur = lines[i];
            let ct = cur.trim();
            if ct.is_empty() || is_block_start(ct, cur) {
                break;
            }
            text = if text.is_empty() {
                ct.to_string()
            } else {
                inline::join_soft(&text, ct)
            };
            i += 1;
        }
        if !text.is_empty() {
            blocks.push(Block::Paragraph(text));
        }
    }
    blocks
}

fn is_fence(t: &str) -> bool {
    t.starts_with("```") || t.starts_with("~~~")
}

fn is_page_break(t: &str) -> bool {
    t == "\\pagebreak" || t.eq_ignore_ascii_case("<!-- pagebreak -->")
}

fn is_rule(t: &str) -> bool {
    let compact: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    compact.len() >= 3
        && compact
            .chars()
            .all(|c| c == compact.chars().next().unwrap())
        && matches!(compact.chars().next().unwrap(), '-' | '*' | '_')
}

fn heading(t: &str) -> Option<(usize, String)> {
    let n = t.chars().take_while(|&c| c == '#').count();
    if n == 0 || n > 6 {
        return None;
    }
    let rest = &t[n..];
    if !(rest.starts_with(' ') || rest.starts_with('\t')) {
        return None;
    }
    let text = rest.trim().trim_end_matches('#').trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some((n, text))
}

fn is_table_line(t: &str) -> bool {
    t.starts_with('|') && t.matches('|').count() >= 2
}

fn is_caption(t: &str) -> bool {
    let probe = t.trim_start_matches(['*', '_', ' ']).trim_start();
    for prefix in ["表", "图"] {
        if let Some(rest) = probe.strip_prefix(prefix)
            && rest
                .trim_start()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
        {
            return true;
        }
    }
    for prefix in ["Table", "Figure", "Fig."] {
        if let Some(rest) = probe.strip_prefix(prefix) {
            let rest = rest.trim_start();
            if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

fn image_line(t: &str) -> Option<(String, String)> {
    let rest = t.strip_prefix("![")?;
    let close = rest.find("](")?;
    let alt = rest[..close].to_string();
    let after = &rest[close + 2..];
    let end = after.find(')')?;
    Some((alt, after[..end].to_string()))
}

fn leading_spaces(line: &str) -> usize {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

/// 列表项:(缩进列数, 是否有序, 文本)
fn list_marker(line: &str) -> Option<(usize, bool, String)> {
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

fn parse_list(lines: &[&str], i: &mut usize, ordered: bool, base: usize) -> Block {
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
            if ct.is_empty() || is_block_start(ct, cont) {
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

fn parse_table(lines: &[&str], i: &mut usize, caption: Option<String>) -> Block {
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

fn is_block_start(t: &str, raw: &str) -> bool {
    is_fence(t)
        || is_page_break(t)
        || is_rule(t)
        || is_table_line(t)
        || t.starts_with('>')
        || heading(t).is_some()
        || list_marker(raw).is_some()
        || is_caption(t)
        || image_line(t).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headings_levels() {
        let blocks = parse("# 标题\n## 二级\n### 三级");
        assert_eq!(
            blocks,
            vec![
                Block::Heading {
                    level: 1,
                    text: "标题".into()
                },
                Block::Heading {
                    level: 2,
                    text: "二级".into()
                },
                Block::Heading {
                    level: 3,
                    text: "三级".into()
                },
            ]
        );
    }

    #[test]
    fn joins_soft_wrapped_paragraph_lines() {
        let blocks = parse("第一行\n第二行\n\n另一段");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], Block::Paragraph("第一行第二行".into()));
    }

    #[test]
    fn parses_nested_lists() {
        let blocks = parse("1. 第一步\n  - 子项\n2. 第二步");
        match &blocks[0] {
            Block::List { ordered, items } => {
                assert!(ordered);
                assert_eq!(items.len(), 3);
                assert_eq!(items[1].level, 1);
                assert_eq!(items[1].text, "子项");
            }
            other => panic!("expect list, got {other:?}"),
        }
    }

    #[test]
    fn parses_table_with_alignment_and_caption() {
        let md = "表 1 各轮结果\n| 模型 | 时间 | 得分 |\n|:---|:---:|---:|\n| A | 1.2 s | 90 |";
        let blocks = parse(md);
        match &blocks[0] {
            Block::Table {
                caption,
                header,
                rows,
                aligns,
            } => {
                assert_eq!(caption.as_deref(), Some("表 1 各轮结果"));
                assert_eq!(header.len(), 3);
                assert_eq!(rows.len(), 1);
                assert_eq!(aligns, &[Align::Left, Align::Center, Align::Right]);
            }
            other => panic!("expect table, got {other:?}"),
        }
    }

    #[test]
    fn parses_fenced_code() {
        let blocks = parse("```python\nprint(1)\n```");
        assert_eq!(
            blocks[0],
            Block::Code {
                lang: "python".into(),
                lines: vec!["print(1)".into()]
            }
        );
    }

    #[test]
    fn parses_image_and_page_break() {
        let blocks = parse("![架构图](img/a.png)\n\n\\pagebreak\n\n正文");
        assert_eq!(
            blocks[0],
            Block::Image {
                alt: "架构图".into(),
                src: "img/a.png".into()
            }
        );
        assert_eq!(blocks[1], Block::PageBreak);
    }

    #[test]
    fn caption_before_table_is_attached() {
        let blocks = parse("表 2-1 参数\n\n| a | b |\n| --- | --- |\n| 1 | 2 |");
        assert_eq!(blocks.len(), 1);
        assert!(matches!(blocks[0], Block::Table { .. }));
    }

    #[test]
    fn standalone_caption_is_kept() {
        let blocks = parse("图 1 界面截图");
        assert_eq!(blocks[0], Block::Caption("图 1 界面截图".into()));
    }
}
