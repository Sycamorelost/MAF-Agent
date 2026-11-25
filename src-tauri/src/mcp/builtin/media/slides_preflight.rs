//! 出稿预检:按字符类别估算文本宽度、超容量自动分页、缩字到下限。
//!
//! 估算规则:中文 1.0em、英文 0.55em、数字 0.5em;版心按 token 计算。
//! 溢出时先缩字(最多到 [`BODY_MIN_PT`]),仍溢出则分页并给诊断,不静默丢内容。

use super::slides::{Layout, Slide, SlideChart, SlideTable};
use super::slides_theme::{
    BODY_MIN_PT, BODY_PT, BODY_SIZE_STEPS, CONTENT_BOTTOM_IN, CONTENT_TOP_IN, CONTENT_W_IN,
    LINE_SPACING, MARGIN_X_IN, PT_PER_IN, SLIDE_H_IN, SLIDE_W_IN, TITLE_PT, aa_ok, check_theme,
    contrast_ratio,
};

/// 预检结果:非阻断告警 / 阻断诊断 / 分页记录。
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct PreflightReport {
    pub warnings: Vec<String>,
    pub diagnostics: Vec<String>,
    /// (原页序号 1-based, 续页数)
    pub page_split: Vec<(usize, usize)>,
}

/// 单字符宽度(em)。
pub(super) fn char_em(c: char) -> f64 {
    if c.is_ascii_digit() {
        0.5
    } else if c.is_ascii_alphabetic() {
        0.55
    } else if c.is_ascii() {
        0.35
    } else if c.is_whitespace() {
        0.35
    } else {
        // 中文/全角标点按 1em
        1.0
    }
}

/// 文本宽度(em):按字符类别累加。
pub(super) fn text_em(s: &str) -> f64 {
    s.chars().map(char_em).sum()
}

/// 可用正文宽度(inch)。
fn body_width_in() -> f64 {
    CONTENT_W_IN.min(SLIDE_W_IN - 2.0 * MARGIN_X_IN)
}

/// 版心高度(inch)。
fn body_height_in() -> f64 {
    (CONTENT_BOTTOM_IN.min(SLIDE_H_IN) - CONTENT_TOP_IN).max(0.0)
}

/// 单行可容纳的字符数(按 em 与字号折算)。
pub(super) fn chars_per_line(pt: u32) -> f64 {
    let line_w_pt = body_width_in() * PT_PER_IN;
    line_w_pt / pt as f64
}

/// 一条要点占用的行数。
pub(super) fn lines_of(text: &str, pt: u32) -> usize {
    let em = text_em(text);
    let per_line = chars_per_line(pt).max(1.0);
    (em / per_line).ceil().max(1.0) as usize
}

/// 正文区可容纳的行数。
fn capacity_lines(pt: u32) -> usize {
    let line_h_in = (pt as f64 / PT_PER_IN) * LINE_SPACING;
    (body_height_in() / line_h_in).floor().max(1.0) as usize
}

/// 检查标题是否超出单行;超长返回告警。
fn check_title(page: usize, title: &str, out: &mut PreflightReport) {
    let em = text_em(title);
    let limit = (body_width_in() * PT_PER_IN / TITLE_PT as f64).max(1.0);
    if em > limit {
        out.warnings.push(format!(
            "第 {page} 页标题约 {em:.0} 个字符宽,超过一行容量 {limit:.0}"
        ));
    }
}

/// 检查表格单元格宽度;超出列宽返回告警。
fn check_table(page: usize, t: &SlideTable, out: &mut PreflightReport) {
    let cols = t.header.len().max(1);
    let col_w_in = body_width_in() / cols as f64;
    let col_chars = col_w_in * PT_PER_IN / BODY_MIN_PT as f64;
    for (r, row) in t.rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            if text_em(cell) > col_chars * 3.0 {
                out.warnings.push(format!(
                    "第 {page} 页表格第 {} 行第 {} 列单元格过长,建议拆分成多条",
                    r + 1,
                    c + 1
                ));
            }
        }
    }
}

/// 图表页检查:系列过长或对比度不足。
fn check_chart(page: usize, c: &SlideChart, theme: &str, out: &mut PreflightReport) {
    if c.categories.len() > 8 {
        out.warnings.push(format!(
            "第 {page} 页图表分类 {} 个,建议合并为不超过 8 个",
            c.categories.len()
        ));
    }
    let p = super::slides_theme::colors(theme);
    if !aa_ok(&p.text, &p.background) {
        out.diagnostics.push(format!(
            "第 {page} 页主题对比度不足 {:.2}:1",
            contrast_ratio(&p.text, &p.background)
        ));
    }
}

/// 尝试把要点放进一页:先原字号,再逐档缩小到下限。
/// 返回 (字号, 每页行数预算)；无法放下则返回 None。
fn fit_pt(bullets: &[String]) -> Option<u32> {
    for pt in BODY_SIZE_STEPS {
        let capacity = capacity_lines(pt);
        let used: usize = bullets.iter().map(|b| lines_of(b, pt)).sum();
        if used <= capacity {
            return Some(pt);
        }
    }
    None
}

