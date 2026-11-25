//! 生成后的 .pptx 包校验:解包 + 结构自检,失败只报告不删产物。
//!
//! 校验项:`[Content_Types].xml` 存在、所有 XML 良构、presentation.xml 的
//! slide 引用数与文件数一致、各 relationship 目标存在。

use std::collections::HashMap;
use std::io::{Cursor, Read};

use roxmltree::Document;
use zip::ZipArchive;

/// 校验 .pptx 字节;成功返回 Ok,失败返回可定位错误列表。
pub(super) fn validate_pptx(bytes: &[u8]) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let mut archive = match ZipArchive::new(Cursor::new(bytes)) {
        Ok(a) => a,
        Err(e) => return Err(vec![format!("无法解包 .pptx(可能是损坏的 zip):{e}")]),
    };
    let names = collect_names(&mut archive, &mut errors);
    for required in ["[Content_Types].xml", "ppt/presentation.xml"] {
        if !names.iter().any(|n| n == required) {
            errors.push(format!("缺少必需部件:{required}"));
        }
    }

    // 逐个 XML 部件做良构检查,并缓存内容供后续结构校验
    let mut docs: HashMap<String, String> = HashMap::new();
    for name in names.iter() {
        if !(name.ends_with(".xml") || name.ends_with(".rels")) {
            continue;
        }
        let Some(text) = read_entry(&mut archive, name, &mut errors) else {
            continue;
        };
        if let Err(e) = Document::parse(&text) {
            let pos = e.pos();
            errors.push(format!(
                "{name} 第 {} 行第 {} 列 XML 不良构:{e}",
                pos.row, pos.col
            ));
        }
        docs.insert(name.clone(), text);
    }

    let slide_files: Vec<&String> = names.iter().filter(|n| is_slide_part(n)).collect();
    check_presentation(&docs, &slide_files, &mut errors);
    check_relationships(&docs, &names, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn collect_names<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    errors: &mut Vec<String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for item in archive.file_names() {
        match item {
            Ok(n) => out.push(n.to_string()),
            Err(e) => errors.push(format!("zip 目录损坏:{e}")),
        }
    }
    out
}

fn read_entry<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    errors: &mut Vec<String>,
) -> Option<String> {
    let mut f = match archive.by_name(name) {
        Ok(f) => f,
        Err(e) => {
            errors.push(format!("无法读取 {name}:{e}"));
            return None;
        }
    };
    let mut buf = String::new();
    if let Err(e) = f.read_to_string(&mut buf) {
        errors.push(format!("{name} 不是有效 UTF-8 文本:{e}"));
        return None;
    }
    Some(buf)
}

fn is_slide_part(name: &str) -> bool {
    name.starts_with("ppt/slides/slide") && name.ends_with(".xml") && !name.contains("/_rels/")
}

/// presentation.xml 的 sldId 数量必须与 slide 文件数一致,rId 必须可解析。
fn check_presentation(
    docs: &HashMap<String, String>,
    slide_files: &[&String],
    errors: &mut Vec<String>,
) {
    let Some(xml) = docs.get("ppt/presentation.xml") else {
        return;
    };
    let doc = match Document::parse(xml) {
        Ok(d) => d,
        Err(_) => return, // 良构错误已记录
    };
    let ids: Vec<String> = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "sldId")
        .map(|n| {
            n.attributes()
                .find(|a| a.name() == "id" && a.namespace().is_some())
                .map(|a| a.value().to_string())
                .unwrap_or_default()
        })
        .collect();
    if ids.len() != slide_files.len() {
        errors.push(format!(
            "ppt/presentation.xml 引用 {} 张幻灯片,但 ppt/slides 下有 {} 个文件",
            ids.len(),
            slide_files.len()
        ));
    }
    let rel_ids = relationship_ids(docs.get("ppt/_rels/presentation.xml.rels"));
    for (i, rid) in ids.iter().enumerate() {
        if !rid.is_empty() && !rel_ids.contains(rid) {
            errors.push(format!(
                "ppt/presentation.xml 第 {} 个 sldId 的 r:id={rid} 在 relationships 中不存在",
                i + 1
            ));
        }
    }
}

fn relationship_ids(rels: Option<&String>) -> Vec<String> {
    let Some(xml) = rels else {
        return Vec::new();
    };
    let Ok(doc) = Document::parse(xml) else {
        return Vec::new();
    };
    doc.descendants()
        .filter(|n| n.tag_name().name() == "Relationship")
        .filter_map(|n| n.attribute("Id").map(str::to_string))
        .collect()
}

/// 所有 .rels 中非外链的 Target 必须能在包内找到。
fn check_relationships(docs: &HashMap<String, String>, names: &[String], errors: &mut Vec<String>) {
    for (name, xml) in docs.iter() {
        if !name.ends_with(".rels") {
            continue;
        }
        let Ok(doc) = Document::parse(xml) else {
            continue;
        };
        let base = rels_base_dir(name);
        for rel in doc
            .descendants()
            .filter(|n| n.tag_name().name() == "Relationship")
        {
            if rel.attribute("TargetMode") == Some("External") {
                continue;
            }
            let Some(target) = rel.attribute("Target") else {
                errors.push(format!("{name} 存在缺少 Target 的 Relationship"));
                continue;
            };
            if target.starts_with('#') || target.is_empty() {
                continue;
            }
            let resolved = resolve_target(&base, target);
            if !names.iter().any(|n| n == &resolved) {
                errors.push(format!(
                    "{name} 的关系目标不存在:{target}(解析为 {resolved})"
                ));
            }
        }
    }
}

fn rels_base_dir(rels_name: &str) -> String {
    // ppt/_rels/presentation.xml.rels -> ppt;ppt/slides/_rels/slide1.xml.rels -> ppt/slides
    match rels_name.rfind("/_rels/") {
        Some(pos) => rels_name[..pos].to_string(),
        None => String::new(),
    }
}

fn resolve_target(base: &str, target: &str) -> String {
    if let Some(stripped) = target.strip_prefix('/') {
        return stripped.to_string();
    }
    let mut parts: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_corrupted_zip() {
        let errs = validate_pptx(b"PK\x03\x04not-a-real-zip").unwrap_err();
        assert!(!errs.is_empty());
        assert!(errs[0].contains("zip") || errs[0].contains("解包"));
    }

    #[test]
    fn resolves_relationship_targets() {
        assert_eq!(
            resolve_target("ppt", "slides/slide1.xml"),
            "ppt/slides/slide1.xml"
        );
        assert_eq!(
            resolve_target("ppt/slides", "../media/x.png"),
            "ppt/media/x.png"
        );
        assert_eq!(
            resolve_target("ppt", "/ppt/slideMasters/a.xml"),
            "ppt/slideMasters/a.xml"
        );
        assert_eq!(
            rels_base_dir("ppt/slides/_rels/slide1.xml.rels"),
            "ppt/slides"
        );
    }

    #[test]
    fn flags_missing_parts_in_empty_zip() {
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file("ppt/slides/slide1.xml", opts).unwrap();
            std::io::Write::write_all(&mut w, b"<p:sld xmlns:p=\"x\"/>").unwrap();
            w.finish().unwrap();
        }
        let errs = validate_pptx(&buf).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("[Content_Types].xml")));
        assert!(errs.iter().any(|e| e.contains("presentation.xml")));
    }
}
