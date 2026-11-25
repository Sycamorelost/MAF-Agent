//! 幻灯片视觉部件:主题、表格、页眉页脚、指标卡与卡片网格。

use super::slides::{Deck, Kpi, MAX_KPIS, SlideTable, clip};
use super::slides_theme as token;

/// 主题名 → 预设配色/字体;未知名回退商务蓝灰。
pub(super) fn theme_of(name: &str) -> ppt_rs::generator::PresentationTheme {
    use ppt_rs::generator::PresentationTheme as T;
    if let Some(p) = token::custom_pack(name) {
        return T::from_palette(
            &p.name,
            &p.title,
            &p.text,
            &p.accent,
            &p.background,
            &p.title,
            &p.background,
            &p.muted,
        );
    }
    match name {
        "modern" => T::modern(),
        "tech" => T::tech(),
        "vibrant" => T::vibrant(),
        "nature" => T::nature(),
        "dark" => T::dark(),
        // 答辩/组会:黑灰学术风(深色标题、灰色系强调,无大面积色块)
        "academic" => T::from_palette(
            "Academic", "1F2937", "4B5563", "6B7280", "FFFFFF", "111827", "F3F4F6", "E5E7EB",
        ),
        "ocean" => T::from_palette(
            "Ocean", "0369A1", "475569", "0369A1", "F0F9FF", "0C4A6E", "F0F9FF", "082F49",
        ),
        "sunset" => T::from_palette(
            "Sunset", "C2410C", "6B7280", "C2410C", "FFF7ED", "431407", "FFF7ED", "7C2D12",
        ),
        _ => T::corporate(),
    }
}

/// 表格页:等宽列,首行表头;位置落在标题下方。
pub(super) fn table_of(t: &SlideTable) -> ppt_rs::generator::Table {
    let cols = t.header.len().max(1);
    let width = token::TABLE_W_EMU / cols as u32;
    let mut builder = ppt_rs::generator::TableBuilder::new(vec![width; cols]);
    if !t.header.is_empty() {
        builder = builder.add_simple_row(t.header.iter().map(String::as_str).collect());
    }
    for row in &t.rows {
        let mut cells: Vec<&str> = row.iter().map(String::as_str).collect();
        cells.resize(cols, "");
        builder = builder.add_simple_row(cells);
    }
    builder
        .position(token::TABLE_X_EMU, token::TABLE_Y_EMU)
        .build()
}

/// 每页统一装饰:顶部色条 + 底部分隔线 + 页脚标题/页码。
pub(super) fn chrome(deck: &Deck, page: Option<usize>) -> Vec<ppt_rs::generator::Shape> {
    use ppt_rs::generator::{Shape, ShapeFill, ShapeType};
    use ppt_rs::prelude::inches;

    let accent_color = token::accent(&deck.theme);
    let mut shapes = vec![
        Shape::new(
            ShapeType::Rectangle,
            0,
            0,
            inches(token::SLIDE_W_IN),
            inches(token::ACCENT_BAR_H_IN),
        )
        .with_fill(ShapeFill::new(&accent_color)),
    ];
    if let Some(n) = page {
        let muted = token::colors(&deck.theme).muted;
        shapes.push(
            Shape::new(
                ShapeType::Rectangle,
                inches(token::MARGIN_X_IN),
                inches(token::FOOTER_RULE_Y_IN),
                inches(token::FOOTER_RULE_W_IN),
                inches(0.012),
            )
            .with_fill(ShapeFill::new(&muted)),
        );
        shapes.push(
            Shape::new(
                ShapeType::Rectangle,
                inches(token::MARGIN_X_IN),
                inches(token::FOOTER_TEXT_Y_IN),
                inches(token::FOOTER_TEXT_W_IN),
                inches(token::FOOTER_ROW_H_IN),
            )
            .with_text(&clip(&deck.title, 40)),
        );
        shapes.push(
            Shape::new(
                ShapeType::Rectangle,
                inches(token::FOOTER_PAGE_X_IN),
                inches(token::FOOTER_TEXT_Y_IN),
                inches(token::FOOTER_PAGE_W_IN),
                inches(token::FOOTER_ROW_H_IN),
            )
            .with_text(&n.to_string()),
        );
    }
    shapes
}

