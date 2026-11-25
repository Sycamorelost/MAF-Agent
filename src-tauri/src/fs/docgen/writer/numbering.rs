//! 原生多级编号:标题与列表改用 `numbering.xml` + `w:numPr`。
//!
//! docx-rs 0.4.22 暴露 `AbstractNumbering` / `Numbering` / `Paragraph::numbering`,
//! 不需要手写原始 XML。`Numberings::build` 会固定写入 abstractNumId=1、numId=1,
//! 因此本模块从 100 起分配,避免冲突。
//!
//! 仅在模板声明 `native_numbering` 时启用(thesis/book),既有 8 个模板继续用
//! 文本前缀方案,保证输出兼容。用户手写「一、」「1.1」「第 X 章」时跳过自动编号。

use docx_rs::{
    AbstractNumbering, Docx, IndentLevel, Level, LevelJc, LevelSuffixType, LevelText, NumberFormat,
    Numbering, NumberingId, Paragraph, SpecialIndentType, Start,
};

use super::super::style::HeadingNumbering;

const HEADING_ID: usize = 100;
const BULLET_ID: usize = 101;
const DECIMAL_ID: usize = 102;

/// 分配到的编号 id。
#[derive(Debug, Clone, Copy)]
pub(super) struct NumberingIds {
    pub heading: Option<usize>,
    pub bullet: usize,
    pub decimal: usize,
}

impl NumberingIds {
    /// 无原生编号时使用的空占位。
    pub fn none() -> Self {
        NumberingIds {
            heading: None,
            bullet: 0,
            decimal: 0,
        }
    }
}

/// 向文档注册标题/项目符号/有序列表三套编号定义。
pub(super) fn register(docx: Docx, kind: HeadingNumbering) -> (Docx, NumberingIds) {
    let mut docx = docx;
    let heading = if kind == HeadingNumbering::None {
        None
    } else {
        docx = docx
            .add_abstract_numbering(heading_abstract(kind))
            .add_numbering(Numbering::new(HEADING_ID, HEADING_ID));
        Some(HEADING_ID)
    };
    docx = docx
        .add_abstract_numbering(bullet_abstract())
        .add_numbering(Numbering::new(BULLET_ID, BULLET_ID))
        .add_abstract_numbering(decimal_abstract())
        .add_numbering(Numbering::new(DECIMAL_ID, DECIMAL_ID));
    (
        docx,
        NumberingIds {
            heading,
            bullet: BULLET_ID,
            decimal: DECIMAL_ID,
        },
    )
}

/// 某一级的 (numFmt, lvlText)。
fn level_text(kind: HeadingNumbering, level: usize) -> (&'static str, &'static str) {
    const ACADEMIC: [(&str, &str); 4] = [
        ("decimal", "%1"),
        ("decimal", "%1.%2"),
        ("decimal", "%1.%2.%3"),
        ("decimal", "（%4）"),
    ];
    const OFFICIAL: [(&str, &str); 4] = [
        ("chineseCounting", "%1、"),
        ("chineseCounting", "（%2）"),
        ("decimal", "%3."),
        ("decimal", "（%4）"),
    ];
    const SIMPLE: [(&str, &str); 4] = [
        ("chineseCounting", "%1、"),
        ("decimal", "%2."),
        ("decimal", "（%3）"),
        ("decimal", "（%4）"),
    ];
    let table = match kind {
        HeadingNumbering::Academic => ACADEMIC,
        HeadingNumbering::Official => OFFICIAL,
        HeadingNumbering::Simple => SIMPLE,
        HeadingNumbering::None => ACADEMIC,
    };
    table[level.min(3)]
}

fn heading_abstract(kind: HeadingNumbering) -> AbstractNumbering {
    let mut abs = AbstractNumbering::new(HEADING_ID);
    for level in 0..4 {
        let (fmt, text) = level_text(kind, level);
        abs = abs.add_level(
            Level::new(
                level,
                Start::new(1),
                NumberFormat::new(fmt),
                LevelText::new(text),
                LevelJc::new("left"),
            )
            .indent(Some(0), Some(SpecialIndentType::FirstLine(0)), None, None)
            .suffix(LevelSuffixType::Space),
        );
    }
    abs
}

fn bullet_abstract() -> AbstractNumbering {
    let mut abs = AbstractNumbering::new(BULLET_ID);
    for level in 0..4 {
        let indent = 420 * (level as i32 + 1);
        abs = abs.add_level(
            Level::new(
                level,
                Start::new(1),
                NumberFormat::new("bullet"),
                LevelText::new("•"),
                LevelJc::new("left"),
            )
            .indent(
                Some(indent),
                Some(SpecialIndentType::Hanging(420)),
                None,
                None,
            )
            .suffix(LevelSuffixType::Space),
        );
    }
    abs
}

fn decimal_abstract() -> AbstractNumbering {
    let mut abs = AbstractNumbering::new(DECIMAL_ID);
    for level in 0..4 {
        let indent = 420 * (level as i32 + 1);
        abs = abs.add_level(
            Level::new(
                level,
                Start::new(1),
                NumberFormat::new("decimal"),
                LevelText::new("%1."),
                LevelJc::new("left"),
            )
            .indent(
                Some(indent),
                Some(SpecialIndentType::Hanging(420)),
                None,
                None,
            )
            .suffix(LevelSuffixType::Space),
        );
    }
    abs
}

/// 给标题段落挂 `w:numPr`;`level` 从 0 起。
pub(super) fn apply_heading(p: Paragraph, ids: &NumberingIds, level: usize) -> Paragraph {
    match ids.heading {
        Some(id) => p.numbering(NumberingId::new(id), IndentLevel::new(level.min(3))),
        None => p,
    }
}

/// 给列表段落挂 `w:numPr`。
pub(super) fn apply_list(
    p: Paragraph,
    ids: &NumberingIds,
    ordered: bool,
    level: usize,
) -> Paragraph {
    let id = if ordered { ids.decimal } else { ids.bullet };
    if id == 0 {
        return p;
    }
    p.numbering(NumberingId::new(id), IndentLevel::new(level.min(3)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn academic_and_official_level_texts() {
        assert_eq!(
            level_text(HeadingNumbering::Academic, 1),
            ("decimal", "%1.%2")
        );
        assert_eq!(
            level_text(HeadingNumbering::Official, 0),
            ("chineseCounting", "%1、")
        );
        assert_eq!(
            level_text(HeadingNumbering::Simple, 2),
            ("decimal", "（%3）")
        );
    }

    #[test]
    fn register_allocates_ids_and_skips_heading_for_none() {
        let (_, ids) = register(Docx::new(), HeadingNumbering::Academic);
        assert_eq!(ids.heading, Some(HEADING_ID));
        assert_eq!(ids.bullet, BULLET_ID);
        assert_eq!(ids.decimal, DECIMAL_ID);
        let (_, none) = register(Docx::new(), HeadingNumbering::None);
        assert!(none.heading.is_none());
    }

    #[test]
    fn apply_heading_adds_numbering_property() {
        let ids = NumberingIds {
            heading: Some(HEADING_ID),
            bullet: BULLET_ID,
            decimal: DECIMAL_ID,
        };
        let dbg = format!("{:?}", apply_heading(Paragraph::new(), &ids, 1));
        assert!(dbg.contains("has_numbering: true"), "{dbg}");
    }
}
