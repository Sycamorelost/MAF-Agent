//! `slides_export`:把结构化大纲导出为 16:9 的 .pptx;失败时回退 Markdown 大纲。
//!
//! 参数解析与校验在 `slides_parse` 子模块;渲染在 `slides_render`。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::slides_parse::parse_args;
use super::slides_render::build_and_write;
use super::slides_templates::SlideTemplate;

/// 指标卡上限(解析与渲染共用)。
pub(super) const MAX_KPIS: usize = 4;

/// 单页版式:段落型 / 两栏 / 右图 / 表格 / 图表 / 指标卡 / 章节过渡 / 结束页。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Layout {
    Bullets,
    TwoCol,
    ImageRight,
    Table,
    Chart,
    Kpi,
    Section,
    Closing,
}

impl Layout {
    pub(super) fn parse(raw: &str) -> Option<Layout> {
        let p = match raw.trim().to_ascii_lowercase().as_str() {
            "bullets" | "content" | "default" => Layout::Bullets,
            "two-col" | "two_col" | "twocol" => Layout::TwoCol,
            "image-right" | "image_right" | "image" => Layout::ImageRight,
            "table" | "data" => Layout::Table,
            "chart" | "trend" | "data-chart" => Layout::Chart,
            "kpi" | "metrics" | "numbers" => Layout::Kpi,
            "section" | "divider" => Layout::Section,
            "closing" | "end" | "thanks" => Layout::Closing,
            _ => return None,
        };
        Some(p)
    }
}

pub(super) struct SlideTable {
    pub(super) header: Vec<String>,
    pub(super) rows: Vec<Vec<String>>,
}

/// 指标卡:一个大数字 + 一行解释,最多 4 张并排。
pub(super) struct Kpi {
    pub(super) value: String,
    pub(super) label: String,
}

/// 原生图表:柱/折线/饼/环形/面积。
pub(super) struct SlideChart {
    pub(super) kind: String,
    pub(super) title: String,
    pub(super) categories: Vec<String>,
    pub(super) series: Vec<(String, Vec<f64>)>,
}

pub(super) struct Slide {
    pub(super) title: String,
    pub(super) bullets: Vec<String>,
    pub(super) image: Option<String>,
    pub(super) notes: Option<String>,
    pub(super) layout: Layout,
    pub(super) table: Option<SlideTable>,
    pub(super) kpis: Vec<Kpi>,
    pub(super) chart: Option<SlideChart>,
}

pub(super) struct Deck {
    pub(super) title: String,
    pub(super) subtitle: Option<String>,
    pub(super) theme: String,
    pub(super) root: PathBuf,
    pub(super) target: PathBuf,
    pub(super) slides: Vec<Slide>,
    /// 场景模板(可选),只影响主题默认值与回执建议
    pub(super) template: Option<&'static SlideTemplate>,
    /// 结构/数据缺口提示(非阻断)
    pub(super) warnings: Vec<String>,
}

pub(crate) async fn exec(args: &Value, session_id: Option<&str>) -> (String, bool) {
    let deck = match parse_args(args, session_id) {
        Ok(d) => d,
        Err(e) => return (format!("演示文稿生成失败:{e}"), false),
    };
    match build_and_write(&deck, session_id) {
        Ok(msg) => {
            let mut msg = msg;
            if let Some(t) = deck.template
                && !deck.warnings.is_empty()
            {
                msg.push('\n');
                msg.push_str(&format!("模板检查:{}", deck.warnings.join(";")));
                msg.push('\n');
                msg.push_str(&t.plan_hint());
            }
            (msg, true)
        }
        Err(e) => match write_outline(&deck) {
            Ok(path) => {
                let (_, registered) = crate::deliverables::present(
                    session_id,
                    &json!({ "path": path, "kind": "text", "name": file_name(&deck.target, "md") }),
                );
                (
                    format!(
                        "演示文稿生成失败:{e}。已回退导出 Markdown 大纲:{path}{}",
                        if registered { "(已登记产物)" } else { "" }
                    ),
                    false,
                )
            }
            Err(e2) => (format!("演示文稿生成失败:{e};大纲回退也失败:{e2}"), false),
        },
    }
}

pub(super) fn file_name(target: &Path, ext: &str) -> String {
    target
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| format!("{s}.{ext}"))
        .unwrap_or_else(|| format!("slides.{ext}"))
}

/// 按字符截断,超长补省略号(避免一页文字溢出)。
pub(super) fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// 回退大纲:严格按页输出标题与要点,不依赖第三方解析。
fn outline_markdown(deck: &Deck) -> String {
    let mut md = format!("# {}\n\n", deck.title);
    if let Some(sub) = &deck.subtitle {
        md.push_str(&format!("{sub}\n\n"));
    }
    for s in &deck.slides {
        md.push_str(&format!("## {}\n", s.title));
        for b in &s.bullets {
            md.push_str(&format!("- {b}\n"));
        }
        if let Some(n) = &s.notes {
            md.push_str(&format!("\n> {n}\n"));
        }
        md.push('\n');
    }
    md
}

