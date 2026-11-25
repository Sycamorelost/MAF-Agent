//! 显式 `formula_mode=svg` 的 DOCX 注入。
//!
//! 正文先写 `@@SVG:<base64-latex>@@` 占位符,本模块在打包后把占位所在 run
//! 替换为 DrawingML 图片:PNG 作为通用回退,同时挂 `asvg:svgBlip` 让
//! 新版 Word 使用矢量副本。两者都写入包内媒体目录并补关系与 Content Types。
//!
//! 默认 OMML 路线不经过这里;本模块只在用户显式请求时调用。

use std::io::{Cursor, Read, Write};

use base64::{Engine as _, engine::general_purpose::STANDARD};

const TOKEN: &str = "@@SVG:";
const REL_IMAGE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const RELS_PATH: &str = "word/_rels/document.xml.rels";
const CT_PATH: &str = "[Content_Types].xml";

/// 生成占位符(原始 LaTeX 以 base64 承载,避免 XML 转义问题)。
pub fn token(latex: &str) -> String {
    format!("{TOKEN}{}@@", STANDARD.encode(latex.trim().as_bytes()))
}

fn decode(token: &str) -> Result<String, String> {
    let body = token
        .strip_prefix(TOKEN)
        .and_then(|s| s.strip_suffix("@@"))
        .ok_or_else(|| "SVG 公式占位符格式错误".to_string())?;
    let bytes = STANDARD
        .decode(body.as_bytes())
        .map_err(|e| format!("SVG 公式解码失败:{e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("SVG 公式不是 UTF-8:{e}"))
}

/// 把包内 SVG 公式占位符替换成矢量图片(含 PNG 回退)。
pub fn inject_svg_formulas(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes.clone())).map_err(|e| format!("解析 docx 失败:{e}"))?;
    let mut doc = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|e| format!("缺少 document.xml:{e}"))?
        .read_to_string(&mut doc)
        .map_err(|e| format!("读取 document.xml 失败:{e}"))?;
    if !doc.contains(TOKEN) {
        return Ok(bytes);
    }
    let mut rels = read_part(&mut archive, RELS_PATH).unwrap_or_else(empty_rels);
    let mut types = read_part(&mut archive, CT_PATH).unwrap_or_else(empty_types);

    // 已有媒体数与关系 id 从包内现状推导,避免覆盖用户图片
    let mut next_image = count_media(&mut archive) + 1;
    let mut next_rid = max_rid(&rels) + 1;
    let mut next_docpr = 90_000u32;

    let mut out = String::with_capacity(doc.len() + 4096);
    let mut pos = 0usize;
    let mut media: Vec<(String, Vec<u8>)> = Vec::new();
    while let Some(rel) = doc[pos..].find(TOKEN) {
        let start = pos + rel;
        let Some(rel_end) = doc[start + TOKEN.len()..].find("@@") else {
            break;
        };
        let end = start + TOKEN.len() + rel_end + 2;
        let Some((run_start, run_end)) = run_bounds(&doc, start) else {
            break;
        };
        let latex = decode(&doc[start..end])?;
        let svg = super::math::vector_fallback::render(&latex)?;
        let (png, pw, ph) = super::math::vector_fallback::render_png(&latex)?;

        let name = format!("formula{next_image}");
        let r_png = format!("rId{next_rid}");
        let r_svg = format!("rId{}", next_rid + 1);
        next_rid += 2;
        let docpr = next_docpr;
        next_docpr += 1;
        let cx = (svg.width as u64 * 9525).max(1);
        let cy = (svg.height as u64 * 9525).max(1);

        out.push_str(&doc[pos..run_start]);
        out.push_str(&drawing(docpr, cx, cy, &r_png, &r_svg));
        pos = run_end;

        append_relationship(&mut rels, &relationship(&r_png, &format!("media/{name}.png")))?;
        append_relationship(&mut rels, &relationship(&r_svg, &format!("media/{name}.svg")))?;
        media.push((format!("word/media/{name}.png"), png));
        media.push((format!("word/media/{name}.svg"), svg.svg.into_bytes()));
        let _ = (pw, ph);
        next_image += 1;
    }
    out.push_str(&doc[pos..]);

    ensure_default(&mut types, "png", "image/png");
    ensure_default(&mut types, "svg", "image/svg+xml");

    write_package(&mut archive, &out, &rels, &types, &media)
}