/// 指标卡:浅色圆角卡片 + 大数字 + 说明。
pub(super) fn kpi_shapes(kpis: &[Kpi], accent: &str) -> Vec<ppt_rs::generator::Shape> {
    use ppt_rs::generator::{Shape, ShapeFill, ShapeType};
    use ppt_rs::prelude::inches;

    let n = kpis.len().clamp(1, MAX_KPIS);
    let gap = token::KPI_GAP_IN;
    let total_w = token::CONTENT_W_IN;
    let card_w = (total_w - gap * (n as f64 - 1.0)) / n as f64;
    let mut shapes = Vec::new();
    for (i, k) in kpis.iter().take(n).enumerate() {
        let x = token::MARGIN_X_IN + i as f64 * (card_w + gap);
        let y = token::KPI_TOP_IN;
        shapes.push(
            Shape::new(
                ShapeType::RoundedRectangle,
                inches(x),
                inches(y),
                inches(card_w),
                inches(token::KPI_CARD_H_IN),
            )
            .with_fill(ShapeFill::new(accent).with_transparency(88)),
        );
        shapes.push(
            Shape::new(
                ShapeType::Rectangle,
                inches(x + token::KPI_PAD_IN),
                inches(y + 0.35),
                inches(card_w - 2.0 * token::KPI_PAD_IN),
                inches(1.15),
            )
            .with_text(&k.value),
        );
        if !k.label.is_empty() {
            shapes.push(
                Shape::new(
                    ShapeType::Rectangle,
                    inches(x + token::KPI_PAD_IN),
                    inches(y + 1.55),
                    inches(card_w - 2.0 * token::KPI_PAD_IN),
                    inches(0.55),
                )
                .with_text(&k.label),
            );
        }
    }
    shapes
}

/// 卡片网格:2 列 × 最多 2 行,半透明底色 + 标题/正文。
pub(super) fn cards_shapes(cards: &[super::slides::Card], accent: &str) -> Vec<ppt_rs::generator::Shape> {
    use ppt_rs::generator::{Shape, ShapeFill, ShapeType};
    use ppt_rs::prelude::inches;

    let n = cards.len().clamp(1, 4);
    let gap = token::KPI_GAP_IN;
    let card_w = (token::CONTENT_W_IN - gap) / 2.0;
    let card_h = token::KPI_CARD_H_IN + 0.35;
    let top = token::KPI_TOP_IN;
    let mut shapes = Vec::new();
    for (i, c) in cards.iter().take(n).enumerate() {
        let col = i % 2;
        let row = i / 2;
        let x = token::MARGIN_X_IN + col as f64 * (card_w + gap);
        let y = top + row as f64 * (card_h + gap);
        shapes.push(
            Shape::new(
                ShapeType::RoundedRectangle,
                inches(x),
                inches(y),
                inches(card_w),
                inches(card_h),
            )
            .with_fill(ShapeFill::new(accent).with_transparency(90)),
        );
        if !c.title.is_empty() {
            shapes.push(
                Shape::new(
                    ShapeType::Rectangle,
                    inches(x + token::KPI_PAD_IN),
                    inches(y + 0.18),
                    inches(card_w - 2.0 * token::KPI_PAD_IN),
                    inches(0.5),
                )
                .with_text(&c.title),
            );
        }
        if !c.text.is_empty() {
            shapes.push(
                Shape::new(
                    ShapeType::Rectangle,
                    inches(x + token::KPI_PAD_IN),
                    inches(y + 0.72),
                    inches(card_w - 2.0 * token::KPI_PAD_IN),
                    inches(card_h - 0.9),
                )
                .with_text(&c.text),
            );
        }
    }
    shapes
}
