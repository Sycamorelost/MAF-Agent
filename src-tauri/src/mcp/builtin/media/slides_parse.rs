//! `slides_export` 参数解析:把工具入参转成 [`Deck`],只做校验与截断。

use serde_json::Value;

use super::chart::{ChartSpec, ChartStyle};
use super::diagram::DiagramSpec;
use super::formula::{self, convert_text};
use super::slides::{
    Card, Deck, Kpi, Layout, MAX_KPIS, Quote, Slide, SlideChart, SlideTable, clip,
};
use super::slides_templates::lookup as lookup_template;
use super::slides_theme::{BODY_2COL_PT, BODY_IMAGE_PT, BODY_PT, BODY_TABLE_PT};

const MAX_SLIDES: usize = 30;
const MAX_TITLE_CHARS: usize = 120;
const MAX_SLIDE_TITLE_CHARS: usize = 80;
const MAX_BULLETS: usize = 10;
const MAX_BULLET_CHARS: usize = 200;
const MAX_TABLE_COLS: usize = 8;
const MAX_TABLE_ROWS: usize = 10;
const MAX_CARDS: usize = 4;
const MAX_CHART_CATEGORIES: usize = 12;
const MAX_CHART_SERIES: usize = 3;

pub(super) fn parse_args(args: &Value, session_id: Option<&str>) -> Result<Deck, String> {
    let title = clip(&text_of(args.get("title")), MAX_TITLE_CHARS);
    if title.is_empty() {
        return Err("title 不能为空".into());
    }
    let raw = args
        .get("slides")
        .and_then(Value::as_array)
        .ok_or("slides 必须是数组")?;
    if raw.is_empty() || raw.len() > MAX_SLIDES {
        return Err(format!("slides 数量需在 1..{MAX_SLIDES} 之间"));
    }
    let mut slides = Vec::with_capacity(raw.len());
    for (i, item) in raw.iter().enumerate() {
        let page = i + 1;
        // 公式先转可读文本,原始 TeX 收集进讲稿备注
        let mut raw_tex = Vec::new();
        let mut degraded = false;
        let (title_text, d) = convert_text(&text_of(item.get("title")), &mut raw_tex);
        degraded |= d;
        let st = clip(&title_text, MAX_SLIDE_TITLE_CHARS);
        if st.is_empty() {
            return Err(format!("第 {page} 页缺少 title"));
        }
        // 超长文本截断而不是报错:模型偶发写长句时仍能出稿,用户可再改
        let bullets: Vec<String> = item
            .get("bullets")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .map(|b| {
                        let (t, d) = convert_text(&text_of(Some(b)), &mut raw_tex);
                        degraded |= d;
                        clip(&t, MAX_BULLET_CHARS)
                    })
                    .filter(|b| !b.is_empty())
                    .take(MAX_BULLETS)
                    .collect()
            })
            .unwrap_or_default();
        let explicit_layout = item.get("layout").and_then(Value::as_str).and_then(Layout::parse);
        // 表格页:表头 + 数据行,单元格按 60 字截断,列/行有上限
        let table = item.get("table").and_then(|t| {
            let header: Vec<String> = t
                .get("headers")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|c| {
                            let (x, d) = convert_text(&text_of(Some(c)), &mut raw_tex);
                            degraded |= d;
                            clip(&x, 60)
                        })
                        .collect()
                })
                .unwrap_or_default();
            let rows: Vec<Vec<String>> = t
                .get("rows")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|r| r.as_array())
                        .map(|r| {
                            r.iter()
                                .take(MAX_TABLE_COLS)
                                .map(|c| {
                                    let (x, d) = convert_text(&text_of(Some(c)), &mut raw_tex);
                                    degraded |= d;
                                    clip(&x, 60)
                                })
                                .collect::<Vec<_>>()
                        })
                        .filter(|r| !r.is_empty())
                        .take(MAX_TABLE_ROWS)
                        .collect()
                })
                .unwrap_or_default();
            let cols = header
                .len()
                .max(rows.iter().map(Vec::len).max().unwrap_or(0))
                .min(MAX_TABLE_COLS);
            if cols == 0 || rows.is_empty() {
                return None;
            }
            let mut header = header;
            header.resize(cols, String::new());
            Some(SlideTable { header, rows })
        });
        let kpis = item
            .get("kpis")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|k| {
                        let value = k.get("value").and_then(Value::as_str)?.trim();
                        if value.is_empty() {
                            return None;
                        }
                        Some(Kpi {
                            value: clip(value, 16),
                            label: clip(k.get("label").and_then(Value::as_str).unwrap_or(""), 28),
                        })
                    })
                    .take(MAX_KPIS)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let chart_style = ChartStyle::parse(item.get("chart_style").and_then(Value::as_str));
        let chart = item.get("chart").and_then(parse_chart);
        // raster 模式严格校验,不截断/不丢弃
        let raster_chart = if chart_style == ChartStyle::Raster {
            if !matches!(explicit_layout, Some(Layout::Chart | Layout::Kpi)) {
                return Err(format!(
                    "第 {page} 页 chart_style=raster 需要 layout=chart 或 kpi"
                ));
            }
            let raw_chart = item
                .get("chart")
                .ok_or_else(|| format!("第 {page} 页 chart_style=raster 但缺少 chart 数据"))?;
            Some(
                ChartSpec::from_json(raw_chart)
                    .map_err(|e| format!("第 {page} 页图表数据无效:{e}"))?,
            )
        } else {
            None
        };
        let diagram = item
            .get("diagram")
            .map(|v| DiagramSpec::from_json(v).map_err(|e| format!("第 {page} 页结构图无效:{e}")))
            .transpose()?;
        // 读书卡片:最多 4 张,标题/正文超长截断
        let cards: Vec<Card> = item
            .get("cards")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| {
                        let title = clip(c.get("title").and_then(Value::as_str).unwrap_or(""), 28);
                        let text = clip(c.get("text").and_then(Value::as_str).unwrap_or(""), 120);
                        (!title.is_empty() || !text.is_empty()).then_some(Card { title, text })
                    })
                    .take(MAX_CARDS)
                    .collect()
            })
            .unwrap_or_default();
        // 引用页:对象 {text, source} 或纯字符串
        let quote = item.get("quote").and_then(|q| {
            if let Some(s) = q.as_str() {
                let text = clip(s.trim(), 160);
                return (!text.is_empty()).then_some(Quote {
                    text,
                    source: String::new(),
                });
            }
            let text = clip(q.get("text").and_then(Value::as_str).unwrap_or(""), 160);
            if text.is_empty() {
                return None;
            }
            Some(Quote {
                text,
                source: clip(q.get("source").and_then(Value::as_str).unwrap_or(""), 60),
            })
        });
        let layout = match explicit_layout {
            Some(Layout::Cards) if cards.is_empty() => {
                return Err(format!("第 {page} 页 layout=cards 需要非空 cards"));
            }
            Some(Layout::Quote) if quote.is_none() => {
                return Err(format!("第 {page} 页 layout=quote 需要 quote 文本"));
            }
            Some(l) => l,
            None if !cards.is_empty() => Layout::Cards,
            None if quote.is_some() => Layout::Quote,
            None => Layout::Bullets,
        };
        if diagram.is_some() && !matches!(layout, Layout::Diagram | Layout::ImageRight) {
            return Err(format!(
                "第 {page} 页 diagram 需要 layout=diagram 或 image-right"
            ));
        }
        let body_pt = match layout {
            Layout::TwoCol => BODY_2COL_PT,
            Layout::ImageRight => BODY_IMAGE_PT,
            Layout::Table => BODY_TABLE_PT,
            _ => BODY_PT,
        };
        let formula_note = formula::Converted {
            text: String::new(),
            raw_tex,
            degraded,
        }
        .note();
        slides.push(Slide {
            title: st,
            bullets,
            image: args_opt(item, "image"),
            notes: args_opt(item, "notes"),
            layout,
            table,
            kpis,
            chart,
            chart_style,
            raster_chart,
            diagram,
            cards,
            quote,
            formula_note,
            body_pt,
        });
    }

    let root = crate::fs::paths::resolve_workspace_root(session_id)?;
    // 场景模板:未知 id 视为未指定,不影响出包
    let template = args
        .get("template")
        .and_then(Value::as_str)
        .and_then(lookup_template);
    // 主题:白名单内的预设名;未显式指定时用模板主题,再回退商务蓝灰
    let theme = {
        let t = text_of(args.get("theme"));
        let mut chosen = match t.as_str() {
            "modern" | "tech" | "vibrant" | "nature" | "dark" | "academic" | "ocean"
            | "sunset" => t,
            _ => template
                .map(|tpl| tpl.theme.to_string())
                .unwrap_or_else(|| "corporate".to_string()),
        };
        // 版本化主题包:内联 JSON 或工作区内的 .json 文件
        if let Some(tp) = args
            .get("theme_pack")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let raw = if tp.starts_with('{') {
                tp.to_string()
            } else {
                let p = crate::fs::paths::resolve(&root, tp, true)?;
                let meta =
                    std::fs::metadata(&p).map_err(|e| format!("读取 theme_pack 失败:{e}"))?;
                if meta.len() > 256 * 1024 {
                    return Err("theme_pack 文件超过 256KB 上限".into());
                }
                std::fs::read_to_string(&p).map_err(|e| format!("读取 theme_pack 失败:{e}"))?
            };
            chosen = super::slides_theme::install_pack(&raw)?;
        }
        chosen
    };
    let explicit = text_of(args.get("path"));
    let target = if explicit.is_empty() {
        let dir = root.join("outputs").join("slides");
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {e}"))?;
        dir.join(format!("{}.pptx", slug_of(&title)))
    } else {
        let mut p = crate::fs::paths::resolve(&root, &explicit, false)?;
        if p.extension().is_none() {
            p.set_extension("pptx");
        }
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        p
    };
    if let Some(t) = template {
        t.validate(&slides)?;
    }
    let warnings = template.map(|t| t.warnings(&slides)).unwrap_or_default();
    Ok(Deck {
        title,
        subtitle: args_opt(args, "subtitle"),
        theme,
        root,
        target,
        slides,
        template,
        warnings,
        // 预检分页与诊断在渲染阶段填充;此处先置空,保证结构完整
        diagnostics: Vec::new(),
        page_split: Vec::new(),
    })
}

