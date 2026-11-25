//! 块渲染:标题/段落/列表/表格/公式/图片/题注 → docx-rs 元素。
//!
//! 只负责「块 → 段落」,不负责封面/目录/分节装配;书签与交叉引用在这里挂载。

use std::path::Path;

use docx_rs::*;

use super::super::inline::{self, Emphasis};
use super::super::markdown::Block;
use super::super::refs::{Label, LabelKind};
use super::super::style::DocStyle;
use super::field;
use super::image::image_para;
use super::metrics;
use super::numbering::{self, NumberingIds};
use super::table::table_block;

/// 渲染参数(块级共享)。
pub(super) struct Ctx<'a> {
    pub style: &'a DocStyle,
    pub base: &'a Path,
    pub ids: NumberingIds,
    /// 是否启用原生编号(thesis/book)
    pub native: bool,
}

pub(super) fn add_block(
    mut docx: Docx,
    block: &Block,
    ctx: &Ctx<'_>,
    brk: bool,
    num: Option<&str>,
    labels: &[Label],
) -> Docx {
    let style = ctx.style;
    let bookmark_ids: Vec<usize> = labels.iter().map(bookmark_id).collect();
    for (label, id) in labels.iter().zip(&bookmark_ids) {
        docx = docx.add_bookmark_start(*id, label.bookmark.clone());
    }

    docx = match block {
        Block::Heading { level, text } => {
            let lvl = (*level).clamp(1, 4);
            let native_num = ctx.native && num.is_some();
            let rendered = match (num, native_num) {
                (_, true) => text.clone(),
                (Some(p), false) => format!("{p}{text}"),
                (None, false) => text.clone(),
            };
            let mut p = Paragraph::new()
                .style(&format!("Heading{lvl}"))
                .page_break_before(brk);
            if native_num {
                p = numbering::apply_heading(p, &ctx.ids, lvl - 1);
            }
            docx.add_paragraph(add_text_runs(p, &rendered, style))
        }
        Block::Paragraph(text) => docx.add_paragraph(add_text_runs(
            Paragraph::new().style("BodyText").page_break_before(brk),
            text,
            style,
        )),
        Block::Quote(text) => docx.add_paragraph(add_text_runs(
            Paragraph::new().style("QuoteText").page_break_before(brk),
            text,
            style,
        )),
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
            docx.add_paragraph(add_text_runs(p, text, style))
        }
        Block::Reference(text) => docx.add_paragraph(add_text_runs(
            Paragraph::new()
                .style("ReferenceText")
                .page_break_before(brk),
            text,
            style,
        )),
        Block::Math(latex) => {
            let token = if style.formula_mode == super::super::FormulaMode::Svg {
                super::super::svg_docx::token(latex)
            } else {
                super::super::math::token(latex, true)
            };
            docx = docx.add_paragraph(
                Paragraph::new()
                    .style("MathBlock")
                    .page_break_before(brk)
                    .add_run(Run::new().add_text(token)),
            );
            // thesis/book:公式编号用 SEQ 域,便于 Word 内刷新
            if ctx.native
                && let Some(label) = labels.iter().find(|l| l.kind == LabelKind::Equation)
            {
                let p = Paragraph::new()
                    .style("Caption")
                    .align(AlignmentType::Right)
                    .add_run(Run::new().add_text("式 "));
                docx =
                    docx.add_paragraph(field::push_seq(p, "公式", &format!("({})", label.number)));
            }
            docx
        }
        Block::List { ordered, items } => add_list(docx, *ordered, items, ctx, brk),
        Block::Table {
            caption,
            header,
            rows,
            aligns,
        } => {
            if header.is_empty() && rows.is_empty() {
                return finish_bookmarks(docx, &bookmark_ids);
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
            docx.add_table(table_block(header, rows, aligns, style))
        }
        Block::Code { lines, .. } => {
            docx.add_paragraph(code_para(lines, style).page_break_before(brk))
        }
        Block::Caption(text) => docx.add_paragraph(add_text_runs(
            Paragraph::new().style("Caption").page_break_before(brk),
            text,
            style,
        )),
        Block::Image { alt, src } => match image_para(ctx.base, src, brk) {
            Some(p) => {
                docx = docx.add_paragraph(p);
                if !alt.trim().is_empty() {
                    docx = docx.add_paragraph(add_text_runs(
                        Paragraph::new().style("Caption"),
                        alt,
                        style,
                    ));
                }
                docx
            }
            None => docx.add_paragraph(add_text_runs(
                Paragraph::new().style("BodyText").page_break_before(brk),
                &format!("［图片未找到：{src}］"),
                style,
            )),
        },
        Block::Rule => {
            let border = ParagraphBorder::new(ParagraphBorderPosition::Bottom)
                .val(BorderType::Single)
                .size(6)
                .space(1)
                .color("BFBFBF");
            docx.add_paragraph(
                Paragraph::new()
                    .page_break_before(brk)
                    .line_spacing(LineSpacing::new().before(60).after(60))
                    .set_borders(ParagraphBorders::with_empty().set(border)),
            )
        }
        Block::PageBreak => {
            docx.add_paragraph(Paragraph::new().add_run(Run::new().add_break(BreakType::Page)))
        }
    };
    finish_bookmarks(docx, &bookmark_ids)
}

fn finish_bookmarks(mut docx: Docx, ids: &[usize]) -> Docx {
    for id in ids {
        docx = docx.add_bookmark_end(*id);
    }
    docx
}

fn add_list(
    mut docx: Docx,
    ordered: bool,
    items: &[super::super::markdown::ListItem],
    ctx: &Ctx<'_>,
    brk: bool,
) -> Docx {
    let style = ctx.style;
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
        if ctx.native {
            let p = numbering::apply_list(
                Paragraph::new()
                    .style("BodyText")
                    .align(AlignmentType::Left)
                    .page_break_before(brk && idx == 0),
                &ctx.ids,
                ordered,
                lvl,
            );
            docx = docx.add_paragraph(add_text_runs(p, &item.text, style));
            continue;
        }
        let marker = if ordered {
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
    docx
}

fn bookmark_id(label: &Label) -> usize {
    let mut h: usize = 7;
    for b in label.bookmark.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as usize);
    }
    h % 900_000 + 100
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
        // 公式:写入占位符,打包后由 math_docx / svg_docx 落地
        if seg.emphasis == Emphasis::Math {
            let token = if style.formula_mode == super::super::FormulaMode::Svg {
                super::super::svg_docx::token(&seg.text)
            } else {
                super::super::math::token(&seg.text, false)
            };
            p = p.add_run(Run::new().add_text(token));
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
