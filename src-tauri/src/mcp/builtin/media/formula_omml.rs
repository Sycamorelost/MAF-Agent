//! EXPERIMENTAL：单页 DrawingML 数学 OOXML 研究原型。
//!
//! 仅复用 docgen 的 LaTeX → OMML 转换并生成内存 ZIP；不注册媒体工具，
//! 不接入生产导出，也不保证第三方演示软件的兼容性或编辑行为。

use std::io::{Cursor, Write};

use crate::fs::docgen::math;

/// 实验格式标记，供隔离测试与后续研究识别。
pub(super) const EXPERIMENTAL_MARKER: &str = "experimental:ppt-omml-drawingml-v1";

const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_A14: &str = "http://schemas.microsoft.com/office/drawing/2010/main";
const NS_M: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
const NS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const NS_TYPES: &str = "http://schemas.openxmlformats.org/package/2006/content-types";

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
  <Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>"#;

const PRESENTATION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst>
  <p:sldSz cx="12192000" cy="6858000" type="screen16x9"/>
  <p:notesSz cx="6858000" cy="9144000"/>
</p:presentation>"#;

const PRESENTATION_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
</Relationships>"#;

/// 构造仅供实验的单页数学 OOXML ZIP，不写磁盘、不接受生产工具参数。
pub(super) fn build_experimental_zip(latex: &str) -> Result<Vec<u8>, String> {
    let latex = latex.trim();
    if latex.is_empty() {
        return Err("实验公式不能为空".to_string());
    }

    // 复用现有 OMML 生成器；display=true 对应 a14:m 内的独立公式段落。
    let slide = slide_xml(&math::omml(latex, true));
    let parts = [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("ppt/presentation.xml", PRESENTATION),
        ("ppt/_rels/presentation.xml.rels", PRESENTATION_RELS),
        ("ppt/slides/slide1.xml", slide.as_str()),
    ];
    let mut out = Cursor::new(Vec::new());
    {
        let mut archive = zip::ZipWriter::new(&mut out);
        let options = zip::write::SimpleFileOptions::default();
        for (name, xml) in parts {
            archive
                .start_file(name, options)
                .map_err(|e| format!("创建实验部件 {name} 失败: {e}"))?;
            archive
                .write_all(xml.as_bytes())
                .map_err(|e| format!("写入实验部件 {name} 失败: {e}"))?;
        }
        archive
            .finish()
            .map_err(|e| format!("完成实验 ZIP 失败: {e}"))?;
    }
    Ok(out.into_inner())
}

