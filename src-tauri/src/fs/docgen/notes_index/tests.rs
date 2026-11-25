use super::*;

#[test]
fn transform_extracts_definitions_and_index() {
    let body = "正文[^a]。\n\n[^a]: 尾注内容\n\n术语{{index:向量空间}}出现。";
    let out = transform(body);
    assert!(!out.contains("[^a]:"));
    assert!(out.contains(EN));
    assert!(out.contains(XE));
    assert!(out.contains("正文"));
}

#[test]
fn undefined_reference_is_kept() {
    let out = transform("见[^missing]处");
    assert_eq!(out, "见[^missing]处");
}

fn fixture(body: &str) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    let mut zw = zip::ZipWriter::new(&mut out);
    let opts = zip::write::SimpleFileOptions::default();
    let doc = format!(
        "<w:document xmlns:w=\"w\"><w:body><w:p><w:r><w:t>{body}</w:t></w:r></w:p>\
<w:sectPr/></w:body></w:document>"
    );
    for (name, text) in [
        ("word/document.xml", doc),
        (RELS_PATH, empty_rels()),
        (CT_PATH, empty_types()),
    ] {
        zw.start_file(name, opts).unwrap();
        zw.write_all(text.as_bytes()).unwrap();
    }
    zw.finish().unwrap();
    out.into_inner()
}

#[test]
fn injects_endnotes_and_index_field() {
    let body = format!(
        "{}正文{}",
        format_args!("{EN}{}@@", STANDARD.encode("注一")),
        format_args!("{XE}{}@@", STANDARD.encode("术语"))
    );
    let out = inject(fixture(&body)).unwrap();
    let report = crate::fs::docgen::validation::validate_package(&out);
    assert!(report.is_empty(), "注入后包校验失败:{report:?}");
    let mut zip = zip::ZipArchive::new(Cursor::new(out)).unwrap();
    let mut doc = String::new();
    zip.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut doc)
        .unwrap();
    assert!(doc.contains("endnoteReference"));
    assert!(doc.contains("INDEX"));
    assert!(doc.contains("XE"));
    let mut notes = String::new();
    zip.by_name(ENDNOTES_PATH)
        .unwrap()
        .read_to_string(&mut notes)
        .unwrap();
    assert!(notes.contains("注一"));
    let mut rels = String::new();
    zip.by_name(RELS_PATH)
        .unwrap()
        .read_to_string(&mut rels)
        .unwrap();
    assert!(rels.contains("endnotes.xml"));
    // 关系必须写在根节点内部:追加到闭合标签之后会让 Word 判定文件损坏
    let parsed = roxmltree::Document::parse(&rels).expect("document.xml.rels 必须是合法 XML");
    assert_eq!(parsed.root_element().tag_name().name(), "Relationships");
    assert!(
        parsed
            .root_element()
            .children()
            .any(|n| n.attribute("Id") == Some("rIdEndnotes")),
        "尾注关系不在 Relationships 根节点内"
    );
    let mut types = String::new();
    zip.by_name(CT_PATH)
        .unwrap()
        .read_to_string(&mut types)
        .unwrap();
    assert!(types.contains("endnotes+xml"));
}
