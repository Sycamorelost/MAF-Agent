//! OPC 校验规则:
//! - `CT-001` `[Content_Types].xml` 必须存在;
//! - `REL-001` 每个关系目标 part 必须存在(外部关系除外);
//! - `NUM-001` `numbering.xml` 的 numId/abstractNumId 双向引用完整;
//! - `BM-001` 书签 start/end 必须配对;
//! - `FN-001` 脚注引用必须有对应 `footnotes.xml` 条目。

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

use quick_xml::Reader;
use quick_xml::events::Event;

/// 一条校验违规(part 名 + 规则 ID)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub part: String,
    pub rule: &'static str,
    pub message: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.rule, self.part, self.message)
    }
}

type Attrs = HashMap<String, String>;
type Tag = (String, Attrs);

/// 校验整个 docx 包,返回全部违规(空表示通过)。
pub fn validate_package(bytes: &[u8]) -> Vec<Violation> {
    let mut out = Vec::new();
    let parts = match unpack(bytes) {
        Ok(p) => p,
        Err(e) => {
            return vec![Violation {
                part: "package".into(),
                rule: "ZIP-001",
                message: e,
            }];
        }
    };

    if !parts.contains_key("[Content_Types].xml") {
        out.push(Violation {
            part: "package".into(),
            rule: "CT-001",
            message: "缺少 [Content_Types].xml".into(),
        });
    }

    check_well_formed(&parts, &mut out);
    check_relationships(&parts, &mut out);
    if let Some(xml) = parts.get("word/numbering.xml") {
        check_numbering(text(xml), &mut out);
    }
    if let Some(xml) = parts.get("word/document.xml") {
        check_bookmarks(text(xml), &mut out);
        check_footnotes(&parts, text(xml), &mut out);
    }
    out
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap_or("")
}

fn unpack(bytes: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("无法解析 zip: {e}"))?;
    let mut parts = HashMap::new();
    for i in 0..archive.len() {
        let mut f = archive
            .by_index(i)
            .map_err(|e| format!("读取条目失败: {e}"))?;
        if f.is_dir() {
            continue;
        }
        let name = f
            .name()
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_default();
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)
            .map_err(|e| format!("读取 {name} 失败: {e}"))?;
        parts.insert(name, buf);
    }
    Ok(parts)
}

/// 每个 XML/rels part 必须是单根合法文档;多个根节点会被 Word 判为损坏。
fn check_well_formed(parts: &HashMap<String, Vec<u8>>, out: &mut Vec<Violation>) {
    for (name, bytes) in parts {
        if !(name.ends_with(".xml") || name.ends_with(".rels")) {
            continue;
        }
        if let Err(e) = roxmltree::Document::parse(text(bytes)) {
            out.push(Violation {
                part: name.clone(),
                rule: "XML-001",
                message: format!("XML 结构不合法: {e}"),
            });
        }
    }
}

fn check_relationships(parts: &HashMap<String, Vec<u8>>, out: &mut Vec<Violation>) {
    for (name, bytes) in parts.iter().filter(|(n, _)| n.ends_with(".rels")) {
        let source_dir = rel_base(name);
        for (tag, attrs) in tags(text(bytes)) {
            if tag != "Relationship" {
                continue;
            }
            if attrs
                .get("TargetMode")
                .is_some_and(|m| m.eq_ignore_ascii_case("External"))
            {
                continue;
            }
            let Some(target) = attrs.get("Target") else {
                continue;
            };
            let target = target.trim_start_matches('/');
            let full = if target.starts_with('/') {
                target.to_string()
            } else if source_dir.is_empty() {
                normalize(target)
            } else {
                normalize(&format!("{source_dir}/{target}"))
            };
            if !parts.contains_key(&full) {
                out.push(Violation {
                    part: name.clone(),
                    rule: "REL-001",
                    message: format!("关系目标不存在: {full}"),
                });
            }
        }
    }
}

/// `word/_rels/document.xml.rels` → `word`;`_rels/.rels` → ""。
fn rel_base(name: &str) -> String {
    let mut path = name.to_string();
    if let Some(idx) = path.rfind("_rels/") {
        path.truncate(idx);
    }
    path.trim_end_matches('/').to_string()
}

fn normalize(path: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            s => stack.push(s),
        }
    }
    stack.join("/")
}

fn check_numbering(xml: &str, out: &mut Vec<Violation>) {
    let tags = tags(xml);
    let abstract_ids: HashSet<String> = tags
        .iter()
        .filter(|(t, _)| t == "abstractNum")
        .filter_map(|(_, a)| a.get("abstractNumId").cloned())
        .collect();
    let mut referenced: HashSet<String> = HashSet::new();
    let mut current_num: Option<String> = None;
    for (tag, attrs) in &tags {
        match tag.as_str() {
            "num" => current_num = attrs.get("numId").cloned(),
            "abstractNumId" => {
                if let (Some(num), Some(abs)) = (&current_num, attrs.get("val")) {
                    if !abstract_ids.contains(abs) {
                        out.push(Violation {
                            part: "word/numbering.xml".into(),
                            rule: "NUM-001",
                            message: format!("numId={num} 引用了不存在的 abstractNumId={abs}"),
                        });
                    }
                    referenced.insert(abs.clone());
                }
            }
            _ => {}
        }
    }
    for id in abstract_ids.difference(&referenced) {
        out.push(Violation {
            part: "word/numbering.xml".into(),
            rule: "NUM-001",
            message: format!("abstractNumId={id} 没有被任何 numId 引用"),
        });
    }
}