fn read_part<R: Read + std::io::Seek>(a: &mut zip::ZipArchive<R>, name: &str) -> Option<String> {
    let mut s = String::new();
    a.by_name(name).ok()?.read_to_string(&mut s).ok()?;
    Some(s)
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

fn count_media<R: Read + std::io::Seek>(a: &mut zip::ZipArchive<R>) -> usize {
    let mut n = 0;
    for i in 0..a.len() {
        if let Ok(f) = a.by_index(i)
            && let Ok(name) = f.name()
            && name.starts_with("word/media/")
        {
            n += 1;
        }
    }
    n
}

fn max_rid(rels: &str) -> u32 {
    let mut max = 0;
    for (idx, _) in rels.match_indices("Id=\"rId") {
        let rest = &rels[idx + 8..];
        let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(v) = num.parse::<u32>() {
            max = max.max(v);
        }
    }
    max
}

fn relationship(rid: &str, target: &str) -> String {
    format!(
        "<Relationship Id=\"{rid}\" Type=\"{REL_IMAGE}\" Target=\"{target}\"/>"
    )
}

fn ensure_default(types: &mut String, ext: &str, mime: &str) {
    let probe = format!("Extension=\"{ext}\"");
    if types.contains(&probe) {
        return;
    }
    let entry = format!("<Default Extension=\"{ext}\" ContentType=\"{mime}\"/>");
    if let Some(p) = types.rfind("</Types>") {
        types.insert_str(p, &entry);
    }
}

fn drawing(docpr: u32, cx: u64, cy: u64, r_png: &str, r_svg: &str) -> String {
    format!(
        "<w:r><w:drawing><wp:inline \
xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/>\
<wp:docPr id=\"{docpr}\" name=\"Formula {docpr}\"/><a:graphic \
xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
<a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
<pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
<pic:nvPicPr><pic:cNvPr id=\"{docpr}\" name=\"formula{docpr}.svg\"/><pic:cNvPicPr/></pic:nvPicPr>\
<pic:blipFill><a:blip \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
r:embed=\"{r_png}\"><a:extLst><a:ext \
uri=\"{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}\"><asvg:svgBlip \
xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
r:embed=\"{r_svg}\"/></a:ext></a:extLst></a:blip><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
<pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData>\
</a:graphic></wp:inline></w:drawing></w:r>"
    )
}

/// 占位符所在 run 的 [起, 止) 区间。
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

fn write_package(
    archive: &mut zip::ZipArchive<Cursor<Vec<u8>>>,
    doc: &str,
    rels: &str,
    types: &str,
    media: &[(String, Vec<u8>)],
) -> Result<Vec<u8>, String> {
    let mut out = Cursor::new(Vec::new());
    let mut zw = zip::ZipWriter::new(&mut out);
    let opts = zip::write::SimpleFileOptions::default();
    for i in 0..archive.len() {
        let mut f = archive
            .by_index(i)
            .map_err(|e| format!("读取 docx 条目失败:{e}"))?;
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
            "word/document.xml" => zw
                .write_all(doc.as_bytes())
                .map_err(|e| format!("写入公式失败:{e}"))?,
            RELS_PATH => zw
                .write_all(rels.as_bytes())
                .map_err(|e| format!("写入关系失败:{e}"))?,
            CT_PATH => zw
                .write_all(types.as_bytes())
                .map_err(|e| format!("写入类型失败:{e}"))?,
            _ => {
                std::io::copy(&mut f, &mut zw).map_err(|e| format!("复制条目失败:{e}"))?;
            }
        }
    }
    for (name, data) in media {
        zw.start_file(name.clone(), opts)
            .map_err(|e| format!("写入媒体失败:{e}"))?;
        zw.write_all(data).map_err(|e| format!("写入媒体失败:{e}"))?;
    }
    zw.finish().map_err(|e| format!("打包 docx 失败:{e}"))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        let mut zw = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default();
        let tok = token("a^2+b^2");
        let doc = format!(
            "<w:document xmlns:w=\"w\"><w:body><w:p><w:r><w:t>{tok}</w:t></w:r></w:p></w:body></w:document>"
        );
        for (name, body) in [
            ("word/document.xml", doc),
            (RELS_PATH, empty_rels()),
            (
                CT_PATH,
                "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"xml\" ContentType=\"application/xml\"/></Types>"
                    .to_string(),
            ),
        ] {
            zw.start_file(name, opts).unwrap();
            zw.write_all(body.as_bytes()).unwrap();
        }
        zw.finish().unwrap();
        out.into_inner()
    }

    #[test]
    fn injects_media_relationships_and_content_types() {
        let out = inject_svg_formulas(fixture()).unwrap();
        let mut zip = zip::ZipArchive::new(Cursor::new(out)).unwrap();
        let mut doc = String::new();
        zip.by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut doc)
            .unwrap();
        assert!(doc.contains("svgBlip"), "缺少矢量关系");
        assert!(doc.contains("<wp:inline"), "缺少图片容器");
        assert!(!doc.contains(TOKEN), "占位符未被替换");
        let mut rels = String::new();
        zip.by_name(RELS_PATH)
            .unwrap()
            .read_to_string(&mut rels)
            .unwrap();
        assert!(rels.contains("media/formula1.png"));
        assert!(rels.contains("media/formula1.svg"));
        // 关系必须在根节点内,否则 Word 打开时提示文件损坏
        let parsed = roxmltree::Document::parse(&rels).expect("document.xml.rels 必须是合法 XML");
        assert_eq!(parsed.root_element().tag_name().name(), "Relationships");
        assert_eq!(
            parsed
                .root_element()
                .children()
                .filter(|n| n.has_tag_name("Relationship"))
                .count(),
            2,
            "图片关系数量不符"
        );
        let mut types = String::new();
        zip.by_name(CT_PATH)
            .unwrap()
            .read_to_string(&mut types)
            .unwrap();
        assert!(types.contains("image/svg+xml"));
        assert!(zip.by_name("word/media/formula1.png").is_ok());
        assert!(zip.by_name("word/media/formula1.svg").is_ok());
    }

    #[test]
    fn no_token_is_noop() {
        let raw = fixture();
        assert!(inject_svg_formulas(raw.clone()).is_ok());
        assert!(!raw.is_empty());
    }

    #[test]
    fn token_roundtrip() {
        let t = token(r"\frac{1}{2}");
        assert_eq!(decode(&t).unwrap(), r"\frac{1}{2}");
    }
}
