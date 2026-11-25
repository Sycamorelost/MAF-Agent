//! 版式 → docx 度量转换。
//!
//! `style` 层只保留纯数值(twip / half-point),不依赖 docx-rs;
//! 这里把数值翻译成渲染库的字体与行距类型,保持依赖单向。

use docx_rs::{LineSpacing, LineSpacingType, RunFonts};

use super::super::style::DocStyle;

pub(super) fn body_fonts(s: &DocStyle) -> RunFonts {
    RunFonts::new()
        .ascii(s.latin.as_str())
        .hi_ansi(s.latin.as_str())
        .east_asia(s.east_asia.as_str())
        .hint("eastAsia")
}

pub(super) fn heading_fonts(s: &DocStyle) -> RunFonts {
    RunFonts::new()
        .ascii(s.latin.as_str())
        .hi_ansi(s.latin.as_str())
        .east_asia(s.heading_east_asia.as_str())
        .hint("eastAsia")
}

pub(super) fn mono_fonts(s: &DocStyle) -> RunFonts {
    RunFonts::new()
        .ascii("Consolas")
        .hi_ansi("Consolas")
        .east_asia(s.east_asia.as_str())
}

/// 正文行距:固定行距优先(公文),否则按倍数。
pub(super) fn body_line_spacing(s: &DocStyle) -> LineSpacing {
    match s.fixed_line {
        Some(line) => LineSpacing::new()
            .line_rule(LineSpacingType::Exact)
            .line(line),
        None => LineSpacing::new()
            .line_rule(LineSpacingType::Auto)
            .line(s.body_line),
    }
}

/// 单倍行距(表格、题注)。
pub(super) fn single_line_spacing() -> LineSpacing {
    LineSpacing::new()
        .line_rule(LineSpacingType::Auto)
        .line(240)
}
