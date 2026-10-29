//! docx 渲染层:把块结构 + 版式翻译成 docx-rs 文档。
//!
//! 这是唯一依赖 docx-rs 的模块;解析、版式数据都保持纯函数,便于替换渲染后端
//! (例如后续换 OOXML 模板或 PDF)。
//!
//! 版式落地为「真实 Word 样式」而非逐段直接格式化:
//! - Heading1~4 带 outlineLvl → 导航窗格、自动目录、样式替换可用;
//! - BodyText 首行缩进 2 字符、1.5 倍行距、两端对齐;
//! - Caption / TableText / SourceCode / QuoteText 各自独立;
//! - 表格用固定列宽 + 三线表(学术模板)或浅灰网格表(通用模板)。

use std::path::{Path, PathBuf};

use docx_rs::*;
use image::GenericImageView;

use super::DocRequest;
use super::inline::{self, Emphasis};
use super::markdown::{Align, Block};
use super::style::{CONTENT_W, DocStyle, MARGIN_LR, MARGIN_TB, PAGE_H, PAGE_W};

/// 图片最大宽度(EMU,约 14.7cm),超出等比缩放。
const IMAGE_MAX_W: u64 = 5_300_000;
/// 图片最大高度(EMU,约 18cm)。
const IMAGE_MAX_H: u64 = 6_500_000;
/// 图片体积上限,防止把超大图塞进文档。
const IMAGE_MAX_BYTES: u64 = 20 * 1024 * 1024;