/// 在 DrawingML 文本段落中用 a14:m 承载 docgen 生成的 OMML。
fn slide_xml(omml: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!-- {EXPERIMENTAL_MARKER}：仅供 OOXML 结构研究 -->
<p:sld xmlns:a="{NS_A}" xmlns:a14="{NS_A14}" xmlns:m="{NS_M}" xmlns:p="{NS_P}" xmlns:r="{NS_R}">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
      <p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
      <p:sp>
        <p:nvSpPr><p:cNvPr id="2" name="Experimental OMML"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr>
        <p:spPr><a:xfrm><a:off x="914400" y="914400"/><a:ext cx="10363200" cy="5029200"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr>
        <p:txBody><a:bodyPr wrap="square"/><a:lstStyle/><a:p><a14:m>{omml}</a14:m><a:endParaRPr lang="zh-CN"/></a:p></p:txBody>
      </p:sp>
    </p:spTree>
  </p:cSld>
  <p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sld>"#
    )
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use roxmltree::Document;
    use zip::ZipArchive;

    use super::super::slides_validate::validate_pptx;
    use super::*;

    // 同时覆盖上下限、分式、根式、上下标、希腊字母与定界符。
    const COMPLEX_FORMULA: &str =
        r"\sum_{i=1}^{n}\frac{\sqrt{\alpha_i^2+\beta_i^2}}{i!}+\left(\frac{x}{y}\right)^{2}";

    fn part(bytes: &[u8], name: &str) -> String {
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut file = archive.by_name(name).unwrap();
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        text
    }

    fn assert_relationship(xml: &str, id: &str, kind: &str, target: &str) {
        let doc = Document::parse(xml).unwrap();
        assert_eq!(doc.root_element().tag_name().namespace(), Some(NS_REL));
        let rel = doc
            .descendants()
            .find(|n| n.tag_name().name() == "Relationship" && n.attribute("Id") == Some(id))
            .unwrap();
        assert_eq!(rel.attribute("Type"), Some(kind));
        assert_eq!(rel.attribute("Target"), Some(target));
    }

    #[test]
    fn complex_formula_fixture_has_drawingml_math() {
        let bytes = build_experimental_zip(COMPLEX_FORMULA).unwrap();
        let slide = part(&bytes, "ppt/slides/slide1.xml");
        assert!(slide.contains(EXPERIMENTAL_MARKER));
        let doc = Document::parse(&slide).unwrap();
        let root = doc.root_element();
        for (prefix, uri) in [
            ("a", NS_A),
            ("a14", NS_A14),
            ("m", NS_M),
            ("p", NS_P),
            ("r", NS_R),
        ] {
            assert!(
                root.namespaces()
                    .any(|ns| ns.name() == Some(prefix) && ns.uri() == uri)
            );
        }
        assert!(doc.descendants().any(|n| n.has_tag_name((NS_A14, "m"))));
        assert!(
            doc.descendants()
                .any(|n| n.has_tag_name((NS_M, "oMathPara")))
        );
        for element in ["nary", "f", "rad", "d", "sSubSup", "sSup"] {
            assert!(
                doc.descendants().any(|n| n.has_tag_name((NS_M, element))),
                "复杂公式缺少 m:{element}"
            );
        }
        assert!(doc.descendants().any(|n| n.text() == Some("α")));
    }

    #[test]
    fn package_content_types_and_relationships_resolve() {
        let bytes = build_experimental_zip(COMPLEX_FORMULA).unwrap();
        let types = part(&bytes, "[Content_Types].xml");
        let doc = Document::parse(&types).unwrap();
        assert_eq!(doc.root_element().tag_name().namespace(), Some(NS_TYPES));
        for (part_name, mime) in [
            (
                "/ppt/presentation.xml",
                "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
            ),
            (
                "/ppt/slides/slide1.xml",
                "application/vnd.openxmlformats-officedocument.presentationml.slide+xml",
            ),
        ] {
            let item = doc
                .descendants()
                .find(|n| {
                    n.tag_name().name() == "Override" && n.attribute("PartName") == Some(part_name)
                })
                .unwrap();
            assert_eq!(item.attribute("ContentType"), Some(mime));
        }
        for extension in ["rels", "xml"] {
            assert!(doc.descendants().any(|n| {
                n.tag_name().name() == "Default" && n.attribute("Extension") == Some(extension)
            }));
        }

        assert_relationship(
            &part(&bytes, "_rels/.rels"),
            "rId1",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
            "ppt/presentation.xml",
        );
        assert_relationship(
            &part(&bytes, "ppt/_rels/presentation.xml.rels"),
            "rId1",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide",
            "slides/slide1.xml",
        );
        let presentation = part(&bytes, "ppt/presentation.xml");
        let doc = Document::parse(&presentation).unwrap();
        let slide_id = doc
            .descendants()
            .find(|n| n.tag_name().name() == "sldId")
            .unwrap();
        assert_eq!(slide_id.attribute((NS_R, "id")), Some("rId1"));

        // 复用现有校验器检查 XML 良构、页数以及全部内部关系目标。
        validate_pptx(&bytes).unwrap();
    }

    #[test]
    fn rejects_empty_formula() {
        assert!(build_experimental_zip(" \n\t ").is_err());
    }
}
