//! reference DOCX 安全导入:只读取已知 OOXML 子集,映射为通用样式参数包。
//!
//! 安全约束:
//! - 文件 ≤ 20MB、条目 ≤ 2000、单条目解压 ≤ 5MB、压缩比 ≤ 100;
//! - 拒绝宏 / OLE / ActiveX 部件,拒绝 DTD 与实体声明;
//! - 只读取 `styles.xml`、`document.xml` 与 `numbering.xml`,不读图片、
//!   字体、外部关系,也不把原始 OOXML 复制进结果。
//!
//! 导入结果只包含可公开复用的字体、字号、间距、缩进、页面与编号方案;
//! 单位徽标等受限资产一律忽略并记录降级警告。

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::package::StylePack;

const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_ENTRIES: usize = 2_000;
const MAX_PART_BYTES: u64 = 5 * 1024 * 1024;
const MAX_RATIO: u64 = 100;

/// 导入结果。
#[derive(Debug)]
pub(super) struct Imported {
    pub pack: StylePack,
    pub name: String,
    pub sha256: String,
    pub warnings: Vec<String>,
}

/// 读取并映射 reference DOCX。
pub(super) fn import(path: &Path) -> Result<Imported, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("读取 reference DOCX 失败:{e}"))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "reference DOCX 超过 {} MB 上限",
            MAX_FILE_BYTES / 1024 / 1024
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("读取 reference DOCX 失败:{e}"))?;
    let sha256 = hex(&Sha256::digest(&bytes));

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("reference DOCX 不是有效的压缩包:{e}"))?;
    if zip.len() > MAX_ENTRIES {
        return Err("reference DOCX 条目数超限".into());
    }
    for i in 0..zip.len() {
        let f = zip
            .by_index(i)
            .map_err(|e| format!("读取 reference DOCX 条目失败:{e}"))?;
        let name = f
            .name()
            .map_err(|e| format!("读取 reference DOCX 条目名失败:{e}"))?
            .to_string();
        let lower = name.to_ascii_lowercase();
        if lower.contains("vbaproject")
            || lower.contains("oleobject")
            || lower.contains("activex")
            || lower.ends_with(".bin")
        {
            return Err(format!("reference DOCX 含受限部件:{name}"));
        }
        if f.size() > MAX_PART_BYTES {
            return Err(format!("reference DOCX 条目过大:{name}"));
        }
        if f.compressed_size() > 0 && f.size() / f.compressed_size() > MAX_RATIO {
            return Err(format!("reference DOCX 条目压缩比异常:{name}"));
        }
    }

    let styles = read_part(&mut zip, "word/styles.xml")?
        .ok_or_else(|| "reference DOCX 缺少 word/styles.xml".to_string())?;
    let document = read_part(&mut zip, "word/document.xml")?.unwrap_or_default();
    let numbering = read_part(&mut zip, "word/numbering.xml")?;

    let mut warnings = Vec::new();
    let mut pack = StylePack {
        name: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("reference")
            .to_string(),
        ..StylePack::default()
    };
    apply_styles(&styles, &mut pack, &mut warnings)?;
    apply_section(&document, &mut pack, &mut warnings)?;
    if let Some(xml) = numbering {
        apply_numbering(&xml, &mut pack, &mut warnings);
    }
    let name = pack.name.clone();
    Ok(Imported {
        pack,
        name,
        sha256,
        warnings,
    })
}

fn read_part(
    zip: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>,
    name: &str,
) -> Result<Option<String>, String> {
    let mut text = String::new();
    let Ok(mut f) = zip.by_name(name) else {
        return Ok(None);
    };
    f.read_to_string(&mut text)
        .map_err(|e| format!("读取 {name} 失败:{e}"))?;
    if text.contains("<!DOCTYPE") || text.contains("<!ENTITY") {
        return Err(format!("{name} 含 DTD/实体声明,已拒绝导入"));
    }
    Ok(Some(text))
}