pub fn render(req: &DocRequest, style: &DocStyle, base: &Path) -> Result<Vec<u8>, String> {
    let blocks = super::markdown::parse(&req.body);
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

fn add_text_runs_with(
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

// ---------------- 表格 ----------------

fn table_block(
    header: &[String],
    rows: &[Vec<String>],
    aligns: &[Align],
    style: &DocStyle,
) -> Table {
    let cols = header
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if cols == 0 {
        return Table::new(vec![]);
    }
    let widths = column_widths(header, rows, cols);

    let mut trs: Vec<TableRow> = Vec::new();
    if !header.is_empty() {
        trs.push(table_row(header, &widths, aligns, style, true));
    }
    for row in rows {
        trs.push(table_row(row, &widths, aligns, style, false));
    }

    let borders = if style.three_line {
        TableBorders::with_empty()
            .set(TableBorder::new(TableBorderPosition::Top).size(12))
            .set(TableBorder::new(TableBorderPosition::Bottom).size(12))
    } else {
        let line = |pos| TableBorder::new(pos).size(4).color("BFBFBF");
        TableBorders::with_empty()
            .set(line(TableBorderPosition::Top))
            .set(line(TableBorderPosition::Left))
            .set(line(TableBorderPosition::Bottom))
            .set(line(TableBorderPosition::Right))
            .set(line(TableBorderPosition::InsideH))
            .set(line(TableBorderPosition::InsideV))
    };

    Table::new(trs)
        .set_grid(widths.clone())
        .width(CONTENT_W, WidthType::Dxa)
        .layout(TableLayoutType::Fixed)
        .margins(TableCellMargins::new().margin(60, 108, 60, 108))
        .set_borders(borders)
}

fn table_row(
    cells: &[String],
    widths: &[usize],
    aligns: &[Align],
    style: &DocStyle,
    is_header: bool,
) -> TableRow {
    let mut tcs: Vec<TableCell> = Vec::new();
    for (i, w) in widths.iter().enumerate() {
        let text = cells.get(i).map(String::as_str).unwrap_or("");
        let align = if is_header {
            AlignmentType::Center
        } else {
            match aligns.get(i).copied().unwrap_or(Align::Left) {
                Align::Left => AlignmentType::Left,
                Align::Center => AlignmentType::Center,
                Align::Right => AlignmentType::Right,
            }
        };
        let p = add_text_runs_with(
            Paragraph::new().style("TableText").align(align),
            text,
            style,
            is_header,
        );
        let mut cell = TableCell::new()
            .width(*w, WidthType::Dxa)
            .vertical_align(VAlignType::Center)
            .add_paragraph(p);
        if is_header {
            if style.three_line {
                // 三线表:表头下方一条细线,其余无竖线
                cell = cell.set_borders(
                    TableCellBorders::with_empty().set(
                        TableCellBorder::new(TableCellBorderPosition::Bottom)
                            .size(6)
                            .border_type(BorderType::Single)
                            .color("000000"),
                    ),
                );
            } else if style.header_fill {
                cell = cell.shading(Shading::new().fill("F2F2F2"));
            }
        }
        tcs.push(cell);
    }
    TableRow::new(tcs)
}

fn column_widths(header: &[String], rows: &[Vec<String>], cols: usize) -> Vec<usize> {
    let mut weights = vec![2usize; cols];
    for row in std::iter::once(header).chain(rows.iter().map(Vec::as_slice)) {
        for (i, cell) in row.iter().enumerate() {
            if i < cols {
                weights[i] = weights[i].max(inline::display_width(cell).min(48));
            }
        }
    }
    let total: usize = weights.iter().sum::<usize>().max(1);
    // 列数很多时按平均宽度压缩下限,避免总宽超出正文区
    let min_w = (CONTENT_W / cols.max(1)).clamp(240, 720);
    let mut widths: Vec<usize> = weights
        .iter()
        .map(|w| (CONTENT_W * w / total).max(min_w))
        .collect();
    for _ in 0..4 {
        let sum: usize = widths.iter().sum();
        if sum == 0 {
            break;
        }
        let diff = CONTENT_W as i64 - sum as i64;
        if diff == 0 {
            break;
        }
        let flex: Vec<usize> = (0..cols)
            .filter(|&i| diff > 0 || widths[i] > min_w)
            .collect();
        if flex.is_empty() {
            break;
        }
        let flex_total: usize = flex.iter().map(|&i| widths[i]).sum::<usize>().max(1);
        for &i in &flex {
            let delta = (diff as f64 * widths[i] as f64 / flex_total as f64).round() as i64;
            widths[i] = (widths[i] as i64 + delta).max(1) as usize;
        }
    }
    // 舍入误差由首列吸收,保证网格总宽与正文区一致
    let sum: usize = widths.iter().sum();
    widths[0] = (widths[0] as i64 + CONTENT_W as i64 - sum as i64).max(1) as usize;
    widths
}

// ---------------- 图片 ----------------

fn image_para(base: &Path, src: &str, brk: bool) -> Option<Paragraph> {
    let path = resolve_image(base, src)?;
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() > IMAGE_MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).ok()?;

    let mut ew = w as u64 * 9525;
    let mut eh = h as u64 * 9525;
    if ew > IMAGE_MAX_W {
        eh = eh * IMAGE_MAX_W / ew;
        ew = IMAGE_MAX_W;
    }
    if eh > IMAGE_MAX_H {
        ew = ew * IMAGE_MAX_H / eh;
        eh = IMAGE_MAX_H;
    }
    let pic = Pic::new_with_dimensions(png.into_inner(), w, h).size(ew as u32, eh as u32);
    Some(
        Paragraph::new()
            .align(AlignmentType::Center)
            .page_break_before(brk)
            .add_run(Run::new().add_image(pic)),
    )
}

fn resolve_image(base: &Path, src: &str) -> Option<PathBuf> {
    let raw = src.trim().trim_matches(|c| c == '"' || c == '\'');
    if raw.is_empty() || raw.starts_with("http://") || raw.starts_with("https://") {
        return None;
    }
    let ext = Path::new(raw)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())?;
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg") {
        return None;
    }
    let path = Path::new(raw);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    path.is_file().then_some(path)
}

// ---------------- 样式表 ----------------

