//! `slides_export`:把结构化大纲导出为 16:9 的 .pptx;失败时回退 Markdown 大纲。
//!
//! 参数解析与校验在 `slides_parse` 子模块;渲染在 `slides_render`。

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::chart::ChartStyle;
use super::diagram::DiagramSpec;
use super::slides_parse::parse_args;
use super::slides_render::build_and_write;
use super::slides_templates::SlideTemplate;

/// 指标卡上限(解析与渲染共用)。
pub(super) const MAX_KPIS: usize = 4;

/// 单页版式:段落型 / 两栏 / 右图 / 表格 / 图表 / 指标卡 / 结构图 /
/// 卡片 / 引用 / 章节过渡 / 结束页。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Layout {
    Bullets,
    TwoCol,
    ImageRight,
    Table,
    Chart,
    Kpi,
    Diagram,
    Cards,
    Quote,
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
            "diagram" | "structure" | "flow" => Layout::Diagram,
            "cards" | "card" | "grid" | "reading-notes" => Layout::Cards,
            "quote" | "citation" | "pull-quote" => Layout::Quote,
            "section" | "divider" => Layout::Section,
            "closing" | "end" | "thanks" => Layout::Closing,
            _ => return None,
        };
        Some(p)
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct SlideTable {
    pub(super) header: Vec<String>,
    pub(super) rows: Vec<Vec<String>>,
}

/// 指标卡:一个大数字 + 一行解释,最多 4 张并排。
#[derive(Debug, Clone)]
pub(super) struct Kpi {
    pub(super) value: String,
    pub(super) label: String,
}

/// 原生图表:柱/折线/饼/环形/面积;unit/source 用于图表页脚。
#[derive(Debug, Clone)]
pub(super) struct SlideChart {
    pub(super) kind: String,
    pub(super) title: String,
    pub(super) categories: Vec<String>,
    pub(super) series: Vec<(String, Vec<f64>)>,
    pub(super) unit: String,
    pub(super) source: String,
}

/// 卡片:读书卡片 / 要点卡,2×N 网格排布。
#[derive(Debug, Clone, Default)]
pub(super) struct Card {
    pub(super) title: String,
    pub(super) text: String,
}

/// 引用页:引文 + 出处。
#[derive(Debug, Clone, Default)]
pub(super) struct Quote {
    pub(super) text: String,
    pub(super) source: String,
}

#[derive(Debug, Clone)]
pub(super) struct Slide {
    pub(super) title: String,
    pub(super) bullets: Vec<String>,
    pub(super) image: Option<String>,
    pub(super) notes: Option<String>,
    pub(super) layout: Layout,
    pub(super) table: Option<SlideTable>,
    pub(super) kpis: Vec<Kpi>,
    pub(super) chart: Option<SlideChart>,
    /// 图表渲染方式:默认 native(兼容旧输出)
    pub(super) chart_style: ChartStyle,
    /// raster 模式的严格校验结果
    pub(super) raster_chart: Option<super::chart::ChartSpec>,
    /// 结构图(Mermaid 子集)
    pub(super) diagram: Option<DiagramSpec>,
    /// 卡片内容(读书卡片等)
    pub(super) cards: Vec<Card>,
    /// 引用页内容
    pub(super) quote: Option<Quote>,
    /// 公式原文(TeX),渲染时并入讲稿备注
    pub(super) formula_note: Option<String>,
    /// 预检后的正文字号
    pub(super) body_pt: u32,
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
    /// 预检诊断(阻断级,但产物仍保留)
    pub(super) diagnostics: Vec<String>,
    /// 自动分页记录:(原页 1-based, 续页数)
    pub(super) page_split: Vec<(usize, usize)>,
}

pub(crate) async fn exec(args: &Value, session_id: Option<&str>) -> (String, bool) {
    let mut deck = match parse_args(args, session_id) {
        Ok(d) => d,
        Err(e) => return (format!("演示文稿生成失败:{e}"), false),
    };
    // 出稿预检:超容量要点自动分页、字号降级与诊断,在渲染前回填,续页也参与渲染
    let (slides, report) = super::slides_preflight::run(&deck.slides, &deck.theme);
    deck.slides = slides;
    deck.warnings.extend(report.warnings);
    deck.diagnostics = report.diagnostics;
    deck.page_split = report.page_split;
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
            if !deck.warnings.is_empty() || !deck.diagnostics.is_empty() {
                let split: usize = deck.page_split.iter().map(|(_, n)| n).sum();
                let mut line = format!(
                    "预检:{} 条告警 / {} 条诊断",
                    deck.warnings.len(),
                    deck.diagnostics.len()
                );
                if split > 0 {
                    line.push_str(&format!(",自动分页 {split} 页"));
                }
                if let Some(first) = deck.diagnostics.first().or_else(|| deck.warnings.first()) {
                    line.push_str(&format!(";首条:{first}"));
                }
                msg.push('\n');
                msg.push_str(&line);
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
mod tests;