fn apply_styles(xml: &str, pack: &mut StylePack, warnings: &mut Vec<String>) -> Result<(), String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("styles.xml 解析失败:{e}"))?;
    // 默认字体
    if let Some(fonts) = doc
        .descendants()
        .find(|n| n.has_tag_name("rPrDefault"))
        .and_then(|n| n.descendants().find(|c| c.has_tag_name("rFonts")))
    {
        pack.latin = attr(&fonts, "ascii").or_else(|| attr(&fonts, "hAnsi"));
        pack.east_asia = attr(&fonts, "eastAsia");
    }
    let mut skipped = 0usize;
    for style in doc.descendants().filter(|n| n.has_tag_name("style")) {
        if attr(&style, "type").as_deref() != Some("paragraph") {
            continue;
        }
        let Some(id) = attr(&style, "styleId") else {
            continue;
        };
        match id.as_str() {
            "Normal" => {
                if let Some(sz) = style
                    .descendants()
                    .find(|n| n.has_tag_name("sz"))
                    .and_then(|n| attr(&n, "val"))
                    .and_then(|v| v.parse::<usize>().ok())
                {
                    pack.body_size = Some(sz);
                }
                if let Some(sp) = style.descendants().find(|n| n.has_tag_name("spacing")) {
                    let line = attr(&sp, "line").and_then(|v| v.parse::<i32>().ok());
                    match (line, attr(&sp, "lineRule").as_deref()) {
                        (Some(v), Some("exact" | "atLeast")) => pack.fixed_line = Some(v),
                        (Some(v), _) => pack.body_line = Some(v),
                        _ => {}
                    }
                }
                if let Some(ind) = style.descendants().find(|n| n.has_tag_name("ind"))
                    && let Some(v) = attr(&ind, "firstLine").and_then(|v| v.parse::<i32>().ok())
                {
                    pack.body_indent = Some(v.max(0));
                }
            }
            "Heading1" | "Heading2" | "Heading3" | "Heading4" => {
                let idx = id["Heading".len()..].parse::<usize>().unwrap_or(0);
                if (1..=4).contains(&idx)
                    && let Some(sz) = style
                        .descendants()
                        .find(|n| n.has_tag_name("sz"))
                        .and_then(|n| attr(&n, "val"))
                        .and_then(|v| v.parse::<usize>().ok())
                {
                    let mut sizes = pack.heading_sizes.unwrap_or([32, 28, 24, 24]);
                    sizes[idx - 1] = sz;
                    pack.heading_sizes = Some(sizes);
                }
            }
            _ => skipped += 1,
        }
    }
    if skipped > 0 {
        warnings.push(format!("已忽略 {skipped} 个未识别的段落样式"));
    }
    Ok(())
}

fn apply_section(
    xml: &str,
    pack: &mut StylePack,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    if xml.trim().is_empty() {
        warnings.push("未读取到 document.xml,页面尺寸与边距沿用基础模板".into());
        return Ok(());
    }
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("document.xml 解析失败:{e}"))?;
    if let Some(sz) = doc.descendants().find(|n| n.has_tag_name("pgSz")) {
        let w = attr(&sz, "w").and_then(|v| v.parse::<u32>().ok());
        let h = attr(&sz, "h").and_then(|v| v.parse::<u32>().ok());
        if let (Some(w), Some(h)) = (w, h)
            && (8_000..=40_000).contains(&w)
            && (8_000..=40_000).contains(&h)
        {
            pack.page = Some([w, h]);
        } else {
            warnings.push("页面尺寸超出支持范围,已沿用基础模板".into());
        }
    }
    if let Some(mar) = doc.descendants().find(|n| n.has_tag_name("pgMar")) {
        let get = |k: &str| attr(&mar, k).and_then(|v| v.parse::<i32>().ok());
        if let (Some(t), Some(b), Some(l), Some(r)) =
            (get("top"), get("bottom"), get("left"), get("right"))
        {
            pack.margins = Some([t.max(0), b.max(0), l.max(0), r.max(0)]);
        }
    }
    Ok(())
}