fn styles_for(s: &DocStyle) -> Styles {
    let mut styles = Styles::new()
        .default_fonts(s.body_fonts())
        .default_size(s.body_size)
        .default_line_spacing(s.body_line_spacing());

    // 正文:首行缩进 2 字符、行距、两端对齐
    styles = styles.add_style(
        Style::new("BodyText", StyleType::Paragraph)
            .name("Body Text")
            .based_on("Normal")
            .q_format(true)
            .align(if s.justify {
                AlignmentType::Both
            } else {
                AlignmentType::Left
            })
            .line_spacing(s.body_line_spacing())
            .indent(
                Some(0),
                Some(SpecialIndentType::FirstLine(s.body_indent)),
                None,
                None,
            ),
    );

    // 标题:真实 Heading 样式 + 大纲级别,供导航窗格与自动目录使用
    for (idx, size) in s.heading_sizes.iter().enumerate() {
        let lvl = idx + 1;
        styles = styles.add_style(
            Style::new(format!("Heading{lvl}"), StyleType::Paragraph)
                .name(format!("heading {lvl}"))
                .based_on("Normal")
                .next("BodyText")
                .q_format(true)
                .ui_priority(9)
                .bold()
                .fonts(s.heading_fonts())
                .size(*size)
                .color("000000")
                .align(AlignmentType::Left)
                .outline_lvl(idx)
                .line_spacing(
                    LineSpacing::new()
                        .before(s.heading_before[idx])
                        .after(s.heading_after[idx]),
                )
                .indent(Some(0), Some(SpecialIndentType::FirstLine(0)), None, None),
        );
    }

    styles = styles
        .add_style(
            Style::new("TableText", StyleType::Paragraph)
                .name("Table Text")
                .based_on("Normal")
                .q_format(true)
                .size(s.table_size)
                .line_spacing(s.single_line_spacing())
                .align(AlignmentType::Left)
                .indent(Some(0), Some(SpecialIndentType::FirstLine(0)), None, None),
        )
        .add_style(
            Style::new("Caption", StyleType::Paragraph)
                .name("caption")
                .based_on("Normal")
                .q_format(true)
                .size(s.caption_size)
                .line_spacing(
                    LineSpacing::new()
                        .before(60)
                        .after(60)
                        .line_rule(LineSpacingType::Auto)
                        .line(240),
                )
                .align(AlignmentType::Center)
                .indent(Some(0), Some(SpecialIndentType::FirstLine(0)), None, None),
        )
        .add_style(
            Style::new("SourceCode", StyleType::Paragraph)
                .name("Source Code")
                .based_on("Normal")
                .q_format(true)
                .fonts(s.mono_fonts())
                .size(18)
                .line_spacing(
                    LineSpacing::new()
                        .line_rule(LineSpacingType::Exact)
                        .line(220),
                )
                .indent(Some(240), Some(SpecialIndentType::FirstLine(0)), None, None),
        )
        .add_style(
            Style::new("QuoteText", StyleType::Paragraph)
                .name("Quote Text")
                .based_on("Normal")
                .q_format(true)
                .size(s.body_size)
                .color("595959")
                .line_spacing(
                    LineSpacing::new()
                        .line_rule(LineSpacingType::Auto)
                        .line(300),
                )
                .indent(Some(480), Some(SpecialIndentType::FirstLine(0)), None, None),
        )
        .add_style(
            Style::new("DocTitle", StyleType::Paragraph)
                .name("Document Title")
                .based_on("Normal")
                .q_format(true)
                .bold()
                .fonts(s.heading_fonts())
                .size(s.title_size)
                .color("000000")
                .align(AlignmentType::Center)
                .line_spacing(LineSpacing::new().after(180))
                .indent(Some(0), Some(SpecialIndentType::FirstLine(0)), None, None),
        )
        .add_style(
            Style::new("DocSubtitle", StyleType::Paragraph)
                .name("Document Subtitle")
                .based_on("Normal")
                .q_format(true)
                .size(s.meta_size)
                .color("595959")
                .align(AlignmentType::Center)
                .line_spacing(LineSpacing::new().after(120))
                .indent(Some(0), Some(SpecialIndentType::FirstLine(0)), None, None),
        )
        .add_style(
            Style::new("MetaText", StyleType::Paragraph)
                .name("Meta Text")
                .based_on("Normal")
                .q_format(true)
                .size(s.meta_size)
                .align(AlignmentType::Center)
                .line_spacing(
                    LineSpacing::new()
                        .line_rule(LineSpacingType::Auto)
                        .line(360),
                )
                .indent(Some(0), Some(SpecialIndentType::FirstLine(0)), None, None),
        )
        .add_style(
            Style::new("TocHeading", StyleType::Paragraph)
                .name("TOC Heading")
                .based_on("Normal")
                .q_format(true)
                .bold()
                .fonts(s.heading_fonts())
                .size(32)
                .align(AlignmentType::Center)
                .line_spacing(LineSpacing::new().after(240))
                .indent(Some(0), Some(SpecialIndentType::FirstLine(0)), None, None),
        );

    styles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widths_of(cells: &[&str]) -> Vec<usize> {
        let header: Vec<String> = cells.iter().map(|s| s.to_string()).collect();
        column_widths(&header, &[], cells.len())
    }

    #[test]
    fn column_widths_fill_content_width() {
        let w = widths_of(&["模型", "推理时间", "得分"]);
        assert_eq!(w.iter().sum::<usize>(), CONTENT_W);
        assert!(w[1] > w[0], "长列应获得更大宽度: {w:?}");
        assert!(w.iter().all(|v| *v >= 720));
    }

    #[test]
    fn column_widths_handle_single_column() {
        assert_eq!(widths_of(&["说明"]), vec![CONTENT_W]);
    }
}
