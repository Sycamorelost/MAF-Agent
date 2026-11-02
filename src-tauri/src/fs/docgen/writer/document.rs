//! 文档装配:封面、页眉页脚、块渲染与行内文本 run。

use std::path::Path;

use docx_rs::*;

use super::super::DocRequest;
use super::super::inline::{self, Emphasis};
use super::super::markdown::Block;
use super::super::style::{DocStyle, MARGIN_LR, MARGIN_TB, PAGE_H, PAGE_W};
use super::image::image_para;
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
            top: MARGIN_TB,
            left: MARGIN_LR,
            bottom: MARGIN_TB,
            right: MARGIN_LR,
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
    for block in &blocks {
        let brk = std::mem::take(&mut need_break);
        docx = add_block(docx, block, style, base, brk);
    }

    let mut cursor = std::io::Cursor::new(Vec::new());
    docx.build()
        .pack(&mut cursor)
        .map_err(|e| format!("生成 docx 失败: {e}"))?;
    Ok(cursor.into_inner())
}

// ---------------- 封面与页眉页脚 ----------------

fn add_cover(mut docx: Docx, req: &DocRequest, style: &DocStyle) -> Docx {
    // 用段前距推高标题,避免空段落堆叠
    docx = docx.add_paragraph(
        Paragraph::new()
            .line_spacing(LineSpacing::new().before(2200))
            .indent(Some(0), Some(SpecialIndentType::FirstLine(0)), None, None),
    );
    if !req.title.trim().is_empty() {
        docx = docx.add_paragraph(add_text_runs(
            Paragraph::new()
                .style("DocTitle")
                .align(AlignmentType::Center),
            &req.title,
            style,
        ));
    }
    if !req.subtitle.trim().is_empty() {
        docx = docx.add_paragraph(add_text_runs(
            Paragraph::new()
                .style("DocSubtitle")
                .align(AlignmentType::Center),
            &req.subtitle,
            style,
        ));
    }
    for (label, value) in req.meta.rows() {
        let text = format!("{label}：{value}");
        docx = docx.add_paragraph(add_text_runs(
            Paragraph::new()
                .style("MetaText")
                .align(AlignmentType::Center)
                .line_spacing(LineSpacing::new().before(160)),
            &text,
            style,
        ));
    }
    docx
}

fn header_para(style: &DocStyle, title: &str) -> Paragraph {
    Paragraph::new()
        .style("Normal")
        .align(AlignmentType::Right)
        .indent(Some(0), Some(SpecialIndentType::FirstLine(0)), None, None)
        .add_run(
            Run::new()
                .add_text(title.trim())
                .size(18)
                .color("808080")
                .fonts(style.body_fonts()),
        )
}

fn footer_para() -> Paragraph {
    Paragraph::new()
        .style("Normal")
        .align(AlignmentType::Center)
        .indent(Some(0), Some(SpecialIndentType::FirstLine(0)), None, None)
        .add_run(Run::new().add_text("第 ").size(18).color("808080"))
        .add_page_num(PageNum::new())
        .add_run(Run::new().add_text(" 页").size(18).color("808080"))
}

// ---------------- 块渲染 ----------------

fn add_block(mut docx: Docx, block: &Block, style: &DocStyle, base: &Path, brk: bool) -> Docx {
    match block {
        Block::Heading { level, text } => {
            let lvl = (*level).clamp(1, 4);
            docx = docx.add_paragraph(add_text_runs(
                Paragraph::new()
                    .style(&format!("Heading{lvl}"))
                    .page_break_before(brk),
                text,
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
                        .line_spacing(style.body_line_spacing())
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

fn add_text_runs(p: Paragraph, text: &str, style: &DocStyle) -> Paragraph {
    add_text_runs_with(p, text, style, false)
}

pub(super) fn add_text_runs_with(
    mut p: Paragraph,
    text: &str,
    style: &DocStyle,
    force_bold: bool,
) -> Paragraph {
    for seg in inline::parse_inline(text) {
        let mut run = Run::new().add_text(seg.text);
        run = match seg.emphasis {
            Emphasis::Bold => run.bold(),
            Emphasis::Italic => run.italic(),
            Emphasis::BoldItalic => run.bold().italic(),
            Emphasis::Strike => run.strike(),
            Emphasis::Code => run
                .fonts(style.mono_fonts())
                .shading(Shading::new().fill("F2F2F2")),
            Emphasis::Normal => run,
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
                    .fonts(style.mono_fonts())
                    .shading(shading.clone()),
            );
        }
        p = p.add_run(
            Run::new()
                .add_text(line)
                .fonts(style.mono_fonts())
                .shading(shading.clone()),
        );
    }
    p
}
