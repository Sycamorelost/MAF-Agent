//! 块解析回归测试:只验证 `parse` 的中立输出,不涉渲染。

use super::*;

#[test]
fn parses_headings_levels() {
    let blocks = parse("# 标题\n## 二级\n### 三级");
    assert_eq!(
        blocks,
        vec![
            Block::Heading {
                level: 1,
                text: "标题".into()
            },
            Block::Heading {
                level: 2,
                text: "二级".into()
            },
            Block::Heading {
                level: 3,
                text: "三级".into()
            },
        ]
    );
}

#[test]
fn joins_soft_wrapped_paragraph_lines() {
    let blocks = parse("第一行\n第二行\n\n另一段");
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0], Block::Paragraph("第一行第二行".into()));
}

#[test]
fn parses_nested_lists() {
    let blocks = parse("1. 第一步\n  - 子项\n2. 第二步");
    match &blocks[0] {
        Block::List { ordered, items } => {
            assert!(ordered);
            assert_eq!(items.len(), 3);
            assert_eq!(items[1].level, 1);
            assert_eq!(items[1].text, "子项");
        }
        other => panic!("expect list, got {other:?}"),
    }
}

#[test]
fn parses_table_with_alignment_and_caption() {
    let md = "表 1 各轮结果\n| 模型 | 时间 | 得分 |\n|:---|:---:|---:|\n| A | 1.2 s | 90 |";
    let blocks = parse(md);
    match &blocks[0] {
        Block::Table {
            caption,
            header,
            rows,
            aligns,
        } => {
            assert_eq!(caption.as_deref(), Some("表 1 各轮结果"));
            assert_eq!(header.len(), 3);
            assert_eq!(rows.len(), 1);
            assert_eq!(aligns, &[Align::Left, Align::Center, Align::Right]);
        }
        other => panic!("expect table, got {other:?}"),
    }
}

#[test]
fn parses_fenced_code() {
    let blocks = parse("```python\nprint(1)\n```");
    assert_eq!(
        blocks[0],
        Block::Code {
            lang: "python".into(),
            lines: vec!["print(1)".into()]
        }
    );
}

#[test]
fn parses_image_and_page_break() {
    let blocks = parse("![架构图](img/a.png)\n\n\\pagebreak\n\n正文");
    assert_eq!(
        blocks[0],
        Block::Image {
            alt: "架构图".into(),
            src: "img/a.png".into()
        }
    );
    assert_eq!(blocks[1], Block::PageBreak);
}

#[test]
fn caption_before_table_is_attached() {
    let blocks = parse("表 2-1 参数\n\n| a | b |\n| --- | --- |\n| 1 | 2 |");
    assert_eq!(blocks.len(), 1);
    assert!(matches!(blocks[0], Block::Table { .. }));
}

#[test]
fn standalone_caption_is_kept() {
    let blocks = parse("图 1 界面截图");
    assert_eq!(blocks[0], Block::Caption("图 1 界面截图".into()));
}

#[test]
fn parses_abstract_and_keywords() {
    let blocks = parse("**摘要**：本文研究排课问题。\n\n关键词：约束满足；回溯搜索");
    assert_eq!(blocks[0], Block::Abstract("本文研究排课问题。".into()));
    assert_eq!(blocks[1], Block::Keywords("约束满足；回溯搜索".into()));
}

#[test]
fn parses_display_math_blocks() {
    let blocks = parse("$$E = mc^2$$\n\n$$\n\\sum_{i=1}^{n} x_i\n$$");
    assert_eq!(blocks[0], Block::Math("E = mc^2".into()));
    assert_eq!(blocks[1], Block::Math("\\sum_{i=1}^{n} x_i".into()));
}

#[test]
fn parses_display_math_blocks_single_line_pair() {
    let blocks = parse("$$\n\\frac{a}{b}\n$$");
    assert_eq!(blocks[0], Block::Math("\\frac{a}{b}".into()));
}

#[test]
fn references_after_heading_use_reference_block() {
    let blocks = parse(
        "## 参考文献\n\n[1] 张三. 组合优化[M]. 北京: 科学出版社, 2020.\n\n2. 李四. CSP 综述[J]. 2021.",
    );
    // 标题保留,其后每个条目一条 Reference
    assert!(matches!(blocks[0], Block::Heading { .. }));
    assert_eq!(blocks.len(), 3);
    assert_eq!(
        blocks[1],
        Block::Reference("[1] 张三. 组合优化[M]. 北京: 科学出版社, 2020.".into())
    );
    assert_eq!(
        blocks[2],
        Block::Reference("李四. CSP 综述[J]. 2021.".into())
    );
}

#[test]
fn reference_mode_stops_at_next_heading() {
    let blocks = parse("## 参考文献\n[1] A\n\n## 附录\n正文");
    assert_eq!(blocks.len(), 4);
    assert_eq!(
        blocks[2],
        Block::Heading {
            level: 2,
            text: "附录".into()
        }
    );
    assert_eq!(blocks[3], Block::Paragraph("正文".into()));
}