/// 把要点按容量切分为多页(续页)。
fn split_bullets(bullets: &[String], pt: u32) -> Vec<Vec<String>> {
    let capacity = capacity_lines(pt);
    let mut pages = Vec::new();
    let mut cur = Vec::new();
    let mut used = 0usize;
    for b in bullets {
        let n = lines_of(b, pt);
        if used + n > capacity && !cur.is_empty() {
            pages.push(std::mem::take(&mut cur));
            used = 0;
        }
        cur.push(b.clone());
        used += n;
    }
    if !cur.is_empty() {
        pages.push(cur);
    }
    if pages.is_empty() {
        pages.push(Vec::new());
    }
    pages
}

/// 对整份稿件做预检,并把超容量的要点页替换为分页后的多页。
///
/// 返回替换后的幻灯片列表(与报告)。
pub(super) fn run(slides: &[Slide], theme: &str) -> (Vec<Slide>, PreflightReport) {
    let mut report = PreflightReport::default();
    // 正文、标题与脚注对比度只报一次,避免每页重复刷屏
    report.diagnostics.extend(check_theme(theme));

    let mut out = Vec::with_capacity(slides.len());
    for (i, s) in slides.iter().enumerate() {
        let page = i + 1;
        check_title(page, &s.title, &mut report);
        if let Some(t) = &s.table {
            check_table(page, t, &mut report);
        }
        if let Some(c) = &s.chart {
            check_chart(page, c, theme, &mut report);
        }

        // 只有纯要点页参与分页;图表/表格/指标卡页保留原有排版
        let splittable = s.layout == Layout::Bullets
            && s.table.is_none()
            && s.chart.is_none()
            && s.image.is_none();
        if !splittable || s.bullets.is_empty() {
            out.push(s.clone());
            continue;
        }
        match fit_pt(&s.bullets) {
            Some(pt) => {
                if pt < BODY_PT {
                    report
                        .warnings
                        .push(format!("第 {page} 页要点偏多,正文字号自适应为 {pt}pt"));
                }
                let mut copy = s.clone();
                copy.body_pt = pt;
                out.push(copy);
            }
            None => {
                // 到下限仍溢出:按最小字号分页
                let chunks = split_bullets(&s.bullets, BODY_MIN_PT);
                let extra = chunks.len().saturating_sub(1);
                if extra > 0 {
                    report.page_split.push((page, extra));
                    report.warnings.push(format!(
                        "第 {page} 页要点超出容量,已自动拆为 {} 页(正文字号降至 {BODY_MIN_PT}pt 下限)",
                        chunks.len()
                    ));
                    if chunks.iter().any(|c| {
                        c.len() == 1 && lines_of(&c[0], BODY_MIN_PT) > capacity_lines(BODY_MIN_PT)
                    }) {
                        report.diagnostics.push(format!(
                            "第 {page} 页存在单条要点在 {BODY_MIN_PT}pt 下仍超出单页,请人工拆分"
                        ));
                    }
                }
                for (k, chunk) in chunks.into_iter().enumerate() {
                    let mut copy = s.clone();
                    if k > 0 {
                        copy.title = format!("{}（续）", s.title);
                    }
                    copy.bullets = chunk;
                    copy.body_pt = BODY_MIN_PT;
                    out.push(copy);
                }
            }
        }
    }
    (out, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::builtin::media::chart::ChartStyle;
    use crate::mcp::builtin::media::slides::Layout;

    fn slide(title: &str, bullets: Vec<String>) -> Slide {
        Slide {
            title: title.into(),
            bullets,
            image: None,
            notes: None,
            layout: Layout::Bullets,
            table: None,
            kpis: Vec::new(),
            chart: None,
            chart_style: ChartStyle::Native,
            raster_chart: None,
            diagram: None,
            cards: Vec::new(),
            quote: None,
            formula_note: None,
            body_pt: BODY_PT,
        }
    }

    #[test]
    fn estimates_width_by_char_class() {
        assert_eq!(text_em("中文"), 2.0);
        assert!((text_em("abc") - 1.65).abs() < 1e-9);
        assert!((text_em("123") - 1.5).abs() < 1e-9);
        assert!(lines_of("短句", BODY_PT) >= 1);
    }

    #[test]
    fn splits_overflowing_bullets_and_warns_at_floor() {
        // 每条约 100 字:18pt 占 3 行、14pt 占 2 行,10 条合计超出最小字号单页容量,触发分页
        let many: Vec<String> = (0..10)
            .map(|i| format!("要点{i}{}", "很长的说明文字".repeat(14)))
            .collect();
        let (out, report) = run(&[slide("超长页", many)], "corporate");
        assert!(out.len() > 1, "应自动分页");
        assert!(out[1].title.ends_with("（续）"));
        assert!(!report.page_split.is_empty());
        assert!(report.warnings.iter().any(|w| w.contains("下限")));
        assert_eq!(report.page_split[0].0, 1);
    }

    #[test]
    fn keeps_short_page_unchanged() {
        let (out, report) = run(&[slide("简短", vec!["一条要点".into()])], "corporate");
        assert_eq!(out.len(), 1);
        assert!(report.diagnostics.is_empty());
        assert!(report.warnings.is_empty());
    }

}
