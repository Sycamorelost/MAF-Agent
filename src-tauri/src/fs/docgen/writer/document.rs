//! 文档装配:封面、页眉页脚、块渲染与行内文本 run。

use std::path::Path;

use docx_rs::*;

use super::super::DocRequest;
use super::super::inline::{self, Emphasis};
use super::super::markdown::Block;
use super::super::style::{DocStyle, HeadingNumbering, PAGE_H, PAGE_W};
use super::super::templates::numbering;
use super::image::image_para;
use super::front_matter::{add_cover, footer_para, header_para};
use super::metrics;
use super::styles::styles_for;
use super::table::table_block;

pub fn render(req: &DocRequest, style: &DocStyle, base: &Path) -> Result<Vec<u8>, String> {
    let blocks = super::super::markdown::parse(&req.body);
    let cover = req.has_cover();
    let has_headings = blocks
        .iter()
        .any(|b| matches!(b, Block::Heading { level, .. } if *level <= 3));
    let toc = req.want_toc() && has_headings;

    let mut docx = Docx::new()
        .page_size(PAGE_W, PAGE_H)
        .page_margin(PageMargin {
            top: style.margin_top,
            left: style.margin_left,
            bottom: style.margin_bottom,
            right: style.margin_right,
            header: 851,
            footer: 992,
            gutter: 0,
        })
        .styles(styles_for(style))
        .footer(Footer::new().add_paragraph(footer_para()));

    if !req.title.trim().is_empty() {
        docx = docx.header(Header::new().add_paragraph(header_para(style, &req.title)));
    }
    if cover {
        // 封面不显示页眉页脚
        docx = docx
            .first_header(Header::new())
            .first_footer(Footer::new())
            .title_pg();
    }

    if cover {
        docx = add_cover(docx, req, style);
    }
    if toc {
        docx = docx.add_paragraph(
            Paragraph::new()
                .style("TocHeading")
                .align(AlignmentType::Center)
                .page_break_before(cover)
                .add_run(Run::new().add_text("目　录")),
        );
        docx = docx.add_table_of_contents(TableOfContents::new().heading_styles_range(1, 3));
    }

    let mut need_break = cover || toc;
    let prefixes = heading_prefixes(&blocks, style.numbering);
    for (idx, block) in blocks.iter().enumerate() {
        let brk = std::mem::take(&mut need_break);
        docx = add_block(docx, block, style, base, brk, prefixes[idx].as_deref());
    }

    let mut cursor = std::io::Cursor::new(Vec::new());
    docx.build()
        .pack(&mut cursor)
        .map_err(|e| format!("生成 docx 失败: {e}"))?;
    // 把正文里的公式占位符替换为 OMML(docx-rs 本身不支持公式)
    super::super::math_docx::inject_docx_math(cursor.into_inner())
}

// ---------------- 块渲染 ----------------

/// 预计算每级标题编号;用户已手写编号的标题返回 None。
fn heading_prefixes(blocks: &[Block], kind: HeadingNumbering) -> Vec<Option<String>> {
    if kind == HeadingNumbering::None {
        return vec![None; blocks.len()];
    }
    let mut counters = [0usize; 4];
    // 以正文中最浅的标题层级作为编号第 1 级(常见写法从 `##` 开始)
    let min_level = blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading { level, .. } if *level <= 4 => Some(*level),
            _ => None,
        })
        .min()
        .unwrap_or(1);
    blocks
        .iter()
        .map(|b| match b {
            Block::Heading { level, text } if *level <= 4 => {
                let l = level.saturating_sub(min_level).min(3);
                counters[l] += 1;
                for c in counters.iter_mut().skip(l + 1) {
                    *c = 0;
                }
                if numbering::has_manual(text) {
                    None
                } else {
                    Some(numbering::prefix(kind, &counters, l + 1))
                }
            }
            _ => None,
        })
        .collect()
}

