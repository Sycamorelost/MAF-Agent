//! 幻灯片渲染层:主题、页眉页脚、表格/图表/指标卡版式与 pptx 写盘。
//!
//! 与 `slides.rs` 的分工:上者只做参数解析与回退大纲,这里只依赖已解析的
//! [`Deck`] 产出 `.pptx`,便于单独调整视觉而不碰解析逻辑。

use serde_json::json;
use std::path::{Path, PathBuf};

use super::slides::{Deck, Layout, SlideChart, file_name};
use super::slides_chrome::{cards_shapes, chrome, kpi_shapes, table_of, theme_of};
use super::slides_theme as token;

/// 光栅图(chart raster / diagram)先落临时文件再交给 ppt-rs 按路径引用;
/// build 完成后统一清理。返回临时文件路径并登记到 `bag`。
fn write_temp_png(deck: &Deck, bag: &mut Vec<PathBuf>, bytes: &[u8]) -> Result<PathBuf, String> {
    let dir = deck.target.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| format!("创建图片目录失败: {e}"))?;
    let p = dir.join(format!(
        ".slides-img-{}-{}.png",
        std::process::id(),
        bag.len()
    ));
    std::fs::write(&p, bytes).map_err(|e| format!("写入临时图片失败: {e}"))?;
    bag.push(p.clone());
    Ok(p)
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
        .position(inches(token::CHART_X_IN), inches(token::CHART_Y_IN))
        .size(inches(token::CHART_W_IN), inches(token::CHART_H_IN))
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
        .title_size(token::COVER_TITLE_PT)
        .content_size(token::COVER_SUB_PT)
        .with_shapes(chrome(deck, None));
    if let Some(sub) = &deck.subtitle {
        cover = cover.add_bullet(sub);
    }
    let colors = token::colors(&deck.theme);
    cover = cover
        .title_color(&colors.title)
        .content_color(&colors.text);
    pres = pres.add_slide(cover);

    let accent = token::accent(&deck.theme);
    // 光栅图临时文件:build 读取后统一删除
    let mut temp_imgs: Vec<PathBuf> = Vec::new();
    for (i, s) in deck.slides.iter().enumerate() {
        let page = i + 2;
        let mut sc = match s.layout {
            Layout::Section => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::SectionHeader)
                .title_size(token::SECTION_TITLE_PT),
            Layout::Closing => ppt_rs::SlideContent::new(if s.title.trim().is_empty() {
                "谢谢"
            } else {
                s.title.as_str()
            })
            .layout(SlideLayout::CenteredTitle)
            .title_size(token::CLOSING_TITLE_PT),
            Layout::TwoCol => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::TwoColumn)
                .title_size(token::TITLE_PT)
                .content_size(16),
            Layout::Kpi => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::TitleOnly)
                .title_size(token::TITLE_PT),
            Layout::Chart => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::TitleOnly)
                .title_size(token::TITLE_PT),
            Layout::Table => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::TitleAndContent)
                .title_size(token::TITLE_PT)
                .content_size(14),
            Layout::ImageRight => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::TitleAndContent)
                .title_size(token::TITLE_PT)
                .content_size(17),
            // 结构图页:与图文页同版心,图示 PNG 在后续插图阶段写入
            Layout::Diagram => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::TitleAndContent)
                .title_size(token::TITLE_PT)
                .content_size(17),
            Layout::Bullets => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::TitleAndContent)
                .title_size(token::TITLE_PT)
                .content_size(18),
            // 卡片页:标题栏 + 2×N 卡片网格
            Layout::Cards => ppt_rs::SlideContent::new(&s.title)
                .layout(SlideLayout::TitleOnly)
                .title_size(token::TITLE_PT),
            // 引用页:引文即主标题,出处作副行
            Layout::Quote => {
                let text = s
                    .quote
                    .as_ref()
                    .map(|q| q.text.as_str())
                    .unwrap_or(s.title.as_str());
                ppt_rs::SlideContent::new(text)
                    .layout(SlideLayout::CenteredTitle)
                    .title_size(token::CLOSING_TITLE_PT)
                    .content_size(16)
            }
        };
        let colors = token::colors(&deck.theme);
        sc = sc
            .title_color(&colors.title)
            .content_color(&colors.text)
            .content_size(s.body_pt);
        // 指标卡 / 图表页:把要点降为讲稿,避免文字压图
        let mut extra_notes = String::new();
        if matches!(
            s.layout,
            Layout::Kpi | Layout::Chart | Layout::Cards | Layout::Quote
        ) {
            if !s.bullets.is_empty() {
                extra_notes = s.bullets.join("\n");
            }
            if s.layout == Layout::Quote {
                if !s.title.trim().is_empty() {
                    extra_notes = format!("{}\n{}", s.title, extra_notes).trim().to_string();
                }
                if let Some(q) = &s.quote
                    && !q.source.is_empty()
                {
                    sc = sc.add_bullet(&q.source);
                }
            }
            if s.layout == Layout::Cards && !s.cards.is_empty() {
                sc = sc.with_shapes(cards_shapes(&s.cards, &accent));
            }
            if !s.kpis.is_empty() {
                sc = sc.with_shapes(kpi_shapes(&s.kpis, &accent));
            }
            if let Some(c) = &s.chart {
                let footnote = if s.chart_style == super::chart::ChartStyle::Raster {
                    s.raster_chart.as_ref().and_then(|spec| spec.footnote())
                } else {
                    super::chart::footnote(&c.unit, &c.source)
                };
                // raster:用 charts-rs 出 PNG 插图;native:ppt-rs 原生图表(默认,兼容旧输出)
                if s.chart_style == super::chart::ChartStyle::Raster {
                    let spec = s
                        .raster_chart
                        .as_ref()
                        .ok_or("chart_style=raster 但缺少有效图表数据")?;
                    let png = spec.render_png()?;
                    let p = write_temp_png(deck, &mut temp_imgs, &png)?;
                    sc = sc.add_image(ppt_rs::Image::from_path(&p)?);
                } else {
                    sc = sc.add_chart(chart_of(c, &s.title)?);
                }
                if let Some(text) = footnote {
                    use ppt_rs::generator::{Shape, ShapeFill, ShapeType};
                    use ppt_rs::prelude::inches;
                    sc = sc.add_shape(
                        Shape::new(
                            ShapeType::Rectangle,
                            inches(token::MARGIN_X_IN),
                            inches(token::FOOTNOTE_Y_IN),
                            inches(token::CONTENT_W_IN),
                            inches(token::FOOTER_ROW_H_IN),
                        )
                        .with_fill(ShapeFill::new(&colors.muted).with_transparency(92))
                        .with_text(&text),
                    );
                }
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
        // 结构图(Mermaid 子集):charts-rs 渲染 PNG 插入,非法语法在解析/渲染阶段已报错
        if let Some(spec) = &s.diagram {
            let png = spec.render_png()?;
            let p = write_temp_png(deck, &mut temp_imgs, &png)?;
            let mut image = ppt_rs::Image::from_path(&p)?;
            let ratio = image.width as f64 / image.height.max(1) as f64;
            image.height = ppt_rs::prelude::inches(token::DIAGRAM_H_IN);
            image.width = (image.height as f64 * ratio).round() as u32;
            sc = sc.add_image(image);
            sc.layout_stacked_content();
        }
        let mut note_parts = Vec::new();
        if let Some(n) = &s.notes {
            note_parts.push(n.as_str());
        }
        if !extra_notes.is_empty() {
            note_parts.push(extra_notes.as_str());
        }
        if let Some(n) = &s.formula_note {
            note_parts.push(n.as_str());
        }
        let notes = if note_parts.is_empty() {
            None
        } else {
            Some(note_parts.join("\n"))
        };
        if let Some(n) = notes {
            sc = sc.notes(&n);
        }
        sc = sc.with_shapes(chrome(deck, Some(page)));
        pres = pres.add_slide(sc);
    }
    let bytes = pres.build().map_err(|e| format!("生成 pptx 失败:{e}"))?;
    // 包级校验:解包检查关系/引用;失败不阻断交付,但写入回执便于排查
    let validation = super::slides_validate::validate_pptx(&bytes);
    // build 已把图片字节读入包内,临时文件可清理
    for p in &temp_imgs {
        let _ = std::fs::remove_file(p);
    }
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
    let mut msg = format!(
        "已生成演示文稿:{rel}(共 {} 页含封面,{} KB){}",
        deck.slides.len() + 1,
        bytes.len() / 1024,
        if ok { ",已登记产物" } else { "" }
    );
    if let Err(errs) = validation {
        msg.push_str(&format!(";包校验 {} 处问题:{}", errs.len(), errs.join(";")));
    }
    Ok(msg)
}
