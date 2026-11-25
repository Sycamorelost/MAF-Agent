//! 白皮书 / 产品手册 / 新闻稿的通用版式覆盖。
//!
//! 复用商务排版主干,只调整字号层级、行距与表格风格;
//! 不内置任何单位专属规范,品牌色与字体由 `BrandTokens` 另行提供。

use super::super::style::DocStyle;

/// 白皮书:大标题、无首行缩进、宽松行距、网格表。
pub(super) fn apply_whitepaper(s: &mut DocStyle) {
    s.body_size = 22;
    s.body_line = 312;
    s.fixed_line = None;
    s.body_indent = 0;
    s.heading_sizes = [36, 28, 24, 22];
    s.heading_before = [360, 240, 160, 120];
    s.heading_after = [160, 120, 80, 60];
    s.title_size = 48;
    s.three_line = false;
    s.header_fill = true;
    s.justify = true;
}

/// 产品手册:紧凑标题、编号清晰、首行不缩进。
pub(super) fn apply_manual(s: &mut DocStyle) {
    s.body_size = 22;
    s.body_line = 300;
    s.fixed_line = None;
    s.body_indent = 0;
    s.heading_sizes = [32, 26, 24, 22];
    s.heading_before = [280, 200, 140, 100];
    s.heading_after = [120, 90, 70, 50];
    s.title_size = 40;
    s.three_line = false;
    s.header_fill = true;
    s.justify = false;
}

/// 新闻稿:左对齐、无虚饰间距,便于媒体二次编辑。
pub(super) fn apply_news(s: &mut DocStyle) {
    s.body_size = 22;
    s.body_line = 276;
    s.fixed_line = None;
    s.body_indent = 0;
    s.heading_sizes = [30, 26, 24, 22];
    s.heading_before = [240, 180, 120, 100];
    s.heading_after = [100, 80, 60, 50];
    s.title_size = 40;
    s.three_line = false;
    s.header_fill = false;
    s.justify = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::profile::Profile;

    fn base_style() -> DocStyle {
        DocStyle::for_profile(Profile::General)
    }

    #[test]
    fn styles_differ_from_generic_and_disable_indent() {
        let mut wp = base_style();
        apply_whitepaper(&mut wp);
        assert_eq!(wp.body_indent, 0);
        assert_eq!(wp.title_size, 48);

        let mut news = base_style();
        apply_news(&mut news);
        assert!(!news.justify);
        assert!(!news.header_fill);
        assert_ne!(news.title_size, wp.title_size);
    }
}
