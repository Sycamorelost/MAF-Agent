//! 学位论文模板:只声明数据(模板元信息 + 装配参数),渲染逻辑一律在 writer。

use super::super::profile::Profile;
use super::super::style::{DocStyle, HeadingNumbering};
use super::{Template, TemplateSpec};

pub(super) const TEMPLATE: Template = Template {
    id: "thesis",
    name: "学位论文",
    profile: Profile::Paper,
    numbering: HeadingNumbering::Academic,
    keys: &["学位论文", "毕业论文", "原创性声明", "致谢", "指导教师"],
    skeleton: &[
        "封面",
        "原创性声明",
        "摘要",
        "Abstract",
        "目录",
        "引言",
        "相关工作",
        "方法",
        "实验",
        "结论",
        "参考文献",
        "致谢",
    ],
    hints: "学位论文:封面+原创性声明+中英文摘要+目录,章节 1/1.1/1.1.1 原生多级编号,前置罗马页码、正文阿拉伯页码。",
};

pub(super) const SPEC: TemplateSpec = TemplateSpec {
    cover_fields: &["学校", "学院", "专业", "学号", "作者", "指导教师", "日期"],
    declaration_page: true,
    native_numbering: true,
    sectioned: true,
    chapter_list: false,
    chapter_first_page: true,
    endnotes: false,
};

/// 版式覆盖:标题更大、固定行距 20pt,适配学位论文送审版。
pub(super) fn apply(style: &mut DocStyle) {
    style.title_size = 48;
    style.fixed_line = Some(400);
    style.caption_size = 21;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declares_thesis_only_assembly_data() {
        assert_eq!(TEMPLATE.id, "thesis");
        assert!(SPEC.declaration_page);
        assert!(SPEC.native_numbering);
        assert!(SPEC.cover_fields.contains(&"指导教师"));
    }

    #[test]
    fn differs_from_paper_in_cover_and_numbering() {
        let paper = super::super::lookup("paper").unwrap();
        assert_ne!(TEMPLATE.skeleton.len(), paper.skeleton.len());
        assert_eq!(TEMPLATE.numbering, HeadingNumbering::Academic);
        assert!(!paper.skeleton.contains(&"原创性声明"));
    }
}