fn check_bookmarks(xml: &str, out: &mut Vec<Violation>) {
    let mut starts: HashSet<String> = HashSet::new();
    let mut ends: HashSet<String> = HashSet::new();
    for (tag, attrs) in tags(xml) {
        match tag.as_str() {
            "bookmarkStart" => {
                if let Some(id) = attrs.get("id") {
                    starts.insert(id.clone());
                }
            }
            "bookmarkEnd" => {
                if let Some(id) = attrs.get("id") {
                    ends.insert(id.clone());
                }
            }
            _ => {}
        }
    }
    for id in starts.difference(&ends) {
        out.push(Violation {
            part: "word/document.xml".into(),
            rule: "BM-001",
            message: format!("书签 {id} 缺少 bookmarkEnd"),
        });
    }
    for id in ends.difference(&starts) {
        out.push(Violation {
            part: "word/document.xml".into(),
            rule: "BM-001",
            message: format!("书签 {id} 缺少 bookmarkStart"),
        });
    }
}

fn check_footnotes(parts: &HashMap<String, Vec<u8>>, xml: &str, out: &mut Vec<Violation>) {
    let refs: Vec<String> = tags(xml)
        .into_iter()
        .filter(|(t, _)| t == "footnoteReference")
        .filter_map(|(_, a)| a.get("id").cloned())
        .collect();
    if refs.is_empty() {
        return;
    }
    let Some(foot) = parts.get("word/footnotes.xml") else {
        out.push(Violation {
            part: "word/document.xml".into(),
            rule: "FN-001",
            message: "存在脚注引用但缺少 word/footnotes.xml".into(),
        });
        return;
    };
    let defined: HashSet<String> = tags(text(foot))
        .into_iter()
        .filter(|(t, _)| t == "footnote")
        .filter_map(|(_, a)| a.get("id").cloned())
        .collect();
    for id in refs {
        if !defined.contains(&id) {
            out.push(Violation {
                part: "word/footnotes.xml".into(),
                rule: "FN-001",
                message: format!("脚注引用 {id} 没有对应定义"),
            });
        }
    }
}

/// 提取全部开始/空元素标签(本地名 + 属性,属性键去命名空间前缀)。
fn tags(xml: &str) -> Vec<Tag> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = local(e.name().as_ref());
                let mut attrs = Attrs::new();
                for a in e.attributes().flatten() {
                    let key = local(a.key.as_ref());
                    attrs.insert(key, a.value.to_string());
                }
                out.push((name, attrs));
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

fn local(raw: &str) -> String {
    raw.rsplit(':').next().unwrap_or(raw).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus() -> Vec<u8> {
        let req = crate::fs::docgen::DocRequest {
            title: "论文".into(),
            body: "# 第一章 {#sec-a}\n\n图 1 架构 {#fig-a}\n\n引用 [@fig-a]。".into(),
            template: Some("thesis".into()),
            ..Default::default()
        };
        crate::fs::docgen::render(&req).unwrap()
    }

    fn read_document(bytes: &[u8]) -> String {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
        let mut doc = String::new();
        zip.by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut doc)
            .unwrap();
        doc
    }

    #[test]
    fn clean_package_passes() {
        let req = crate::fs::docgen::DocRequest {
            title: "论文".into(),
            body: "# 第一章\n\n图 1 架构\n\n引用 [@fig-1]。".into(),
            template: Some("thesis".into()),
            ..Default::default()
        };
        let bytes = crate::fs::docgen::render(&req).unwrap();
        assert!(
            validate_package(&bytes).is_empty(),
            "{:?}",
            validate_package(&bytes)
        );
    }

    #[test]
    fn detects_missing_relationship_target_and_broken_bookmark() {
        let bytes = corpus();
        // 破坏一个 bookmarkEnd,制造 BM-001
        let broken = read_document(&bytes).replacen("<w:bookmarkEnd", "<w:bookmarkEndX", 1);
        let damaged = repack(bytes, "word/document.xml", broken.as_bytes());
        let v = validate_package(&damaged);
        assert!(v.iter().any(|x| x.rule == "BM-001"), "{v:?}");

        // 删除关系目标 part,制造 REL-001
        let mut parts = unpack(&damaged).unwrap();
        parts.remove("word/styles.xml");
        let v2 = validate_package(&repack_map(parts));
        assert!(v2.iter().any(|x| x.rule == "REL-001"), "{v2:?}");
    }

    #[test]
    fn detects_second_root_in_relationships() {
        let bytes = corpus();
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes.clone())).unwrap();
        let mut rels = String::new();
        zip.by_name("word/_rels/document.xml.rels")
            .unwrap()
            .read_to_string(&mut rels)
            .unwrap();
        drop(zip);
        let broken = rels.replace(
            "</Relationships>",
            "</Relationships><Relationship Id=\"rIdX\" Target=\"x.xml\"/>",
        );
        let damaged = repack(bytes, "word/_rels/document.xml.rels", broken.as_bytes());
        let v = validate_package(&damaged);
        assert!(v.iter().any(|x| x.rule == "XML-001"), "{v:?}");
    }

    fn repack(bytes: Vec<u8>, name: &str, data: &[u8]) -> Vec<u8> {
        let mut parts = unpack(&bytes).unwrap();
        parts.insert(name.to_string(), data.to_vec());
        repack_map(parts)
    }

    fn repack_map(parts: HashMap<String, Vec<u8>>) -> Vec<u8> {
        use std::io::Write;
        let mut out = Cursor::new(Vec::new());
        let mut zw = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in parts {
            zw.start_file(name, opts).unwrap();
            zw.write_all(&data).unwrap();
        }
        zw.finish().unwrap();
        out.into_inner()
    }
}
