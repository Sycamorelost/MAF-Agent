//! `refs` 单元测试(独立文件,避免主模块超过 400 行)。

use super::markdown::{self, Block};
use super::refs::{resolve_blocks, scan};
use super::style::HeadingNumbering;

fn blocks(md: &str) -> Vec<Block> {
    markdown::parse(md)
}

#[test]
fn numbers_figures_tables_equations_and_sections() {
    let bs = blocks(
        "# 第一章 {#sec-a}\n\n正文 [@fig-a] [@tbl-a] [@eq-a] [@sec-a]\n\n图 1 架构 {#fig-a}\n\n\
         表 1 参数 {#tbl-a}\n| a | b |\n|---|---|\n| 1 | 2 |\n\n$$\nE=mc^2 {#eq-a}\n$$",
    );
    let scan = scan(&bs, HeadingNumbering::Academic);
    assert_eq!(scan.lookup("fig-a").unwrap().number, 1);
    assert_eq!(scan.lookup("tbl-a").unwrap().number, 1);
    assert_eq!(scan.lookup("eq-a").unwrap().number, 1);
    assert_eq!(scan.lookup("sec-a").unwrap().number, 1);
    let (resolved, diags) = resolve_blocks(&bs, &scan);
    let text = resolved
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(t) => Some(t.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(text, "正文 图 1 表 1 式 (1) 第 1 章");
    assert!(diags.is_empty());
    // `{#...}` 标记不进入正文
    assert!(resolved.iter().all(|b| match b {
        Block::Heading { text, .. } => !text.contains("{#"),
        Block::Caption(t) => !t.contains("{#"),
        Block::Math(t) => !t.contains("{#"),
        _ => true,
    }));
}

#[test]
fn duplicate_and_unknown_keys_are_diagnostics() {
    let bs = blocks("图 1 A {#fig-x}\n\n图 2 B {#fig-x}\n\n引用 [@fig-y]");
    let scan = scan(&bs, HeadingNumbering::None);
    assert!(scan.diagnostics().iter().any(|d| d.code == "REF-DUP-KEY"));
    let (resolved, diags) = resolve_blocks(&bs, &scan);
    assert!(diags.iter().any(|d| d.code == "REF-UNKNOWN"));
    let last = resolved.last().unwrap();
    assert!(matches!(last, Block::Paragraph(t) if t.contains("[@fig-y]")));
}

#[test]
fn explicit_section_key_and_auto_bookmark() {
    let bs = blocks("# 引言 {#sec-intro}\n\n[@sec-intro]");
    let scan = scan(&bs, HeadingNumbering::Academic);
    let label = scan.lookup("sec-intro").unwrap();
    assert_eq!(label.bookmark, "_Ref_sec_intro");
    let (resolved, _) = resolve_blocks(&bs, &scan);
    assert!(matches!(&resolved[0], Block::Heading { text, .. } if text == "引言"));
}