fn text_of(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or("").trim().to_string()
}

/// 文件名片段:保留中英文与数字,其余折叠为 `-`。
fn slug_of(title: &str) -> String {
    let mut s = String::new();
    for ch in title.chars().take(40) {
        if ch.is_alphanumeric() || ch == '-' || ch == '_' {
            s.push(ch);
        } else if !s.ends_with('-') {
            s.push('-');
        }
    }
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "slides".into() } else { s }
}

fn args_opt(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 图表页:categories + 1..3 条数值序列。
fn parse_chart(v: &Value) -> Option<SlideChart> {
    let categories: Vec<String> = v
        .get("categories")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|c| clip(&text_of(Some(c)), 24))
                .take(MAX_CHART_CATEGORIES)
                .collect()
        })
        .unwrap_or_default();
    if categories.is_empty() {
        return None;
    }
    let series: Vec<(String, Vec<f64>)> = v
        .get("series")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| {
                    let name = clip(s.get("name").and_then(Value::as_str).unwrap_or(""), 24);
                    let values: Vec<f64> = s
                        .get("values")
                        .and_then(Value::as_array)?
                        .iter()
                        .filter_map(Value::as_f64)
                        .take(categories.len())
                        .collect();
                    if values.is_empty() {
                        return None;
                    }
                    Some((name, values))
                })
                .take(MAX_CHART_SERIES)
                .collect()
        })
        .unwrap_or_default();
    if series.is_empty() {
        return None;
    }
    Some(SlideChart {
        kind: v
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("bar")
            .to_string(),
        title: clip(v.get("title").and_then(Value::as_str).unwrap_or(""), 40),
        categories,
        series,
        unit: clip(v.get("unit").and_then(Value::as_str).unwrap_or(""), 16),
        source: clip(v.get("source").and_then(Value::as_str).unwrap_or(""), 60),
    })
}
