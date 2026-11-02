//! Word 样式表:正文/标题/表格/题注/代码/引用/封面等真实样式定义。

use docx_rs::*;

use super::super::style::DocStyle;

pub(super) fn styles_for(s: &DocStyle) -> Styles {
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
