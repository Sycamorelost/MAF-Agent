//! 幻灯片渲染层:主题、页眉页脚、表格/图表/指标卡版式与 pptx 写盘。
//!
//! 与 `slides.rs` 的分工:上者只做参数解析与回退大纲,这里只依赖已解析的
//! [`Deck`] 产出 `.pptx`,便于单独调整视觉而不碰解析逻辑。

use serde_json::json;

use super::slides::{
    clip, file_name, Deck, Kpi, Layout, SlideChart, SlideTable, MAX_KPIS,
};

/// 主题名 → 预设配色/字体;未知名回退商务蓝灰。
fn theme_of(name: &str) -> ppt_rs::generator::PresentationTheme {
    use ppt_rs::generator::PresentationTheme as T;
    match name {
        "modern" => T::modern(),
        "tech" => T::tech(),
        "vibrant" => T::vibrant(),
        "nature" => T::nature(),
        "dark" => T::dark(),
        // 答辩/组会:黑灰学术风(深色标题、灰色系强调,无大面积色块)
        "academic" => T::from_palette(
            "Academic",
            "1F2937",
            "4B5563",
            "6B7280",
            "FFFFFF",
            "111827",
            "F3F4F6",
            "E5E7EB",
        ),
        _ => T::corporate(),
    }
}

/// 表格页:等宽列,首行表头;位置落在标题下方。
fn table_of(t: &SlideTable) -> ppt_rs::generator::Table {
    let cols = t.header.len().max(1);
    let width = 10_600_000 / cols as u32;
    let mut builder = ppt_rs::generator::TableBuilder::new(vec![width; cols]);
    if !t.header.is_empty() {
        builder = builder.add_simple_row(t.header.iter().map(String::as_str).collect());
    }
    for row in &t.rows {
        let mut cells: Vec<&str> = row.iter().map(String::as_str).collect();
        cells.resize(cols, "");
        builder = builder.add_simple_row(cells);
    }
    builder.position(700_000, 1_900_000).build()
}

/// 主题强调色(用于顶部色条、指标卡与图表)
fn accent_of(theme: &str) -> &'static str {
    match theme {
        "modern" => "2563EB",
        "tech" => "0EA5E9",
        "vibrant" => "EA580C",
        "nature" => "15803D",
        "dark" => "F59E0B",
        "academic" => "374151",
        _ => "2F5597",
    }
}

/// 每页统一装饰:顶部色条 + 底部分隔线 + 页脚标题/页码。
fn chrome(deck: &Deck, page: Option<usize>) -> Vec<ppt_rs::generator::Shape> {
    use ppt_rs::generator::{Shape, ShapeFill, ShapeType};
    use ppt_rs::prelude::inches;

    let mut shapes = vec![
        Shape::new(ShapeType::Rectangle, 0, 0, 12_192_000, inches(0.075))
            .with_fill(ShapeFill::new(accent_of(&deck.theme))),
    ];
    if let Some(n) = page {
        shapes.push(
            Shape::new(
                ShapeType::Rectangle,
                inches(0.6),
                inches(6.55),
                inches(12.1),
                inches(0.012),
            )
            .with_fill(ShapeFill::new("D9D9D9")),
        );
        shapes.push(
            Shape::new(
                ShapeType::Rectangle,
                inches(0.6),
                inches(6.63),
                inches(9.0),
                inches(0.26),
            )
            .with_text(&clip(&deck.title, 40)),
        );
        shapes.push(
            Shape::new(
                ShapeType::Rectangle,
                inches(11.95),
                inches(6.63),
                inches(0.75),
                inches(0.26),
            )
            .with_text(&n.to_string()),
        );
    }
    shapes
}

/// 指标卡:浅色圆角卡片 + 大数字 + 说明。
fn kpi_shapes(kpis: &[Kpi], accent: &str) -> Vec<ppt_rs::generator::Shape> {
    use ppt_rs::generator::{Shape, ShapeFill, ShapeType};
    use ppt_rs::prelude::inches;

    let n = kpis.len().clamp(1, MAX_KPIS);
    let gap = 0.25;
    let total_w = 12.1;
    let card_w = (total_w - gap * (n as f64 - 1.0)) / n as f64;
    let mut shapes = Vec::new();
    for (i, k) in kpis.iter().take(n).enumerate() {
        let x = 0.6 + i as f64 * (card_w + gap);
        let y = 2.15;
        shapes.push(
            Shape::new(
                ShapeType::RoundedRectangle,
                inches(x),
                inches(y),
                inches(card_w),
                inches(2.3),
            )
            .with_fill(ShapeFill::new(accent).with_transparency(88)),
        );
        shapes.push(
            Shape::new(
                ShapeType::Rectangle,
                inches(x + 0.2),
                inches(y + 0.35),
                inches(card_w - 0.4),
                inches(1.15),
            )
            .with_text(&k.value),
        );
        if !k.label.is_empty() {
            shapes.push(
                Shape::new(
                    ShapeType::Rectangle,
                    inches(x + 0.2),
                    inches(y + 1.55),
                    inches(card_w - 0.4),
                    inches(0.55),
                )
                .with_text(&k.label),
            );
        }
    }
    shapes
}