fn apply_numbering(xml: &str, pack: &mut StylePack, warnings: &mut Vec<String>) {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        warnings.push("numbering.xml 解析失败,已跳过编号方案导入".into());
        return;
    };
    let fmt = doc
        .descendants()
        .filter(|n| n.has_tag_name("lvl"))
        .filter(|n| attr(n, "ilvl").as_deref() == Some("0"))
        .find_map(|n| {
            n.descendants()
                .find(|c| c.has_tag_name("numFmt"))
                .and_then(|c| attr(&c, "val"))
        });
    match fmt.as_deref() {
        Some("chineseCounting" | "chineseCountingThousand" | "ideographTraditional") => {
            pack.numbering = Some("official".into());
        }
        Some("decimal") => pack.numbering = Some("academic".into()),
        Some(other) => warnings.push(format!("编号格式 `{other}` 暂不支持,已跳过")),
        None => {}
    }
}

fn attr(node: &roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    node.attribute(name)
        .or_else(|| {
            // 带命名空间前缀时用本地名兜底
            node.attributes()
                .find(|a| a.name().split(':').next_back() == Some(name))
                .map(|a| a.value())
        })
        .map(str::to_string)
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixture(styles: &str, document: &str) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        let mut zw = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in [
            ("word/styles.xml", styles.to_string()),
            ("word/document.xml", document.to_string()),
        ] {
            zw.start_file(name, opts).unwrap();
            zw.write_all(body.as_bytes()).unwrap();
        }
        zw.finish().unwrap();
        out.into_inner()
    }

    fn write_tmp(bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "docgen-ref-{}-{}.docx",
            std::process::id(),
            bytes.len()
        ));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    const STYLES: &str = r#"<w:styles xmlns:w="w"><w:docDefaults><w:rPrDefault>
<w:rPr><w:rFonts w:ascii="Arial" w:eastAsia="思源宋体"/></w:rPr></w:rPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:styleId="Normal"><w:rPr><w:sz w:val="28"/></w:rPr>
<w:pPr><w:spacing w:line="400" w:lineRule="auto"/><w:ind w:firstLine="560"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:rPr><w:sz w:val="36"/></w:rPr></w:style>
</w:styles>"#;

    const DOCUMENT: &str = r#"<w:document xmlns:w="w"><w:body><w:sectPr>
<w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:bottom="1440" w:left="1701" w:right="1701"/>
</w:sectPr></w:body></w:document>"#;

    #[test]
    fn imports_fonts_sizes_and_page() {
        let path = write_tmp(&fixture(STYLES, DOCUMENT));
        let r = import(&path).unwrap();
        assert_eq!(r.pack.latin.as_deref(), Some("Arial"));
        assert_eq!(r.pack.east_asia.as_deref(), Some("思源宋体"));
        assert_eq!(r.pack.body_size, Some(28));
        assert_eq!(r.pack.body_line, Some(400));
        assert_eq!(r.pack.body_indent, Some(560));
        assert_eq!(r.pack.heading_sizes.unwrap()[0], 36);
        assert_eq!(r.pack.page, Some([11906, 16838]));
        assert_eq!(r.pack.margins, Some([1440, 1440, 1701, 1701]));
        assert_eq!(r.sha256.len(), 64);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rejects_macro_parts_and_dtd() {
        let mut out = std::io::Cursor::new(Vec::new());
        let mut zw = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default();
        zw.start_file("word/vbaProject.bin", opts).unwrap();
        zw.write_all(b"x").unwrap();
        zw.finish().unwrap();
        let path = write_tmp(&out.into_inner());
        assert!(import(&path).is_err());
        let _ = std::fs::remove_file(path);

        let path = write_tmp(&fixture(
            "<!DOCTYPE x><w:styles xmlns:w=\"w\"></w:styles>",
            DOCUMENT,
        ));
        assert!(import(&path).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_styles_is_error() {
        let mut out = std::io::Cursor::new(Vec::new());
        let mut zw = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default();
        zw.start_file("word/document.xml", opts).unwrap();
        zw.write_all(DOCUMENT.as_bytes()).unwrap();
        zw.finish().unwrap();
        let path = write_tmp(&out.into_inner());
        assert!(import(&path).is_err());
        let _ = std::fs::remove_file(path);
    }
}
