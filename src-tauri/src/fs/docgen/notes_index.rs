//! 书籍尾注与索引:Markdown 标记 → OOXML 尾注 / 索引域。
//!
//! 约定:
//! - `[^标签]` 为引用,`[^标签]: 内容` 为定义;定义行从正文移除。
//! - `{{index:术语}}` 为索引条目,Word 中右键「更新域」生成索引页码。
//!
//! 与公式一致,正文先写占位符,打包后再替换,避免 docx-rs 不支持的 OOXML 结构。

use std::io::{Cursor, Read, Write};

use base64::{Engine as _, engine::general_purpose::STANDARD};

const EN: &str = "@@EN:";
const XE: &str = "@@XE:";
const REL_ENDNOTES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes";
const CT_ENDNOTES: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml";
const RELS_PATH: &str = "word/_rels/document.xml.rels";
const CT_PATH: &str = "[Content_Types].xml";
const ENDNOTES_PATH: &str = "word/endnotes.xml";

/// 把尾注定义与索引标记改写为占位符;无定义时保留原文。
pub fn transform(body: &str) -> String {
    let mut defs = std::collections::HashMap::<String, String>::new();
    let mut kept = String::with_capacity(body.len());
    let lines: Vec<&str> = body.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("[^")
            && let Some((label, text)) = rest.split_once("]:")
            && !label.is_empty()
            && label.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            defs.insert(label.trim().to_string(), text.trim().to_string());
            continue;
        }
        kept.push_str(line);
        if i + 1 < lines.len() || body.ends_with('\n') {
            kept.push('\n');
        }
    }
    // 若首行即定义且后面无内容,保持空串
    if body.ends_with('\n') && kept.is_empty() {
        kept.clear();
    }
    let mut out = kept;
    for (label, text) in &defs {
        let token = format!("{EN}{}@@", STANDARD.encode(text.as_bytes()));
        out = out.replace(&format!("[^{label}]"), &token);
    }
    // 未定义的引用按普通文本保留,不制造悬空尾注
    let mut result = String::with_capacity(out.len());
    let mut rest = out.as_str();
    while let Some(start) = rest.find("{{index:") {
        result.push_str(&rest[..start]);
        let after = &rest[start + "{{index:".len()..];
        if let Some(end) = after.find("}}") {
            let term = after[..end].trim();
            if !term.is_empty() {
                result.push_str(&format!("{XE}{}@@", STANDARD.encode(term.as_bytes())));
            }
            rest = &after[end + 2..];
        } else {
            result.push_str(&rest[start..]);
            rest = "";
        }
    }
    result.push_str(rest);
    result
}

/// 是否存在需要落地的占位符。
pub fn has_markers(xml: &str) -> bool {
    xml.contains(EN) || xml.contains(XE)
}

/// 替换占位符,并按需写入 `endnotes.xml` 与索引域。
pub fn inject(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes.clone()))
        .map_err(|e| format!("解析 docx 失败:{e}"))?;
    let mut doc = string_part(&mut zip, "word/document.xml")
        .ok_or_else(|| "缺少 document.xml".to_string())?;
    if !has_markers(&doc) {
        return Ok(bytes);
    }
    let mut rels = string_part(&mut zip, RELS_PATH).unwrap_or_else(empty_rels);
    let has_endnotes_part = zip.by_name(ENDNOTES_PATH).is_ok();

    // 尾注与索引可能同处一个 run,必须按文本片段一次性重建
    let mut notes: Vec<String> = Vec::new();
    let mut indexed = false;
    doc = rewrite_runs(&doc, |kind, raw| {
        let value = decode(raw).unwrap_or_default();
        if kind == EN {
            let id = match notes.iter().position(|n| n == &value) {
                Some(i) => i + 1,
                None => {
                    notes.push(value);
                    notes.len()
                }
            };
            format!(
                "<w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr>\
<w:endnoteReference w:id=\"{id}\"/></w:r>"
            )
        } else if value.is_empty() {
            String::new()
        } else {
            indexed = true;
            format!(
                "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>\
<w:r><w:instrText xml:space=\"preserve\"> XE \"{}\" </w:instrText></w:r>\
<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
                escape_attr(&value)
            )
        }
    });

    if indexed {
        doc = insert_index(&doc);
    }

    let mut types = string_part(&mut zip, CT_PATH).unwrap_or_else(empty_types);
    let mut extra: Vec<(String, Vec<u8>)> = Vec::new();
    if !notes.is_empty() {
        if !has_endnotes_part {
            append_relationship(
                &mut rels,
                &format!(
                    "<Relationship Id=\"rIdEndnotes\" Type=\"{REL_ENDNOTES}\" Target=\"endnotes.xml\"/>"
                ),
            )?;
            if !types.contains(ENDNOTES_PATH) {
                let entry = format!(
                    "<Override PartName=\"/{ENDNOTES_PATH}\" ContentType=\"{CT_ENDNOTES}\"/>"
                );
                if let Some(p) = types.rfind("</Types>") {
                    types.insert_str(p, &entry);
                }
            }
            extra.push((ENDNOTES_PATH.to_string(), endnotes_xml(&notes).into_bytes()));
        }
    }

    write_package(&mut zip, &doc, &rels, &types, &extra)
}