fn chart_of(c: &SlideChart, fallback_title: &str) -> Result<ppt_rs::generator::Chart, String> {
    use ppt_rs::generator::{ChartBuilder, ChartSeries, ChartType};
    use ppt_rs::prelude::inches;

    let kind = match c.kind.to_ascii_lowercase().as_str() {
        "line" | "trend" => ChartType::LineMarkers,
        "pie" => ChartType::Pie,
        "doughnut" | "ring" => ChartType::Doughnut,
        "area" => ChartType::Area,
        "hbar" | "bar-horizontal" => ChartType::BarHorizontal,
        "stacked" => ChartType::BarStacked,
        _ => ChartType::Bar,
    };
    let cats: Vec<&str> = c.categories.iter().map(String::as_str).collect();
    let title = if c.title.is_empty() {
        fallback_title
    } else {
        &c.title
    };
    let mut builder = ChartBuilder::new(title, kind)
        .position(inches(0.7), inches(1.55))
        .size(inches(11.9), inches(4.7))
        .categories(cats);
    for (name, values) in &c.series {
        builder = builder.add_series(ChartSeries::new(name, values.clone()));
    }
    builder.build().map_err(|e| format!("图表生成失败: {e}"))
}

pub(super) fn build_and_write(deck: &Deck, session_id: Option<&str>) -> Result<String, String> {
    use ppt_rs::generator::SlideLayout;

    let mut pres = ppt_rs::Presentation::with_title(&deck.title).with_theme(theme_of(&deck.theme));
    // 封面:居中大标题 + 可选副标题 + 顶部色条
    let mut cover = ppt_rs::SlideContent::new(&deck.title)
        .layout(SlideLayout::CenteredTitle)
        .title_size(40)
        .content_size(18)
        .with_shapes(chrome(deck, None));
    if let Some(sub) = &deck.subtitle {
        cover = cover.add_bullet(sub);
    }
    pres = pres.add_slide(cover);

    let accent = accent_of(&deck.theme);
    for (i, s) in deck.slides.iter().enumerate() {
        let page = i + 2;
        let mut sc = match s.layout {
            Layout::Section => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::SectionHeader)
                .title_size(36),
            Layout::Closing => ppt_rs::SlideContent::new(if s.title.trim().is_empty() {
                "谢谢"
            } else {
                s.title.as_str()
            })
            .layout(SlideLayout::CenteredTitle)
            .title_size(36),
            Layout::TwoCol => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::TwoColumn)
                .title_size(28)
                .content_size(16),
            Layout::Kpi => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::TitleOnly)
                .title_size(28),
            Layout::Chart => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::TitleOnly)
                .title_size(28),
            Layout::Table => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::TitleAndContent)
                .title_size(28)
                .content_size(14),
            Layout::ImageRight => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::TitleAndContent)
                .title_size(28)
                .content_size(17),
            Layout::Bullets => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::TitleAndContent)
                .title_size(28)
                .content_size(18),
        };
        // 指标卡 / 图表页:把要点降为讲稿,避免文字压图
        let mut extra_notes = String::new();
        if matches!(s.layout, Layout::Kpi | Layout::Chart) {
            if !s.bullets.is_empty() {
                extra_notes = s.bullets.join("\n");
            }
            if !s.kpis.is_empty() {
                sc = sc.with_shapes(kpi_shapes(&s.kpis, accent));
            }
            if let Some(c) = &s.chart {
                sc = sc.add_chart(chart_of(c, &s.title)?);
            }
        } else {
            for b in &s.bullets {
                // 「- 」前缀 = 二级要点,便于一页承载更多信息
                if let Some(sub) = b.strip_prefix("- ") {
                    sc = sc.add_sub_bullet(sub);
                } else {
                    sc = sc.add_bullet(b);
                }
            }
        }
        if let Some(t) = &s.table {
            sc = sc.table(table_of(t));
        }
        if let Some(img) = &s.image {
            let path = crate::fs::paths::resolve(&deck.root, img, true)?;
            sc = sc.add_image(ppt_rs::Image::from_path(&path)?);
            // 右图版式保留默认图文分区;其余图文同页时上下堆叠,避免图片压住要点
            if s.layout != Layout::ImageRight {
                sc.layout_stacked_content();
            }
        } else if s.table.is_some() {
            sc.layout_stacked_content();
        }
        let notes = match (&s.notes, extra_notes.is_empty()) {
            (Some(n), true) => Some(n.clone()),
            (Some(n), false) => Some(format!("{n}\n{extra_notes}")),
            (None, false) => Some(extra_notes),
            (None, true) => None,
        };
        if let Some(n) = notes {
            sc = sc.notes(&n);
        }
        sc = sc.with_shapes(chrome(deck, Some(page)));
        pres = pres.add_slide(sc);
    }
    let bytes = pres.build().map_err(|e| format!("生成 pptx 失败:{e}"))?;
    std::fs::write(&deck.target, &bytes).map_err(|e| format!("写入 pptx 失败: {e}"))?;

    let rel = crate::fs::paths::rel_of(&deck.root, &deck.target)
        .unwrap_or_else(|_| deck.target.display().to_string().replace('\\', "/"));
    let (note, ok) = crate::deliverables::present(
        session_id,
        &json!({ "path": rel, "kind": "slides", "name": file_name(&deck.target, "pptx") }),
    );
    if !ok {
        log::warn!("登记演示文稿产物失败: {note}");
    }
    Ok(format!(
        "已生成演示文稿:{rel}(共 {} 页含封面,{} KB){}",
        deck.slides.len() + 1,
        bytes.len() / 1024,
        if ok { ",已登记产物" } else { "" }
    ))
}
