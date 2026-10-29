//! 版式数据层:页面、字体、字号、行距、缩进、表格样式。
//!
//! 只描述「应该长什么样」,不依赖 docx-rs;由 `writer` 翻译成 Word 样式。
//! 数值单位:字号为半点(half-point,12pt=24),间距为 twip(1pt=20)。

use docx_rs::{LineSpacing, LineSpacingType, RunFonts};

use super::profile::Profile;

/// A4 纸张宽度(twip)。
pub const PAGE_W: u32 = 11_906;
/// A4 纸张高度(twip)。
pub const PAGE_H: u32 = 16_838;
/// 上下页边距 2.54cm。
pub const MARGIN_TB: i32 = 1_440;
/// 左右页边距 3cm。
pub const MARGIN_LR: i32 = 1_701;
/// 正文可用宽度(twip)= A4 − 左右边距。
pub const CONTENT_W: usize = (PAGE_W as i32 - 2 * MARGIN_LR) as usize;

/// 一套文档版式。
#[derive(Debug, Clone)]
pub struct DocStyle {
    /// 西文字体
    pub latin: &'static str,
    /// 正文中文字体
    pub east_asia: &'static str,
    /// 标题中文字体
    pub heading_east_asia: &'static str,
    /// 正文字号(半点)
    pub body_size: usize,
    /// 正文行距(240 = 单倍)
    pub body_line: i32,
    /// 正文首行缩进(twip,0 表示不缩进)
    pub body_indent: i32,
    /// 标题字号(4 级)
    pub heading_sizes: [usize; 4],
    /// 标题段前间距(twip)
    pub heading_before: [u32; 4],
    /// 标题段后间距(twip)
    pub heading_after: [u32; 4],
    /// 表格字号(半点)
    pub table_size: usize,
    /// 题注字号(半点)
    pub caption_size: usize,
    /// 封面/落款字号(半点)
    pub meta_size: usize,
    /// 文档大标题字号(半点)
    pub title_size: usize,
    /// 是否三线表
    pub three_line: bool,
    /// 表头是否加浅色底纹(通用文档用)
    pub header_fill: bool,
    /// 正文是否两端对齐(中文报告默认)
    pub justify: bool,
}

impl DocStyle {
    pub fn for_profile(profile: Profile) -> Self {
        match profile {
            // 中文报告/论文:宋体小四、1.5 倍行距、首行缩进 2 字符、黑体标题、三线表
            Profile::Experiment | Profile::Paper | Profile::Practice => DocStyle {
                latin: "Times New Roman",
                east_asia: "宋体",
                heading_east_asia: "黑体",
                body_size: 24,
                body_line: 360,
                body_indent: 480,
                heading_sizes: [32, 28, 24, 24],
                heading_before: [240, 180, 120, 120],
                heading_after: [120, 90, 60, 60],
                table_size: 21,
                caption_size: 21,
                meta_size: 24,
                title_size: 44,
                three_line: profile.three_line_table(),
                header_fill: !profile.three_line_table(),
                justify: true,
            },
            // 通用文档:微软雅黑 11pt、1.15 倍行距、无首行缩进、网格表
            Profile::General => DocStyle {
                latin: "Calibri",
                east_asia: "微软雅黑",
                heading_east_asia: "微软雅黑",
                body_size: 22,
                body_line: 276,
                body_indent: 0,
                heading_sizes: [32, 26, 24, 22],
                heading_before: [280, 200, 140, 120],
                heading_after: [140, 100, 80, 60],
                table_size: 21,
                caption_size: 20,
                meta_size: 22,
                title_size: 40,
                three_line: false,
                header_fill: true,
                justify: false,
            },
        }
    }

    pub fn body_fonts(&self) -> RunFonts {
        RunFonts::new()
            .ascii(self.latin)
            .hi_ansi(self.latin)
            .east_asia(self.east_asia)
            .hint("eastAsia")
    }

    pub fn heading_fonts(&self) -> RunFonts {
        RunFonts::new()
            .ascii(self.latin)
            .hi_ansi(self.latin)
            .east_asia(self.heading_east_asia)
            .hint("eastAsia")
    }

    pub fn mono_fonts(&self) -> RunFonts {
        RunFonts::new()
            .ascii("Consolas")
            .hi_ansi("Consolas")
            .east_asia(self.east_asia)
    }

    /// 正文行距(1.5 倍 / 1.15 倍)。
    pub fn body_line_spacing(&self) -> LineSpacing {
        LineSpacing::new()
            .line_rule(LineSpacingType::Auto)
            .line(self.body_line)
    }

    /// 单倍行距(表格、题注)。
    pub fn single_line_spacing(&self) -> LineSpacing {
        LineSpacing::new()
            .line_rule(LineSpacingType::Auto)
            .line(240)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn academic_profile_uses_chinese_report_typography() {
        let s = DocStyle::for_profile(Profile::Experiment);
        assert_eq!(s.body_size, 24); // 小四
        assert_eq!(s.body_line, 360); // 1.5 倍
        assert_eq!(s.body_indent, 480); // 2 字符
        assert!(s.three_line);
        assert_eq!(s.east_asia, "宋体");
    }

    #[test]
    fn general_profile_has_no_indent_and_grid_table() {
        let s = DocStyle::for_profile(Profile::General);
        assert_eq!(s.body_indent, 0);
        assert!(!s.three_line);
        assert!(s.header_fill);
    }

    #[test]
    fn content_width_matches_a4_minus_margins() {
        assert_eq!(CONTENT_W, 8504);
    }
}
