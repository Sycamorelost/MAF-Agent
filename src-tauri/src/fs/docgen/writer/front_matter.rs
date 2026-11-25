//! 封面、页眉、页脚:与正文块渲染无关的装配逻辑。

use docx_rs::*;

use super::super::DocRequest;
use super::super::style::DocStyle;
use super::body::add_text_runs;
use super::metrics;

pub(super) fn add_cover(mut docx: Docx, req: &DocRequest, style: &DocStyle) -> Docx {
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

pub(super) fn header_para(style: &DocStyle, title: &str) -> Paragraph {
    Paragraph::new()
        .style("Normal")
        .align(AlignmentType::Right)
        .indent(Some(0), Some(SpecialIndentType::FirstLine(0)), None, None)
        .add_run(
            Run::new()
                .add_text(title.trim())
                .size(18)
                .color("808080")
                .fonts(metrics::body_fonts(style)),
        )
}

/// 偶数页页眉:标题左对齐,与奇数页右对齐区分(配合 evenAndOddHeaders)。
pub(super) fn even_header_para(style: &DocStyle, title: &str) -> Paragraph {
    Paragraph::new()
        .style("Normal")
        .align(AlignmentType::Left)
        .indent(Some(0), Some(SpecialIndentType::FirstLine(0)), None, None)
        .add_run(
            Run::new()
                .add_text(title.trim())
                .size(18)
                .color("808080")
                .fonts(metrics::body_fonts(style)),
        )
}

pub(super) fn footer_para() -> Paragraph {
    Paragraph::new()
        .style("Normal")
        .align(AlignmentType::Center)
        .indent(Some(0), Some(SpecialIndentType::FirstLine(0)), None, None)
        .add_run(Run::new().add_text("第 ").size(18).color("808080"))
        .add_page_num(PageNum::new())
        .add_run(Run::new().add_text(" 页").size(18).color("808080"))
}
