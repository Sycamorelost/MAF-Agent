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
                chart_style: ChartStyle::Native,
                raster_chart: None,
                diagram: None,
                cards: Vec::new(),
                quote: None,
                formula_note: None,
                body_pt: 18,
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
                chart_style: ChartStyle::Native,
                raster_chart: None,
                diagram: None,
                cards: Vec::new(),
                quote: None,
                formula_note: None,
                body_pt: 18,
            },
        ],
        template: None,
        warnings: Vec::new(),
        diagnostics: Vec::new(),
        page_split: Vec::new(),
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