fn endnotes_xml(notes: &[String]) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:endnotes xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:endnote w:type=\"separator\" w:id=\"-1\"><w:p><w:pPr><w:spacing w:after=\"0\" \
w:line=\"240\" w:lineRule=\"auto\"/></w:pPr><w:r><w:separator/></w:r></w:p></w:endnote>\
<w:endnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:pPr><w:spacing w:after=\"0\" \
w:line=\"240\" w:lineRule=\"auto\"/></w:pPr><w:r><w:continuationSeparator/></w:r></w:p></w:endnote>",
    );
    for (i, note) in notes.iter().enumerate() {
        xml.push_str(&format!(
            "<w:endnote w:id=\"{}\"><w:p><w:pPr><w:jc w:val=\"both\"/></w:pPr>\
<w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr><w:endnoteRef/></w:r>\
<w:r><w:t xml:space=\"preserve\"> {}</w:t></w:r></w:p></w:endnote>",
            i + 1,
            escape_text(note)
        ));
    }
    xml.push_str("</w:endnotes>");
    xml
}

fn insert_index(doc: &str) -> String {
    let block = "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>\
<w:r><w:instrText xml:space=\"preserve\"> INDEX \\c \"2\" </w:instrText></w:r>\
<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>\
<w:r><w:t>（索引将在 Word 中更新域后生成）</w:t></w:r>\
<w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>";
    // 索引放在最后一个 sectPr 之前,保证仍属于正文节
    if let Some(pos) = doc.rfind("<w:sectPr") {
        let mut out = String::with_capacity(doc.len() + block.len());
        out.push_str(&doc[..pos]);
        out.push_str(block);
        out.push_str(&doc[pos..]);
        out
    } else if let Some(pos) = doc.rfind("</w:body>") {
        let mut out = String::with_capacity(doc.len() + block.len());
        out.push_str(&doc[..pos]);
        out.push_str(block);
        out.push_str(&doc[pos..]);
        out
    } else {
        doc.to_string()
    }
}

/// 重建含尾注/索引标记的 run:文本片段各成一个 run,标记片段交给 `build`。
fn rewrite_runs<F: FnMut(&str, &str) -> String>(xml: &str, mut build: F) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut pos = 0usize;
    loop {
        let Some((start, prefix)) = find_marker(xml, pos) else {
            break;
        };
        let Some((run_start, run_end)) = run_bounds(xml, start) else {
            pos = start + prefix.len();
            continue;
        };
        out.push_str(&xml[pos..run_start]);
        let run = &xml[run_start..run_end];
        out.push_str(&rebuild_run(run, &mut build));
        pos = run_end;
    }
    out.push_str(&xml[pos..]);
    out
}

fn find_marker(xml: &str, from: usize) -> Option<(usize, &'static str)> {
    let a = xml[from..].find(EN).map(|i| (from + i, EN));
    let b = xml[from..].find(XE).map(|i| (from + i, XE));
    match (a, b) {
        (Some((pa, x)), Some((pb, y))) => Some(if pa <= pb { (pa, x) } else { (pb, y) }),
        (Some(v), None) | (None, Some(v)) => Some(v),
        (None, None) => None,
    }
}

