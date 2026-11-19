//! `slides_export` 参数解析:把工具入参转成 [`Deck`],只做校验与截断。

use serde_json::Value;

use super::slides::{clip, Deck, Kpi, Layout, Slide, SlideChart, SlideTable, MAX_KPIS};
use super::slides_templates::lookup as lookup_template;

const MAX_SLIDES: usize = 30;
const MAX_TITLE_CHARS: usize = 120;
const MAX_SLIDE_TITLE_CHARS: usize = 80;
const MAX_BULLETS: usize = 10;
const MAX_BULLET_CHARS: usize = 200;
const MAX_TABLE_COLS: usize = 8;
const MAX_TABLE_ROWS: usize = 10;
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
        let st = clip(&text_of(item.get("title")), MAX_SLIDE_TITLE_CHARS);
        if st.is_empty() {
            return Err(format!("第 {page} 页缺少 title"));
        }
        // 超长文本截断而不是报错:模型偶发写长句时仍能出稿,用户可再改
        let bullets: Vec<String> = item
            .get("bullets")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .map(|b| clip(&text_of(Some(b)), MAX_BULLET_CHARS))
                    .filter(|b| !b.is_empty())
                    .take(MAX_BULLETS)
                    .collect()
            })
            .unwrap_or_default();
        let layout = item
            .get("layout")
            .and_then(Value::as_str)
            .and_then(Layout::parse)
            .unwrap_or(Layout::Bullets);
        // 表格页:表头 + 数据行,单元格按 60 字截断,列/行有上限
        let table = item.get("table").and_then(|t| {
            let header: Vec<String> = t
                .get("headers")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(|c| clip(&text_of(Some(c)), 60)).collect())
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
                                .map(|c| clip(&text_of(Some(c)), 60))
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
                            label: clip(
                                k.get("label").and_then(Value::as_str).unwrap_or(""),
                                28,
                            ),
                        })
                    })
                    .take(MAX_KPIS)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let chart = item.get("chart").and_then(|c| parse_chart(c));
        slides.push(Slide {
            title: st,
            bullets,
            image: args_opt(item, "image"),
            notes: args_opt(item, "notes"),
            layout,
            table,
            kpis,
            chart,
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
        match t.as_str() {
            "modern" | "tech" | "vibrant" | "nature" | "dark" | "academic" => t,
            _ => template
                .map(|tpl| tpl.theme.to_string())
                .unwrap_or_else(|| "corporate".to_string()),
        }
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
    let warnings = template
        .map(|t| t.warnings(&slides))
        .unwrap_or_default();
    Ok(Deck {
        title,
        subtitle: args_opt(args, "subtitle"),
        theme,
        root,
        target,
        slides,
        template,
        warnings,
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
    })
}
