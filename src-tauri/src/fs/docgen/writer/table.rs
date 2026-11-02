//! 表格:表头/单元格样式、边框与列宽分配。

use docx_rs::*;

use super::super::inline;
use super::super::markdown::Align;
use super::super::style::{CONTENT_W, DocStyle};
use super::document::add_text_runs_with;

pub(super) fn table_block(
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

pub(super) fn column_widths(header: &[String], rows: &[Vec<String>], cols: usize) -> Vec<usize> {
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