fn rebuild_run<F: FnMut(&str, &str) -> String>(run: &str, build: &mut F) -> String {
    let (Some(open_end), Some(t_open), Some(t_close)) = (
        run.find('>').map(|i| i + 1),
        run.find("<w:t"),
        run.find("</w:t>"),
    ) else {
        return run.to_string();
    };
    let Some(t_inner) = run[t_open..].find('>').map(|i| t_open + i + 1) else {
        return run.to_string();
    };
    if t_close < t_inner {
        return run.to_string();
    }
    let rpr = if run[open_end..t_open].trim_start().starts_with("<w:rPr") {
        run[open_end..t_open].to_string()
    } else {
        String::new()
    };
    let text = &run[t_inner..t_close];
    let mut rebuilt = String::new();
    let mut cursor = 0usize;
    while let Some((at, prefix)) = find_marker(text, cursor) {
        let Some(tail) = text[at + prefix.len()..].find("@@") else {
            break;
        };
        let end = at + prefix.len() + tail;
        let plain = &text[cursor..at];
        if !plain.is_empty() {
            rebuilt.push_str(&format!(
                "<w:r>{rpr}<w:t xml:space=\"preserve\">{plain}</w:t></w:r>"
            ));
        }
        rebuilt.push_str(&build(prefix, &text[at + prefix.len()..end]));
        cursor = end + 2;
    }
    let tail = &text[cursor..];
    if !tail.is_empty() {
        rebuilt.push_str(&format!(
            "<w:r>{rpr}<w:t xml:space=\"preserve\">{tail}</w:t></w:r>"
        ));
    }
    if rebuilt.is_empty() {
        run.to_string()
    } else {
        rebuilt
    }
}

fn run_bounds(xml: &str, token_pos: usize) -> Option<(usize, usize)> {
    let t_open = last_tag_open(xml, "<w:t", token_pos)?;
    let r_open = last_tag_open(xml, "<w:r", t_open)?;
    let t_close = xml[token_pos..].find("</w:t>")? + token_pos;
    let r_close = xml[t_close..].find("</w:r>")? + t_close + "</w:r>".len();
    Some((r_open, r_close))
}

fn last_tag_open(xml: &str, tag: &str, before: usize) -> Option<usize> {
    let mut hit = None;
    for (idx, _) in xml[..before].match_indices(tag) {
        match xml[idx + tag.len()..].chars().next() {
            Some('>') | Some(' ') | Some('/') => hit = Some(idx),
            _ => {}
        }
    }
    hit
}

fn decode(raw: &str) -> Option<String> {
    let bytes = STANDARD.decode(raw.as_bytes()).ok()?;
    String::from_utf8(bytes).ok()
}

fn string_part(zip: &mut zip::ZipArchive<Cursor<Vec<u8>>>, name: &str) -> Option<String> {
    let mut text = String::new();
    zip.by_name(name).ok()?.read_to_string(&mut text).ok()?;
    Some(text)
}

/// 在 Relationships 根节点闭合前插入关系,避免写出第二个根节点。
fn append_relationship(rels: &mut String, entry: &str) -> Result<(), String> {
    let at = rels
        .rfind("</Relationships>")
        .ok_or_else(|| "document.xml.rels 缺少 Relationships 根节点".to_string())?;
    rels.insert_str(at, entry);
    Ok(())
}

fn empty_rels() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"></Relationships>"
        .into()
}

fn empty_types() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"></Types>".into()
}

fn write_package(
    zip: &mut zip::ZipArchive<Cursor<Vec<u8>>>,
    doc: &str,
    rels: &str,
    types: &str,
    extra: &[(String, Vec<u8>)],
) -> Result<Vec<u8>, String> {
    let mut out = Cursor::new(Vec::new());
    let mut zw = zip::ZipWriter::new(&mut out);
    let opts = zip::write::SimpleFileOptions::default();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| format!("读取条目失败:{e}"))?;
        let name = f
            .name()
            .map_err(|e| format!("读取条目名失败:{e}"))?
            .to_string();
        if f.is_dir() {
            zw.add_directory(name, opts)
                .map_err(|e| format!("写入目录失败:{e}"))?;
            continue;
        }
        zw.start_file(name.clone(), opts)
            .map_err(|e| format!("写入条目失败:{e}"))?;
        match name.as_str() {
            "word/document.xml" => zw.write_all(doc.as_bytes()),
            RELS_PATH => zw.write_all(rels.as_bytes()),
            CT_PATH => zw.write_all(types.as_bytes()),
            _ => std::io::copy(&mut f, &mut zw).map(|_| ()),
        }
        .map_err(|e| format!("写入条目失败:{e}"))?;
    }
    for (name, data) in extra {
        zw.start_file(name.clone(), opts)
            .map_err(|e| format!("写入尾注失败:{e}"))?;
        zw.write_all(data).map_err(|e| format!("写入尾注失败:{e}"))?;
    }
    zw.finish().map_err(|e| format!("打包 docx 失败:{e}"))?;
    Ok(out.into_inner())
}

fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attr(text: &str) -> String {
    escape_text(text).replace('"', "&quot;")
}

#[cfg(test)]
mod tests;
