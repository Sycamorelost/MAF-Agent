use super::*;
use std::io::Read;

fn request(body: &str) -> DocRequest {
    DocRequest {
        title: "实验三 排课 CSP 求解".into(),
        body: body.into(),
        ..DocRequest::default()
    }
}

fn part_xml(bytes: &[u8], name: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
    let mut xml = String::new();
    zip.by_name(name).unwrap().read_to_string(&mut xml).unwrap();
    xml
}

fn document_xml(bytes: &[u8]) -> String {
    part_xml(bytes, "word/document.xml")
}

fn styles_xml(bytes: &[u8]) -> String {
    part_xml(bytes, "word/styles.xml")
}

/// 提取全部 `<w:t>` 文本并拼接,避免 run 切分影响断言。
fn plain_text(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<w:t") {
        let after = &rest[start..];
        let Some(gt) = after.find('>') else { break };
        let body = &after[gt + 1..];
        let Some(end) = body.find("</w:t>") else {
            break;
        };
        out.push_str(&body[..end]);
        rest = &body[end + 6..];
    }
    out
}

#[test]
fn renders_zip_with_word_styles() {
    let req = request(
        "## 一、实验目的\n\n理解 CSP 约束传播。\n\n表 1 各轮剪枝结果\n| 轮次 | 节点数 |\n|:---|---:|\n| 1 | 120 |",
    );
    let bytes = render(&req).unwrap();
    assert!(bytes.starts_with(b"PK"), "应产出合法 docx(zip)");
    let doc = document_xml(&bytes);
    // 段落引用真实 Heading 样式,样式定义带大纲级别 → 导航/目录可用
    assert!(doc.contains(r#"w:pStyle w:val="Heading2""#));
    assert!(styles_xml(&bytes).contains(r#"w:styleId="Heading1""#));
    assert!(styles_xml(&bytes).contains("<w:outlineLvl"));
    // 三线表:上下粗线
    assert!(doc.contains(r#"w:sz="12""#));
    // 表格列宽使用固定网格
    assert!(doc.contains("<w:tblGrid>"));
}

#[test]
fn general_profile_uses_grid_table() {
    let req = DocRequest {
        title: "项目周报".into(),
        body: "| 事项 | 状态 |\n|---|---|\n| 联调 | 完成 |".into(),
        ..DocRequest::default()
    };
    assert_eq!(req.effective_profile(), Profile::General);
    let xml = document_xml(&render(&req).unwrap());
    assert!(xml.contains(r#"w:color="BFBFBF""#));
}

#[test]
fn toc_only_when_headings_exist() {
    let mut req = request("## 一、实验目的\n\n正文");
    assert!(req.want_toc());
    assert!(document_xml(&render(&req).unwrap()).contains("TOC"));

    req.body = "正文无标题".into();
    assert!(!document_xml(&render(&req).unwrap()).contains("TOC"));
}

#[test]
fn cover_only_when_meta_present() {
    let mut req = request("## 一、实验目的\n\n正文");
    assert!(!req.has_cover());
    req.meta.author = "张三".into();
    req.meta.course = "人工智能".into();
    assert!(req.has_cover());
    let xml = document_xml(&render(&req).unwrap());
    assert!(xml.contains("张三"));
    assert!(xml.contains("人工智能"));
}

#[test]
fn meta_rows_skip_empty_fields() {
    let meta = DocMeta {
        author: " 李四 ".into(),
        ..DocMeta::default()
    };
    assert_eq!(meta.rows(), vec![("作者", "李四")]);
    assert!(!meta.is_empty());
}

#[test]
fn paper_profile_renders_abstract_and_references() {
    let req = DocRequest {
        title: "排课问题研究".into(),
        body: "摘要：本文研究排课问题。\n\n关键词：约束满足；回溯搜索\n\n## 一、引言\n\n正文。\n\n## 参考文献\n\n[1] 张三. 组合优化[M]. 北京: 科学出版社, 2020.".into(),
        profile: Some(Profile::Paper),
        meta: DocMeta {
            author: "张三".into(),
            course: "人工智能".into(),
            ..DocMeta::default()
        },
        ..DocRequest::default()
    };
    let bytes = render(&req).unwrap();
    let doc = document_xml(&bytes);
    assert!(doc.contains("AbstractText"));
    assert!(doc.contains("KeywordsText"));
    assert!(doc.contains("ReferenceText"));
    // 悬挂缩进由样式定义承载
    assert!(styles_xml(&bytes).contains(r#"w:styleId="ReferenceText""#));
    assert!(styles_xml(&bytes).contains("w:hanging"));
}

#[test]
fn official_template_applies_gb9704_margins_and_numbering() {
    let req = DocRequest {
        title: "关于推进项目的请示".into(),
        body: "## 总则\n\n正文。\n\n## 分则\n\n正文。".into(),
        template: Some("official".into()),
        ..DocRequest::default()
    };
    let bytes = render(&req).unwrap();
    let doc = document_xml(&bytes);
    // 上 37mm / 下 35mm / 左 28mm / 右 26mm
    assert!(doc.contains(r#"w:top="2098""#), "缺少公文本上边距");
    assert!(doc.contains(r#"w:bottom="1984""#), "缺少公文下边距");
    assert!(doc.contains(r#"w:left="1587""#), "缺少公文左边距");
    assert!(doc.contains(r#"w:right="1474""#), "缺少公文右边距");
    let text = plain_text(&doc);
    assert!(text.contains("一、总则"), "公文一级编号未生效: {text}");
    assert!(text.contains("二、分则"), "公文一级编号未递增: {text}");
    assert!(styles_xml(&bytes).contains("仿宋_GB2312"));
}

#[test]
fn paper_template_numbers_headings_hierarchically() {
    let req = DocRequest {
        title: "排课问题研究".into(),
        body: "## 引言\n\n### 研究背景\n\n正文。\n\n## 方法\n\n正文。".into(),
        template: Some("paper".into()),
        ..DocRequest::default()
    };
    let doc = document_xml(&render(&req).unwrap());
    assert!(doc.contains("1 引言"), "一级标题未编号");
    assert!(doc.contains("1.1 研究背景"), "二级标题未编号");
    assert!(doc.contains("2 方法"), "一级标题编号未递增");
}

#[test]
fn manual_heading_numbers_are_not_duplicated() {
    let req = DocRequest {
        title: "实验一".into(),
        body: "## 一、实验目的\n\n正文。".into(),
        ..DocRequest::default()
    };
    let doc = document_xml(&render(&req).unwrap());
    assert!(doc.contains("一、实验目的"));
    assert!(!doc.contains("一、一、实验目的"));
}

#[test]
fn explicit_profile_maps_to_template_and_explicit_template_wins() {
    let p = DocRequest {
        title: "任意标题".into(),
        body: "正文".into(),
        profile: Some(Profile::Paper),
        ..DocRequest::default()
    };
    assert_eq!(p.effective_template().id, "paper");
    let t = DocRequest {
        template: Some("minutes".into()),
        ..p.clone()
    };
    assert_eq!(t.effective_template().id, "minutes");
}

#[test]
fn auto_detects_notice_and_official() {
    let notice = DocRequest {
        title: "关于召开例会的通知".into(),
        body: "各部门:定于周五召开例会。".into(),
        ..DocRequest::default()
    };
    assert_eq!(notice.effective_template().id, "notice");
    let official = DocRequest {
        title: "关于××的请示".into(),
        body: "××〔2026〕3 号".into(),
        ..DocRequest::default()
    };
    assert_eq!(official.effective_template().id, "official");
}

#[test]
fn latex_is_written_as_omml() {
    let req = DocRequest {
        title: "公式测试".into(),
        body: "由 $E=mc^2$ 可知。\n\n$$\n\\frac{a+b}{2} \\le \\sqrt{x^2+y^2}\n$$".into(),
        ..DocRequest::default()
    };
    let bytes = render(&req).unwrap();
    let doc = document_xml(&bytes);
    assert!(doc.contains("xmlns:m="), "缺少 math 命名空间");
    assert!(doc.contains("<m:oMath>"), "行内公式未注入");
    assert!(doc.contains("<m:oMathPara>"), "独立公式未注入");
    assert!(doc.contains("<m:f>"), "分式未转换");
    assert!(doc.contains("<m:rad>"), "根式未转换");
    assert!(!doc.contains("@@MATH:"), "占位符未被替换");
}