fn write_outline(deck: &Deck) -> Result<String, String> {
    let mut p = deck.target.clone();
    p.set_extension("md");
    std::fs::write(&p, outline_markdown(deck)).map_err(|e| format!("写入大纲失败: {e}"))?;
    Ok(crate::fs::paths::rel_of(&deck.root, &p)
        .unwrap_or_else(|_| p.display().to_string().replace('\\', "/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> PathBuf {
        let p = std::env::temp_dir().join(format!("maf-slides-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn deck_in(root: &Path) -> Deck {
        Deck {
            title: "季度复盘".into(),
            subtitle: Some("2026 Q3".into()),
            theme: "corporate".into(),
            root: root.to_path_buf(),
            target: root.join("deck.pptx"),
            slides: vec![
                Slide {
                    title: "营收概览".into(),
                    bullets: vec!["营收同比 +12%".into(), "成本下降 3%".into()],
                    image: None,
                    notes: Some("先讲大盘".into()),
                    layout: Layout::Bullets,
                    table: None,
                    kpis: Vec::new(),
                    chart: None,
                },
                Slide {
                    title: "下一步".into(),
                    bullets: vec!["推进交付".into()],
                    image: None,
                    notes: None,
                    layout: Layout::Bullets,
                    table: None,
                    kpis: Vec::new(),
                    chart: None,
                },
            ],
            template: None,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn parses_kpi_and_chart() {
        let root = ws();
        let args = json!({
            "title": "经营看板",
            "path": root.join("k.pptx").to_string_lossy(),
            "slides": [
                {
                    "title": "核心指标",
                    "layout": "kpi",
                    "kpis": [
                        { "value": "1.2 亿", "label": "营业收入" },
                        { "value": "+18%", "label": "同比增长" }
                    ]
                },
                {
                    "title": "月度趋势",
                    "layout": "chart",
                    "chart": {
                        "type": "line",
                        "categories": ["1月", "2月", "3月"],
                        "series": [{ "name": "营收", "values": [10, 12, 15] }]
                    }
                }
            ]
        });
        let deck = parse_args(&args, None).unwrap();
        assert_eq!(deck.slides[0].layout, Layout::Kpi);
        assert_eq!(deck.slides[0].kpis.len(), 2);
        assert_eq!(deck.slides[1].layout, Layout::Chart);
        let c = deck.slides[1].chart.as_ref().unwrap();
        assert_eq!(c.categories.len(), 3);
        assert_eq!(c.series[0].1, vec![10.0, 12.0, 15.0]);
        build_and_write(&deck, None).unwrap();
        // 图表页与指标卡页都能正常出包
        let text = crate::fs::formats::pptx_text(&deck.target).unwrap();
        assert!(text.contains("核心指标"), "读回内容: {text}");
    }

    #[test]
    fn applies_template_theme_and_returns_warnings() {
        let root = ws();
        let args = json!({
            "title": "论文答辩",
            "path": root.join("d.pptx").to_string_lossy(),
            "template": "defense",
            "slides": [
                { "title": "研究背景", "bullets": ["问题一"] },
                { "title": "方法", "bullets": ["方案"] }
            ]
        });
        let deck = parse_args(&args, None).unwrap();
        assert_eq!(deck.theme, "academic");
        assert_eq!(deck.template.unwrap().id, "defense");
        assert!(deck.warnings.iter().any(|w| w.contains("页数")));
        assert!(deck.warnings.iter().any(|w| w.contains("图表")));
    }

    #[test]
    fn explicit_theme_overrides_template() {
        let root = ws();
        let args = json!({
            "title": "组会",
            "path": root.join("g.pptx").to_string_lossy(),
            "template": "group-meeting",
            "theme": "dark",
            "slides": [{ "title": "进展", "bullets": ["完成 A"] }]
        });
        assert_eq!(parse_args(&args, None).unwrap().theme, "dark");
    }

    #[test]
    fn parses_layout_and_table() {
        let root = ws();
        let args = json!({
            "title": "数据看板",
            "path": root.join("t.pptx").to_string_lossy(),
            "slides": [{
                "title": "指标",
                "layout": "table",
                "table": { "headers": ["指标", "值"], "rows": [["营收", "120"]] }
            }]
        });
        let deck = parse_args(&args, None).unwrap();
        assert_eq!(deck.slides[0].layout, Layout::Table);
        let t = deck.slides[0].table.as_ref().unwrap();
        assert_eq!(t.header, vec!["指标".to_string(), "值".to_string()]);
        assert_eq!(t.rows[0], vec!["营收".to_string(), "120".to_string()]);
        // 版式与表格都能正常出包
        build_and_write(&deck, None).unwrap();
        let text = crate::fs::formats::pptx_text(&deck.target).unwrap();
        assert!(text.contains("营收"), "读回内容: {text}");
    }

    #[test]
    fn build_deck_roundtrips_text() {
        let root = ws();
        let deck = deck_in(&root);
        build_and_write(&deck, None).unwrap();
        let text = crate::fs::formats::pptx_text(&deck.target).unwrap();
        assert!(text.contains("营收概览"), "读回内容: {text}");
        assert!(text.contains("营收同比 +12%"));
        assert!(text.contains("下一步"));
    }

    #[test]
    fn outline_contains_titles_and_bullets() {
        let root = ws();
        let md = outline_markdown(&deck_in(&root));
        assert!(md.contains("# 季度复盘"));
        assert!(md.contains("## 营收概览"));
        assert!(md.contains("- 成本下降 3%"));
    }

    #[test]
    fn rejects_too_many_slides_and_bad_title() {
        let root = ws();
        let too_many = json!({
            "title": "x",
            "path": root.join("x.pptx").to_string_lossy(),
            "slides": (0..31).map(|i| json!({"title": format!("p{i}")})).collect::<Vec<_>>()
        });
        assert!(parse_args(&too_many, None).is_err());
        let empty_title = json!({"title": "  ", "slides": [{"title": "a"}]});
        assert!(parse_args(&empty_title, None).is_err());
    }
}
