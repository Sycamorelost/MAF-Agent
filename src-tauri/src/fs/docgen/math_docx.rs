//! docx 公式注入:把 `word/document.xml` 里的占位符替换成 OMML。
//!
//! 占位符由 [`super::math::token`] 生成;这里负责拆 run、插入
//! `<m:oMath>` / `<m:oMathPara>` 并补 `xmlns:m` 声明。
//! 没有占位符时原样返回,避免无谓的重新打包。

use std::io::{Cursor, Read, Write};

use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::math::{TOKEN, omml};

const NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";

/// 把 docx 包内 `word/document.xml` 的占位符替换为 OMML。
pub fn inject_docx_math(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(&bytes)).map_err(|e| format!("解析 docx 失败: {e}"))?;
    let mut xml = String::new();
    {
        let mut f = archive
            .by_name("word/document.xml")
            .map_err(|e| format!("缺少 document.xml: {e}"))?;
        f.read_to_string(&mut xml)
            .map_err(|e| format!("读取 document.xml 失败: {e}"))?;
    }
    if !xml.contains(TOKEN) {
        return Ok(bytes);
    }
    let xml = substitute(&xml)?;
    let mut out = Cursor::new(Vec::new());
    let mut zw = zip::ZipWriter::new(&mut out);
    let opts = zip::write::SimpleFileOptions::default();
    for i in 0..archive.len() {
        let mut f = archive
            .by_index(i)
            .map_err(|e| format!("读取 docx 条目失败: {e}"))?;
        let name = f
            .name()
            .map_err(|e| format!("读取条目名失败: {e}"))?
            .to_string();
        if f.is_dir() {
            zw.add_directory(name, opts)
                .map_err(|e| format!("写入目录失败: {e}"))?;
            continue;
        }
        zw.start_file(name.clone(), opts)
            .map_err(|e| format!("写入条目失败: {e}"))?;
        if name == "word/document.xml" {
            zw.write_all(xml.as_bytes())
                .map_err(|e| format!("写入公式失败: {e}"))?;
        } else {
            std::io::copy(&mut f, &mut zw).map_err(|e| format!("复制条目失败: {e}"))?;
        }
    }
    zw.finish().map_err(|e| format!("打包 docx 失败: {e}"))?;
    Ok(out.into_inner())
}

fn substitute(xml: &str) -> Result<String, String> {
    let mut out = String::with_capacity(xml.len() + 512);
    let mut pos = 0usize;
    while let Some(rel) = xml[pos..].find(TOKEN) {
        let p = pos + rel;
        let Some(rel_end) = xml[p + TOKEN.len()..].find("@@") else {
            break;
        };
        let end = p + TOKEN.len() + rel_end + 2;
        let Some(t_start) = last_wt_open(xml, p) else {
            break;
        };
        let Some(t_close_rel) = xml[end..].find("</w:t>") else {
            break;
        };
        let t_close = end + t_close_rel;
        let Some(r_close_rel) = xml[t_close..].find("</w:r>") else {
            break;
        };
        let r_close = t_close + r_close_rel + "</w:r>".len();

        let (latex, display) = decode(&xml[p..end])?;
        out.push_str(&xml[pos..t_start]);
        out.push_str("</w:r>");
        out.push_str(&omml(&latex, display));
        out.push_str("<w:r><w:t xml:space=\"preserve\"></w:t></w:r>");
        pos = r_close;
    }
    out.push_str(&xml[pos..]);
    Ok(with_math_ns(&out))
}

/// 占位符所在 `<w:t>` 的起点(排除 `<w:tbl>`/`<w:tc>` 等前缀)。
fn last_wt_open(xml: &str, before: usize) -> Option<usize> {
    let mut hit = None;
    for (idx, _) in xml[..before].match_indices("<w:t") {
        let next = xml[idx + 4..].chars().next();
        if matches!(next, Some('>') | Some(' ') | Some('/')) {
            hit = Some(idx);
        }
    }
    hit
}

fn decode(token: &str) -> Result<(String, bool), String> {
    let body = token
        .strip_prefix(TOKEN)
        .and_then(|s| s.strip_suffix("@@"))
        .ok_or_else(|| "公式占位符格式错误".to_string())?;
    let (kind, b64) = body
        .split_once(':')
        .ok_or_else(|| "公式占位符缺少类型".to_string())?;
    let bytes = STANDARD
        .decode(b64.as_bytes())
        .map_err(|e| format!("公式解码失败: {e}"))?;
    let latex = String::from_utf8(bytes).map_err(|e| format!("公式不是 UTF-8: {e}"))?;
    Ok((latex, kind == "1"))
}

fn with_math_ns(xml: &str) -> String {
    if xml.contains("xmlns:m=") {
        return xml.to_string();
    }
    match xml.find("<w:document") {
        Some(p) => {
            let mut out = String::with_capacity(xml.len() + NS.len() + 8);
            out.push_str(&xml[..p + "<w:document".len()]);
            out.push_str(&format!(" xmlns:m=\"{NS}\""));
            out.push_str(&xml[p + "<w:document".len()..]);
            out
        }
        None => xml.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::docgen::math::token;

    #[test]
    fn token_roundtrip() {
        let t = token(r"a^2+b^2", true);
        let (latex, display) = decode(&t).unwrap();
        assert_eq!(latex, r"a^2+b^2");
        assert!(display);
    }

    #[test]
    fn substitutes_placeholder_inside_run() {
        let token = token("x^2", false);
        let xml = format!(
            r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:rPr/><w:t>{token}</w:t></w:r></w:p></w:body></w:document>"#
        );
        let out = substitute(&xml).unwrap();
        assert!(out.contains("xmlns:m="));
        assert!(out.contains("<m:oMath>"));
        assert!(!out.contains(TOKEN));
        assert!(out.contains("</w:t></w:r>"));
    }
}
