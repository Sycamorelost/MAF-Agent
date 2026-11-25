//! Markdown 块级解析入口:只产出与 Word 无关的块结构,便于单测与复用。
//!
//! 支持:标题、段落(软换行合并)、有序/无序列表(可嵌套)、管道表格(含对齐与题注)、
//! 围栏代码块、引用、图片、分隔线、分页符、摘要/关键词、参考文献。
//! 不支持:HTML 块、数学公式排版(公式在 `inline` 层做可读化兜底)。
//!
//! 职责拆分:本文件只做块循环与块类型定义;表格、列表、题注/标签识别
//! 分别位于 `table` / `list` / `caption` 子模块。

mod caption;
mod list;
mod table;

#[cfg(test)]
mod tests;

use super::inline;
pub(crate) use caption::caption_kind;
use caption::{image_line, is_caption, is_reference_heading, labeled_block};
use list::{list_marker, parse_list};
use table::{is_table_line, parse_table};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// 题注类型(与引用扫描共用 `caption::caption_kind`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptionKind {
    Figure,
    Table,
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
    /// 独立公式(`$$...$$`),由 docx 层转 OMML
    Math(String),
    /// 摘要正文(「摘要：…」/「Abstract: …」)
    Abstract(String),
    /// 关键词(「关键词：…」/「Keywords: …」)
    Keywords(String),
    /// 参考文献条目(「参考文献 / References」标题之后,逐条悬挂缩进)
    Reference(String),
    Rule,
    PageBreak,
}

pub fn parse(md: &str) -> Vec<Block> {
    let lines: Vec<&str> = md.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0usize;
    // 「参考文献」标题之后,正文段落/列表项统一按文献条目排版(悬挂缩进)
    let mut in_refs = false;

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
            in_refs = is_reference_heading(&text);
            blocks.push(Block::Heading { level, text });
            i += 1;
            continue;
        }

        // 独立公式:$$...$$(可跨行)
        if let Some(rest) = t.strip_prefix("$$") {
            let mut body = String::new();
            if rest.trim_end().ends_with("$$") && rest.len() >= 2 {
                body.push_str(rest.trim_end_matches('$'));
                i += 1;
            } else {
                if !rest.trim().is_empty() {
                    body.push_str(rest.trim());
                }
                i += 1;
                while i < lines.len() {
                    let cur = lines[i].trim();
                    if let Some(inner) = cur.strip_suffix("$$") {
                        if !inner.trim().is_empty() {
                            if !body.is_empty() {
                                body.push(' ');
                            }
                            body.push_str(inner.trim());
                        }
                        i += 1;
                        break;
                    }
                    if !body.is_empty() {
                        body.push(' ');
                    }
                    body.push_str(cur);
                    i += 1;
                }
            }
            let body = body.trim().to_string();
            if !body.is_empty() {
                blocks.push(Block::Math(body));
            }
            continue;
        }

        // 摘要 / 关键词:独立成段,使用不缩进的专用样式
        if let Some(block) = labeled_block(t) {
            blocks.push(block);
            i += 1;
            continue;
        }

        if in_refs {
            if list_marker(raw).is_some() {
                if let Block::List { items, .. } = parse_list(&lines, &mut i, true, 0) {
                    for item in items {
                        if !item.text.trim().is_empty() {
                            blocks.push(Block::Reference(item.text));
                        }
                    }
                }
                continue;
            }
            if !is_table_line(t) && image_line(t).is_none() {
                let mut text = String::new();
                while i < lines.len() {
                    let ct = lines[i].trim();
                    if ct.is_empty() || heading(ct).is_some() {
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
                    blocks.push(Block::Reference(text));
                }
                continue;
            }
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

fn is_block_start(t: &str, raw: &str) -> bool {
    is_fence(t)
        || is_page_break(t)
        || is_rule(t)
        || is_table_line(t)
        || t.starts_with('>')
        || heading(t).is_some()
        || list_marker(raw).is_some()
        || is_caption(t)
        || labeled_block(t).is_some()
        || t.starts_with("$$")
        || image_line(t).is_some()
}
