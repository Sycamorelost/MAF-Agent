//! 书稿模板:支持「# 第 1 章 …」形式的章清单与章首页规则。
//!
//! 只声明数据与轻量提取函数;编号是否生效由 writer 统一处理:
//! 用户手写「第 1 章」时 `has_manual` 为真,自动跳过编号。

use super::super::markdown::Block;
use super::super::profile::Profile;
use super::super::style::{DocStyle, HeadingNumbering};
use super::{Template, TemplateSpec};

pub(super) const TEMPLATE: Template = Template {
    id: "book",
    name: "书稿",
    profile: Profile::Paper,
    // 章标题由作者手写「第 N 章」,自动编号关闭,避免重复
    numbering: HeadingNumbering::None,
    keys: &["书稿", "图书", "第 1 章", "第一章", "前言", "后记"],
    skeleton: &["前言", "第 1 章", "第 2 章", "第 3 章", "后记", "参考文献"],
    hints: "书稿:每章从奇数页起排,章首页无页眉;章标题写「# 第 N 章 标题」,小节用 1.1/1.1.1。",
};

pub(super) const SPEC: TemplateSpec = TemplateSpec {
    cover_fields: &["书名", "作者", "出版社", "版次", "日期"],
    declaration_page: false,
    native_numbering: true,
    sectioned: true,
    chapter_list: true,
    chapter_first_page: true,
    endnotes: true,
};

/// 版式覆盖:书稿正文首行缩进 2 字符、行距稍松。
pub(super) fn apply(style: &mut DocStyle) {
    style.body_indent = 480;
    style.fixed_line = None;
    style.body_line = 360;
}

/// 章标题识别(「第 N 章 …」/「Chapter N …」)。
#[allow(dead_code)] // 声明式数据 API,供写作提示与后续装配复用
pub fn is_chapter_heading(text: &str) -> bool {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix('第') {
        let rest = rest.trim_start();
        if let Some(pos) = rest.find('章') {
            let num = rest[..pos].trim();
            return !num.is_empty()
                && num
                    .chars()
                    .all(|c| c.is_ascii_digit() || "一二三四五六七八九十百零〇".contains(c));
        }
        return false;
    }
    t.to_ascii_lowercase().starts_with("chapter ")
}

/// 章清单:按出现顺序返回一级标题,供目录/装配使用。
#[allow(dead_code)]
pub fn chapters(blocks: &[Block]) -> Vec<String> {
    let top = blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading { level, .. } => Some(*level),
            _ => None,
        })
        .min()
        .unwrap_or(1);
    blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading { level, text } if *level == top => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::docgen::markdown;

    #[test]
    fn detects_chapter_headings() {
        assert!(is_chapter_heading("第 1 章 绪论"));
        assert!(is_chapter_heading("第一章 绪论"));
        assert!(is_chapter_heading("Chapter 2"));
        assert!(!is_chapter_heading("1 引言"));
    }

    #[test]
    fn collects_chapter_list_from_blocks() {
        let bs = markdown::parse("# 第 1 章 绪论\n\n正文\n\n# 第 2 章 方法\n\n正文");
        assert_eq!(chapters(&bs), vec!["第 1 章 绪论", "第 2 章 方法"]);
        assert!(SPEC.chapter_list);
        assert!(SPEC.chapter_first_page);
    }
}
