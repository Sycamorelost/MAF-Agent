//! 分节装配:封面 / 前置(摘要目录) / 正文 / 附录的节参数。
//!
//! 只生成分节参数与 `Section`,不负责块渲染。能力边界:
//! - docx-rs 0.4.22 的 `PageNumType` 只有 `start`/`chapStyle`,**不支持页码格式**,
//!   因此前置罗马页码降级为「从 1 重新编号」并记录诊断;
//! - `PageMargin` 无镜像边距字段,镜像边距同样降级并记录诊断;
//! - 章首页无页眉通过 `Section::first_header(空)` 实现(`w:titlePg`);
//! - 奇偶页页眉通过 `Docx::even_header` 实现(同时开启 evenAndOddHeaders)。

use docx_rs::{Docx, Footer, Header, PageMargin, PageNumType, PageSize, Paragraph, Section};

use super::super::refs::{Diagnostic, Severity};
use super::super::style::DocStyle;
use super::front_matter::{footer_para, header_para};

/// 文档分区。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Part {
    Cover,
    Front,
    Body,
    Appendix,
}

impl Part {
    fn name(self) -> &'static str {
        match self {
            Part::Cover => "封面",
            Part::Front => "前置",
            Part::Body => "正文",
            Part::Appendix => "附录",
        }
    }
}

/// 一个分区的装配参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SectionSpec {
    pub part: Part,
    /// 期望罗马页码(docx-rs 不支持,记录诊断)
    pub roman: bool,
    /// 页码起始值
    pub start: u32,
    /// 首页不显示页眉(章/封面首页规则)
    pub blank_first_header: bool,
    /// 期望镜像边距(docx-rs 不支持,记录诊断)
    pub mirrored: bool,
}

/// 由文档结构推导分节计划。
pub(super) fn plan(has_cover: bool, toc: bool, has_appendix: bool) -> Vec<SectionSpec> {
    let mut out = Vec::new();
    if has_cover {
        out.push(SectionSpec {
            part: Part::Cover,
            roman: false,
            start: 1,
            blank_first_header: true,
            mirrored: false,
        });
    }
    if toc {
        out.push(SectionSpec {
            part: Part::Front,
            roman: true,
            start: 1,
            blank_first_header: false,
            mirrored: true,
        });
    }
    out.push(SectionSpec {
        part: Part::Body,
        roman: false,
        start: 1,
        blank_first_header: true,
        mirrored: true,
    });
    if has_appendix {
        out.push(SectionSpec {
            part: Part::Appendix,
            roman: false,
            start: 1,
            blank_first_header: false,
            mirrored: true,
        });
    }
    out
}

/// 装配能力不足的降级项,汇总为诊断。
pub(super) fn diagnostics(plan: &[SectionSpec]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (i, spec) in plan.iter().enumerate() {
        if spec.roman {
            out.push(Diagnostic {
                level: Severity::Warning,
                code: "DOCX-PG-NUM-FMT",
                message: format!(
                    "{}节需要罗马页码,docx-rs 的 PageNumType 不支持页码格式,已降级为从 {} 重新编号",
                    spec.part.name(),
                    spec.start
                ),
                line: i + 1,
            });
        }
        if spec.mirrored {
            out.push(Diagnostic {
                level: Severity::Info,
                code: "DOCX-MIRROR-MARGIN",
                message: format!(
                    "{}节需要镜像边距,docx-rs 的 PageMargin 无对应字段,已使用左右对称边距",
                    spec.part.name()
                ),
                line: i + 1,
            });
        }
    }
    out
}

/// 页面边距(docx-rs `PageMargin`)。
pub(super) fn margin(style: &DocStyle) -> PageMargin {
    PageMargin {
        top: style.margin_top,
        left: style.margin_left,
        bottom: style.margin_bottom,
        right: style.margin_right,
        header: 851,
        footer: 992,
        gutter: 0,
    }
}

/// 生成一个分节的 `Section`(内容由调用方在此之前写入)。
pub(super) fn section(spec: &SectionSpec, style: &DocStyle, title: &str) -> Section {
    let mut s = Section::new()
        .page_size(PageSize::new().size(style.page_w, style.page_h))
        .page_margin(margin(style))
        .page_num_type(PageNumType::new().start(spec.start));
    match spec.part {
        Part::Cover | Part::Front => {
            // 无页眉页脚:用空段落占位,避免 docx-rs 生成「有 rel 无 part」的空白部件
            s = s
                .header(Header::new().add_paragraph(Paragraph::new()))
                .footer(Footer::new().add_paragraph(Paragraph::new()));
        }
        Part::Body | Part::Appendix => {
            if !title.trim().is_empty() {
                s = s.header(Header::new().add_paragraph(header_para(style, title)));
            }
            s = s.footer(Footer::new().add_paragraph(footer_para()));
            if spec.blank_first_header {
                s = s.first_header(Header::new().add_paragraph(Paragraph::new()));
            }
        }
    }
    s
}

/// 把最终(正文/附录)节的属性写回文档级 sectPr。
pub(super) fn apply_final(mut docx: Docx, style: &DocStyle, title: &str, start: u32) -> Docx {
    docx = docx
        .page_size(style.page_w, style.page_h)
        .page_margin(margin(style))
        .page_num_type(PageNumType::new().start(start));
    if !title.trim().is_empty() {
        docx = docx.header(Header::new().add_paragraph(header_para(style, title)));
    }
    docx = docx
        .footer(Footer::new().add_paragraph(footer_para()))
        .first_header(Header::new().add_paragraph(Paragraph::new()));
    docx
}

/// 是否为附录标题(用于切分正文/附录)。
pub(super) fn is_appendix_heading(text: &str) -> bool {
    let t = text.trim_start_matches(['#', ' ', '*', '_']).trim();
    t == "附录" || t.starts_with("附录") || t.eq_ignore_ascii_case("appendix")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_contains_front_body_and_appendix() {
        let p = plan(true, true, true);
        let parts: Vec<Part> = p.iter().map(|s| s.part).collect();
        assert_eq!(
            parts,
            vec![Part::Cover, Part::Front, Part::Body, Part::Appendix]
        );
        assert_eq!(p[1].start, 1);
        assert!(p[2].blank_first_header);
    }

    #[test]
    fn diagnostics_report_unsupported_roman_and_mirror() {
        let d = diagnostics(&plan(true, true, false));
        assert!(d.iter().any(|x| x.code == "DOCX-PG-NUM-FMT"));
        assert!(d.iter().any(|x| x.code == "DOCX-MIRROR-MARGIN"));
    }

    #[test]
    fn section_carries_page_setup() {
        let style = DocStyle::for_profile(super::super::super::profile::Profile::Paper);
        let s = section(&plan(false, true, false)[0], &style, "论文");
        let dbg = format!("{s:?}");
        assert!(dbg.contains("page_size"), "{dbg}");
        assert!(dbg.contains("page_num_type"), "{dbg}");
    }
}
