//! 集成测试:3 章 / 2 图 / 2 表 / 2 公式 / 交叉引用,以及损坏包校验。

use std::io::{Cursor, Read, Write};

use super::*;

const BODY: &str = r#"## 引言 {#sec-intro}

见 [@fig-arch]、[@tbl-data]、[@eq-euler] 与 [@sec-intro]。

图 1 系统架构 {#fig-arch}
![架构](missing.png)

表 1 实验数据 {#tbl-data}
| 项目 | 数值 |
|---|---|
| 节点 | 120 |

$$
E=mc^2 {#eq-euler}
$$

## 方法 {#sec-method}

图 2 流程图 {#fig-flow}
![流程](missing.png)

表 2 参数 {#tbl-param}
| 参数 | 取值 |
|---|---|
| 学习率 | 0.01 |

$$
\frac{a}{b} {#eq-frac}
$$

## 结论 {#sec-concl}

参见 [@sec-method] 与 [@fig-flow]。
"#;

fn request(template: &str) -> DocRequest {
    DocRequest {
        title: "排课问题研究".into(),
        body: BODY.into(),
        template: Some(template.into()),
        toc: Some(false),
        ..DocRequest::default()
    }
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
    let mut s = String::new();
    zip.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

/// 拼接全部 `<w:t>` 文本。
fn plain_text(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<w:t>").or_else(|| rest.find("<w:t ")) {
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
fn renders_three_chapters_with_cross_references() {
    let bytes = render(&request("thesis")).unwrap();
    let doc = part(&bytes, "word/document.xml");
    let text = plain_text(&doc);

    // 引用全部解析为可读文本,不再残留标记
    for expect in [
        "图 1",
        "图 2",
        "表 1",
        "表 2",
        "式 (1)",
        "式 (2)",
        "第 1 章",
        "第 2 章",
    ] {
        assert!(text.contains(expect), "缺少引用渲染 {expect}: {text}");
    }
    assert!(!text.contains("[@"), "存在未解析引用: {text}");
    assert!(!text.contains("{#"), "存在未剥离的 key 标记: {text}");

    // 编号连续:题注按出现顺序
    assert!(text.contains("图 1 系统架构"));
    assert!(text.contains("图 2 流程图"));
    assert!(text.contains("表 1 实验数据"));
    assert!(text.contains("表 2 参数"));

    // 书签 start/end 成对
    for name in [
        "_Ref_fig_arch",
        "_Ref_tbl_data",
        "_Ref_eq_euler",
        "_Ref_sec_intro",
    ] {
        assert!(
            doc.contains(&format!("w:name=\"{name}\"")),
            "缺少书签 {name}"
        );
    }
    assert_eq!(
        doc.matches("<w:bookmarkStart").count(),
        doc.matches("<w:bookmarkEnd").count()
    );

    // thesis 启用原生编号与分节
    assert!(doc.contains("<w:numPr>"), "标题未使用原生编号");
    assert!(doc.contains("<w:sectPr"), "缺少分节属性");
    assert!(part(&bytes, "word/numbering.xml").contains("w:abstractNumId=\"100\""));

    // 包级校验通过
    assert!(
        validation::validate_package(&bytes).is_empty(),
        "{:?}",
        validation::validate_package(&bytes)
    );
}

#[test]
fn book_chapters_are_detected_and_numbering_skipped() {
    let mut req = request("book");
    req.body = "# 第 1 章 绪论\n\n正文。\n\n# 第 2 章 方法\n\n正文。".into();
    let bytes = render(&req).unwrap();
    let text = plain_text(&part(&bytes, "word/document.xml"));
    assert!(text.contains("第 1 章 绪论"));
    assert!(!text.contains("1.1 第 1 章"), "手写章号被重复编号: {text}");
}

#[test]
fn validator_reports_missing_relationship_and_bookmark() {
    let bytes = render(&request("thesis")).unwrap();

    // 删掉 numbering.xml:document.xml.rels 的目标缺失 → REL-001
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes.clone())).unwrap();
    let mut parts = std::collections::HashMap::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        if f.is_dir() {
            continue;
        }
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).unwrap();
        parts.insert(f.name().unwrap_or_default().to_string(), buf);
    }
    drop(zip);
    parts.remove("word/numbering.xml");
    let damaged = pack(&parts);
    assert!(
        validation::validate_package(&damaged)
            .iter()
            .any(|v| v.rule == "REL-001" && v.part.ends_with("document.xml.rels")),
        "{:?}",
        validation::validate_package(&damaged)
    );

    // 破坏 bookmarkEnd:BM-001
    let doc = String::from_utf8_lossy(parts.get("word/document.xml").unwrap()).to_string();
    let broken = doc.replacen("<w:bookmarkEnd", "<w:bookmarkEndX", 1);
    let mut parts2 = parts.clone();
    parts2.insert("word/document.xml".into(), broken.into_bytes());
    assert!(
        validation::validate_package(&pack(&parts2))
            .iter()
            .any(|v| v.rule == "BM-001")
    );
}

fn pack(parts: &std::collections::HashMap<String, Vec<u8>>) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    let mut zw = zip::ZipWriter::new(&mut out);
    let opts = zip::write::SimpleFileOptions::default();
    for (name, data) in parts {
        zw.start_file(name, opts).unwrap();
        zw.write_all(data).unwrap();
    }
    zw.finish().unwrap();
    out.into_inner()
}
