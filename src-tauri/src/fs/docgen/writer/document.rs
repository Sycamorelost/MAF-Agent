//! 文档装配:封面、目录、分节与块渲染调度。
//!
//! 只做装配;块级渲染在 `body`、分节参数在 `section`、原生编号在 `numbering`。
//! 既有 8 个模板沿用单节装配与文本前缀编号,行为保持兼容;thesis/book 启用
//! 分节 + 原生编号(由 `TemplateSpec` 声明)。

use std::path::Path;

use docx_rs::*;

use super::super::DocRequest;
use super::super::markdown::Block;
use super::super::refs;
use super::super::style::DocStyle;
use super::super::templates::{self, numbering};
use super::body::{self, Ctx};
use super::front_matter::{add_cover, even_header_para, footer_para, header_para};
use super::numbering::{self as native, NumberingIds};
use super::section;
use super::styles::styles_for;

pub fn render(req: &DocRequest, style: &DocStyle, base: &Path) -> Result<Vec<u8>, String> {
    let template = req.effective_template();
    let spec = templates::spec_of(template.id);
    let body_src = if spec.endnotes {
        super::super::notes_index::transform(&req.body)
    } else {
        req.body.clone()
    };
    let blocks = super::super::markdown::parse(&body_src);

    // 第一遍:标签编号;第二遍:交叉引用替换
    let scan = refs::scan(&blocks, style.numbering);
    for d in scan.diagnostics() {
        log::warn!("docgen 引用诊断 [{}] {}", d.code, d.message);
    }
    let (blocks, ref_diags) = refs::resolve_blocks(&blocks, &scan);
    for d in ref_diags {
        log::warn!("docgen 引用诊断 [{}] {}", d.code, d.message);
    }

    let cover = req.has_cover();
    let has_headings = blocks
        .iter()
        .any(|b| matches!(b, Block::Heading { level, .. } if *level <= 3));
    let toc = req.want_toc() && has_headings;
    let appendix_at = blocks.iter().position(
        |b| matches!(b, Block::Heading { text, .. } if section::is_appendix_heading(text)),
    );
    let plan = if spec.sectioned {
        section::plan(cover, toc, appendix_at.is_some())
    } else {
        Vec::new()
    };
    for d in section::diagnostics(&plan) {
        log::warn!("docgen 分节诊断 [{}] {}", d.code, d.message);
    }

    let mut docx = Docx::new()
        .page_size(style.page_w, style.page_h)
        .page_margin(super::section::margin(style))
        .styles(styles_for(style));
    if !spec.sectioned {
        // 单节模板:文档级页脚只设置一次;分节模板由 section::apply_final 统一设置
        docx = docx.footer(Footer::new().add_paragraph(footer_para()));
    }

    // 原生编号只在模板声明时注册(thesis/book)
    let (mut docx, ids) = if spec.native_numbering {
        native::register(docx, style.numbering)
    } else {
        (docx, NumberingIds::none())
    };

    if !spec.sectioned {
        // 既有模板:单节 + 首节页眉页脚(保持原行为)
        if !req.title.trim().is_empty() {
            docx = docx.header(Header::new().add_paragraph(header_para(style, &req.header_line())));
        }
        if cover {
            docx = docx
                .first_header(Header::new().add_paragraph(Paragraph::new()))
                .first_footer(Footer::new().add_paragraph(Paragraph::new()))
                .title_pg();
        }
    }

    if cover {
        docx = add_cover(docx, req, style);
        if let Some(s) = plan.iter().find(|p| p.part == section::Part::Cover) {
            docx = docx.add_section(section::section(s, style, &req.title));
        }
    }
    if toc {
        docx = docx.add_paragraph(
            Paragraph::new()
                .style("TocHeading")
                .align(AlignmentType::Center)
                .page_break_before(cover && !spec.sectioned)
                .add_run(Run::new().add_text("目　录")),
        );
        docx = docx.add_table_of_contents(TableOfContents::new().heading_styles_range(1, 3));
        if let Some(s) = plan.iter().find(|p| p.part == section::Part::Front) {
            docx = docx.add_section(section::section(s, style, &req.title));
        }
    }

    let prefixes = numbering::heading_prefixes(&blocks, style.numbering);
    let ctx = Ctx {
        style,
        base,
        ids,
        native: spec.native_numbering,
    };
    let mut need_break = !spec.sectioned && (cover || toc);
    for (idx, block) in blocks.iter().enumerate() {
        // 附录边界:关闭正文节,剩余内容进入附录节
        if appendix_at == Some(idx)
            && let Some(spec) = plan.iter().find(|p| p.part == section::Part::Body)
        {
            docx = docx.add_section(section::section(spec, style, &req.title));
            need_break = false;
        }
        let brk = std::mem::take(&mut need_break);
        docx = body::add_block(
            docx,
            block,
            &ctx,
            brk,
            prefixes[idx].as_deref(),
            &scan.labels_at(idx),
        );
    }

    if spec.sectioned {
        let start = plan.last().map(|p| p.start).unwrap_or(1);
        docx = section::apply_final(docx, style, &req.header_line(), start);
        if !req.title.trim().is_empty() {
            docx =
                docx.even_header(Header::new().add_paragraph(even_header_para(style, &req.header_line())));
        }
    }

    let mut cursor = std::io::Cursor::new(Vec::new());
    docx.build()
        .pack(&mut cursor)
        .map_err(|e| format!("生成 docx 失败: {e}"))?;
    // 把正文里的公式占位符替换为 OMML(docx-rs 本身不支持公式)
    let bytes = super::super::math_docx::inject_docx_math(cursor.into_inner())?;
    // 显式 formula_mode=svg 时,再把 SVG 占位符替换为矢量图片 + PNG 回退
    let bytes = if style.formula_mode == super::super::FormulaMode::Svg {
        super::super::svg_docx::inject_svg_formulas(bytes)?
    } else {
        bytes
    };
    // 尾注与索引:有标记才落地
    super::super::notes_index::inject(bytes)
}