fn add_block(
    mut docx: Docx,
    block: &Block,
    style: &DocStyle,
    base: &Path,
    brk: bool,
    num: Option<&str>,
) -> Docx {
    match block {
        Block::Heading { level, text } => {
            let lvl = (*level).clamp(1, 4);
            let numbered = match num {
                Some(p) => format!("{p}{text}"),
                None => text.clone(),
            };
            docx = docx.add_paragraph(add_text_runs(
                Paragraph::new()
                    .style(&format!("Heading{lvl}"))
                    .page_break_before(brk),
                &numbered,
                style,
            ));
        }
        Block::Paragraph(text) => {
            docx = docx.add_paragraph(add_text_runs(
                Paragraph::new().style("BodyText").page_break_before(brk),
                text,
                style,
            ));
        }
        Block::Quote(text) => {
            docx = docx.add_paragraph(add_text_runs(
                Paragraph::new().style("QuoteText").page_break_before(brk),
                text,
                style,
            ));
        }
        Block::Abstract(text) | Block::Keywords(text) => {
            let label = if matches!(block, Block::Abstract(_)) {
                "摘要："
            } else {
                "关键词："
            };
            let p = Paragraph::new()
                .style(if matches!(block, Block::Abstract(_)) {
                    "AbstractText"
                } else {
                    "KeywordsText"
                })
                .page_break_before(brk)
                .add_run(
                    Run::new()
                        .add_text(label)
                        .bold()
                        .fonts(metrics::heading_fonts(style)),
                );
            docx = docx.add_paragraph(add_text_runs(p, text, style));
        }
        Block::Reference(text) => {
            docx = docx.add_paragraph(add_text_runs(
                Paragraph::new().style("ReferenceText").page_break_before(brk),
                text,
                style,
            ));
        }
        Block::Math(latex) => {
            let token = super::super::math::token(latex, true);
            docx = docx.add_paragraph(
                Paragraph::new()
                    .style("MathBlock")
                    .page_break_before(brk)
                    .add_run(Run::new().add_text(token)),
            );
        }
        Block::List { ordered, items } => {
            let base_indent = if style.body_indent > 0 {
                style.body_indent
            } else {
                360
            };
            let mut counters = [0usize; 4];
            for (idx, item) in items.iter().enumerate() {
                let lvl = item.level.min(3);
                counters[lvl] += 1;
                for c in counters.iter_mut().skip(lvl + 1) {
                    *c = 0;
                }
                let marker = if *ordered {
                    format!("{}. ", counters[lvl])
                } else {
                    "• ".to_string()
                };
                let left = base_indent + 420 * lvl as i32;
                docx = docx.add_paragraph(add_text_runs(
                    Paragraph::new()
                        .style("BodyText")
                        .align(AlignmentType::Left)
                        .page_break_before(brk && idx == 0)
                        .line_spacing(metrics::body_line_spacing(style))
                        .indent(
                            Some(left),
                            Some(SpecialIndentType::Hanging(420)),
                            None,
                            None,
                        ),
                    &format!("{marker}{}", item.text),
                    style,
                ));
            }
        }
        Block::Table {
            caption,
            header,
            rows,
            aligns,
        } => {
            if header.is_empty() && rows.is_empty() {
                return docx;
            }
            if let Some(cap) = caption {
                docx = docx.add_paragraph(
                    add_text_runs(
                        Paragraph::new().style("Caption").keep_next(true),
                        cap,
                        style,
                    )
                    .page_break_before(brk),
                );
            } else if brk {
                docx = docx.add_paragraph(
                    Paragraph::new().page_break_before(true).line_spacing(
                        LineSpacing::new()
                            .line_rule(LineSpacingType::Exact)
                            .line(20),
                    ),
                );
            }
            docx = docx.add_table(table_block(header, rows, aligns, style));
        }
        Block::Code { lines, .. } => {
            docx = docx.add_paragraph(code_para(lines, style).page_break_before(brk));
        }
        Block::Caption(text) => {
            docx = docx.add_paragraph(add_text_runs(
                Paragraph::new().style("Caption").page_break_before(brk),
                text,
                style,
            ));
        }
        Block::Image { alt, src } => match image_para(base, src, brk) {
            Some(p) => {
                docx = docx.add_paragraph(p);
                if !alt.trim().is_empty() {
                    docx = docx.add_paragraph(add_text_runs(
                        Paragraph::new().style("Caption"),
                        alt,
                        style,
                    ));
                }
            }
            None => {
                docx = docx.add_paragraph(add_text_runs(
                    Paragraph::new().style("BodyText").page_break_before(brk),
                    &format!("［图片未找到：{src}］"),
                    style,
                ));
            }
        },
        Block::Rule => {
            let border = ParagraphBorder::new(ParagraphBorderPosition::Bottom)
                .val(BorderType::Single)
                .size(6)
                .space(1)
                .color("BFBFBF");
            docx = docx.add_paragraph(
                Paragraph::new()
                    .page_break_before(brk)
                    .line_spacing(LineSpacing::new().before(60).after(60))
                    .set_borders(ParagraphBorders::with_empty().set(border)),
            );
        }
        Block::PageBreak => {
            docx =
                docx.add_paragraph(Paragraph::new().add_run(Run::new().add_break(BreakType::Page)));
        }
    }
    docx
}

pub(super) fn add_text_runs(p: Paragraph, text: &str, style: &DocStyle) -> Paragraph {
    add_text_runs_with(p, text, style, false)
}

pub(super) fn add_text_runs_with(
    mut p: Paragraph,
    text: &str,
    style: &DocStyle,
    force_bold: bool,
) -> Paragraph {
    for seg in inline::parse_inline(text) {
        // 公式:写入占位符,打包后由 math_docx::inject_docx_math 换 OMML
        if seg.emphasis == Emphasis::Math {
            p = p.add_run(Run::new().add_text(super::super::math::token(&seg.text, false)));
            continue;
        }
        let mut run = Run::new().add_text(seg.text);
        run = match seg.emphasis {
            Emphasis::Bold => run.bold(),
            Emphasis::Italic => run.italic(),
            Emphasis::BoldItalic => run.bold().italic(),
            Emphasis::Strike => run.strike(),
            Emphasis::Code => run
                .fonts(metrics::mono_fonts(style))
                .shading(Shading::new().fill("F2F2F2")),
            Emphasis::Normal | Emphasis::Math => run,
        };
        if force_bold {
            run = run.bold();
        }
        p = p.add_run(run);
    }
    p
}

/// 代码块:单段落 + 行内换行 + 浅灰底纹,避免逐行段落带来的行距缝隙。
fn code_para(lines: &[String], style: &DocStyle) -> Paragraph {
    let mut p = Paragraph::new().style("SourceCode");
    let shading = Shading::new().fill("F5F5F5");
    for (idx, line) in lines.iter().enumerate() {
        if idx > 0 {
            p = p.add_run(
                Run::new()
                    .add_break(BreakType::TextWrapping)
                    .fonts(metrics::mono_fonts(style))
                    .shading(shading.clone()),
            );
        }
        p = p.add_run(
            Run::new()
                .add_text(line)
                .fonts(metrics::mono_fonts(style))
                .shading(shading.clone()),
        );
    }
    p
}
