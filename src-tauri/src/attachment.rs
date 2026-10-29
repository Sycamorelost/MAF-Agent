//! 文档理解管线(迭代 D1.2):按扩展名分类附件并提取纯文本,供 LLM 上下文注入。
//!
//! - docx:docx-rs 解析段落与表格文本
//! - xlsx/xls:calamine 遍历全部 sheet,按行拼 TSV
//! - pdf :pdf-extract 提取文本
//! - text/md/csv/tsv 等:直接按 UTF-8 文本读取(calamine 不支持 csv,故不走表格解析)
//! - 图片 / 其他:不提取(图片走 vision content 注入)

use std::path::Path;

/// 附件类别(按扩展名分类)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKind {
    Docx,
    Xlsx,
    Pdf,
    /// PPTX 演示文稿
    Pptx,
    /// EPUB 电子书
    Epub,
    /// HTML / HTM 网页
    Html,
    /// 纯文本 / Markdown / CSV / TSV / 配置等按文本读取的格式
    Text,
    Image,
    Other,
}

impl AttachmentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::Pdf => "pdf",
            Self::Pptx => "pptx",
            Self::Epub => "epub",
            Self::Html => "html",
            Self::Text => "text",
            Self::Image => "image",
            Self::Other => "other",
        }
    }
}

/// 按文件扩展名分类(大小写不敏感)。
pub fn classify(file_name: &str) -> AttachmentKind {
    let ext = file_name
        .rsplit('.')
        .next()
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "docx" => AttachmentKind::Docx,
        "xlsx" | "xlsm" | "xls" | "xla" | "xlsb" | "ods" => AttachmentKind::Xlsx,
        "pdf" => AttachmentKind::Pdf,
        "pptx" => AttachmentKind::Pptx,
        "epub" => AttachmentKind::Epub,
        "html" | "htm" => AttachmentKind::Html,
        "txt" | "md" | "markdown" | "text" | "log" | "csv" | "tsv" | "json" | "yaml" | "yml"
        | "toml" | "ini" | "conf" => AttachmentKind::Text,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" => AttachmentKind::Image,
        _ => AttachmentKind::Other,
    }
}

/// 单附件提取纯文本的字符预算(H-3,防解压炸弹/恶意文档导致 OOM):
/// 200 万字符(中文约 600 万字节)。docx/xlsx 在拼接过程中按字节闸提前停止,
/// pdf/plain 等库返回全量文本后由 clamp_text 精确截断。
pub const EXTRACT_BUDGET_CHARS: usize = 2_000_000;

/// 过程中的字节闸:按最坏每字符 4 字节估算,保证字符预算上限的内存边界。
const BUDGET_BYTES: usize = EXTRACT_BUDGET_CHARS * 4;

/// 在预算内向输出缓冲追加一行(含换行)。
/// 超过字节闸后按 UTF-8 字符边界回退截断、追加截断标记并置位 capped;
/// capped 一旦为 true,后续调用直接丢弃内容(调用方应尽快结束遍历)。
fn push_line_capped(out: &mut String, line: &str, capped: &mut bool) {
    if *capped {
        return;
    }
    out.push_str(line);
    out.push('\n');
    if out.len() > BUDGET_BYTES {
        let mut cut = BUDGET_BYTES;
        while cut > 0 && !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
        out.push_str("\n…(内容过长已截断)");
        *capped = true;
    }
}

/// 提取附件纯文本。图片/其他类型返回 None(不进入文本注入)。
/// 失败返回 Err(错误信息供前端展示),不会 panic。
pub fn extract_text(path: &Path, kind: AttachmentKind) -> Result<Option<String>, String> {
    match kind {
        AttachmentKind::Docx => extract_docx(path).map(Some),
        AttachmentKind::Xlsx => extract_xlsx(path).map(Some),
        AttachmentKind::Pdf => extract_pdf(path).map(Some),
        // R4:PPTX / EPUB / HTML 复用工作区 file_extract 的同一套解析器,
        // 不再出现「工作区能读、聊天附件读不了」的格式断层。
        AttachmentKind::Pptx => {
            let text = crate::fs::formats::pptx_text(path)?;
            Ok(Some(clamp_text(&text, EXTRACT_BUDGET_CHARS)))
        }
        AttachmentKind::Epub => {
            let text = crate::fs::formats::epub_text(path)?;
            Ok(Some(clamp_text(&text, EXTRACT_BUDGET_CHARS)))
        }
        AttachmentKind::Html => {
            let raw = crate::fs::formats::read_lossy(path)?;
            let text = crate::fs::formats::strip_tags(&raw);
            Ok(Some(clamp_text(text.trim(), EXTRACT_BUDGET_CHARS)))
        }
        AttachmentKind::Text => extract_plain_text(path).map(Some),
        AttachmentKind::Image | AttachmentKind::Other => Ok(None),
    }
}

/// 纯文本类:按 UTF-8 读取(非法字节 lossy 兜底),去掉 BOM 与首尾空白。
fn extract_plain_text(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取文本失败: {e}"))?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}').trim();
    // 库层已读全量(文件本身受 20MB 上限约束),这里按字符预算精确截断
    Ok(clamp_text(text, EXTRACT_BUDGET_CHARS))
}

/// docx:读字节 → docx_rs 解析 → 拼接全部段落与表格文本。
fn extract_docx(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取 docx 失败: {e}"))?;
    let doc = docx_rs::read_docx(&bytes).map_err(|e| format!("解析 docx 失败: {e}"))?;
    let mut out = String::new();
    let mut capped = false;
    for child in &doc.document.children {
        match child {
            docx_rs::DocumentChild::Paragraph(p) => {
                let line = p.raw_text();
                if !line.trim().is_empty() {
                    push_line_capped(&mut out, &line, &mut capped);
                }
            }
            docx_rs::DocumentChild::Table(t) => {
                let table = table_to_text(t, &mut capped);
                if !table.is_empty() {
                    push_line_capped(&mut out, &table, &mut capped);
                }
            }
            _ => {}
        }
        if capped {
            break;
        }
    }
    Ok(out.trim().to_string())
}

/// 表格 → 文本:每行以制表符连接单元格,单元格内多段落以空格连接。
/// capped 透传文档级预算标志,置位后停止后续行的拼接。
fn table_to_text(table: &docx_rs::Table, capped: &mut bool) -> String {
    let mut out = String::new();
    'rows: for row in &table.rows {
        let docx_rs::TableChild::TableRow(r) = row;
        let cells: Vec<String> = r
            .cells
            .iter()
            .map(|c| match c {
                docx_rs::TableRowChild::TableCell(cell) => {
                    let mut text = String::new();
                    for content in &cell.children {
                        if let docx_rs::TableCellContent::Paragraph(p) = content {
                            let line = p.raw_text();
                            let line = line.trim();
                            if !line.is_empty() {
                                if !text.is_empty() {
                                    text.push(' ');
                                }
                                text.push_str(line);
                            }
                        }
                    }
                    text
                }
            })
            .collect();
        push_line_capped(&mut out, &cells.join("\t"), capped);
        if *capped {
            break 'rows;
        }
    }
    out.trim_end().to_string()
}

/// xlsx:calamine 自动识别格式,遍历所有 sheet,每行单元格以制表符连接。
fn extract_xlsx(path: &Path) -> Result<String, String> {
    use calamine::{Reader, open_workbook_auto};
    let mut wb = open_workbook_auto(path).map_err(|e| format!("解析表格失败: {e}"))?;
    let mut out = String::new();
    let mut capped = false;
    'sheets: for name in wb.sheet_names().to_vec() {
        if let Ok(range) = wb.worksheet_range(&name) {
            // C3/F5:逐 sheet 输出并标注规模,前端与模型都能保留「哪个表、多少行」的语义
            let (rows, cols) = range.get_size();
            push_line_capped(
                &mut out,
                &format!("# Sheet: {name} ({rows} 行 × {cols} 列)"),
                &mut capped,
            );
            for row in range.rows() {
                let line: Vec<String> = row.iter().map(|c| c.to_string()).collect();
                push_line_capped(&mut out, &line.join("\t"), &mut capped);
                if capped {
                    break 'sheets;
                }
            }
        }
    }
    Ok(out.trim().to_string())
}

/// pdf:逐页提取(C3/F4),每页前置 `【第 N 页】` 标记,供分块保留页码。
/// 无文本层的扫描版 PDF 返回空串(上层据此提示需 OCR / 视觉模型)。
/// 库不提供流式接口,解析阶段无法中途限预算;输入文件已受 20MB 上限约束,
/// 提取后立即按字符预算截断,控制落库/注入规模。
fn extract_pdf(path: &Path) -> Result<String, String> {
    let pages =
        pdf_extract::extract_text_by_pages(path).map_err(|e| format!("解析 pdf 失败: {e}"))?;
    let mut out = String::new();
    for (i, p) in pages.iter().enumerate() {
        let t = p.trim();
        if t.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("【第 {} 页】\n{}\n", i + 1, t));
    }
    Ok(clamp_text(out.trim_end(), EXTRACT_BUDGET_CHARS))
}

/// 截断提取文本(注入与落库共用护栏):最多 max 字符,超出部分截断并追加标记。
pub fn clamp_text(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}\n…(内容过长已截断)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 测试临时目录:项目 target/test-tmp(D 盘合规),用例各自建子目录。
    fn tmp_dir(case: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-tmp")
            .join(case);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn classify_by_extension() {
        assert_eq!(classify("报告.DOCX"), AttachmentKind::Docx);
        assert_eq!(classify("data.xlsx"), AttachmentKind::Xlsx);
        assert_eq!(classify("old.xls"), AttachmentKind::Xlsx);
        assert_eq!(classify("a.csv"), AttachmentKind::Text);
        assert_eq!(classify("data.tsv"), AttachmentKind::Text);
        assert_eq!(classify("paper.pdf"), AttachmentKind::Pdf);
        assert_eq!(classify("截图.PNG"), AttachmentKind::Image);
        assert_eq!(classify("photo.jpeg"), AttachmentKind::Image);
        assert_eq!(classify("notes.txt"), AttachmentKind::Text);
        assert_eq!(classify("readme.md"), AttachmentKind::Text);
        assert_eq!(classify("README.MD"), AttachmentKind::Text);
        assert_eq!(classify("config.yaml"), AttachmentKind::Text);
        assert_eq!(classify("deck.pptx"), AttachmentKind::Pptx);
        assert_eq!(classify("book.epub"), AttachmentKind::Epub);
        assert_eq!(classify("page.HTML"), AttachmentKind::Html);
        assert_eq!(classify("page.htm"), AttachmentKind::Html);
        assert_eq!(classify("noext"), AttachmentKind::Other);
    }

    #[test]
    fn text_and_markdown_extract() {
        let dir = tmp_dir("text");
        let txt = dir.join("notes.txt");
        std::fs::write(&txt, "\u{feff}  纯文本内容\n第二行  ").unwrap();
        let text = extract_text(&txt, classify("notes.txt")).unwrap().unwrap();
        assert_eq!(text, "纯文本内容\n第二行");

        let md = dir.join("readme.md");
        std::fs::write(&md, "# 标题\n\n- 列表项\n").unwrap();
        let text = extract_text(&md, classify("readme.md")).unwrap().unwrap();
        assert!(
            text.contains("# 标题") && text.contains("- 列表项"),
            "got: {text}"
        );
    }

    #[test]
    fn csv_extract_as_text() {
        let dir = tmp_dir("csv");
        let path = dir.join("data.csv");
        std::fs::write(&path, "姓名,分数\n张三,95\n李四,88\n").unwrap();
        // csv 不走 calamine,而是按文本读取
        assert_eq!(classify("data.csv"), AttachmentKind::Text);
        let text = extract_text(&path, AttachmentKind::Text).unwrap().unwrap();
        assert!(text.contains("姓名,分数"), "got: {text}");
        assert!(text.contains("张三,95"));
        assert!(text.contains("李四,88"));
    }

    #[test]
    fn clamp_text_truncates() {
        assert_eq!(clamp_text("abc", 10), "abc");
        let long = "x".repeat(5000);
        let out = clamp_text(&long, 100);
        assert!(out.chars().count() < 120);
        assert!(out.contains("已截断"));
    }

    #[test]
    fn docx_roundtrip() {
        // 用 docx-rs 程序化生成 docx 再解析
        let dir = tmp_dir("docx");
        let path = dir.join("sample.docx");
        let doc = docx_rs::Docx::new()
            .add_paragraph(
                docx_rs::Paragraph::new()
                    .add_run(docx_rs::Run::new().add_text("会议纪要:项目里程碑确认")),
            )
            .add_paragraph(
                docx_rs::Paragraph::new().add_run(docx_rs::Run::new().add_text("第二阶段下月启动")),
            )
            .build();
        let mut buf = std::io::Cursor::new(Vec::new());
        doc.pack(&mut buf).unwrap();
        std::fs::write(&path, buf.into_inner()).unwrap();

        let text = extract_text(&path, AttachmentKind::Docx).unwrap().unwrap();
        assert!(text.contains("会议纪要:项目里程碑确认"), "got: {text}");
        assert!(text.contains("第二阶段下月启动"));
    }

    #[test]
    fn docx_extract_table() {
        // 段落 + 表格混排:表格行以制表符连接,单元格文本应被提取
        let dir = tmp_dir("docx-table");
        let path = dir.join("table.docx");
        let table = docx_rs::Table::new(vec![
            docx_rs::TableRow::new(vec![
                docx_rs::TableCell::new().add_paragraph(
                    docx_rs::Paragraph::new().add_run(docx_rs::Run::new().add_text("姓名")),
                ),
                docx_rs::TableCell::new().add_paragraph(
                    docx_rs::Paragraph::new().add_run(docx_rs::Run::new().add_text("分数")),
                ),
            ]),
            docx_rs::TableRow::new(vec![
                docx_rs::TableCell::new().add_paragraph(
                    docx_rs::Paragraph::new().add_run(docx_rs::Run::new().add_text("张三")),
                ),
                docx_rs::TableCell::new().add_paragraph(
                    docx_rs::Paragraph::new().add_run(docx_rs::Run::new().add_text("95")),
                ),
            ]),
        ]);
        let doc = docx_rs::Docx::new()
            .add_paragraph(
                docx_rs::Paragraph::new().add_run(docx_rs::Run::new().add_text("成绩表")),
            )
            .add_table(table)
            .build();
        let mut buf = std::io::Cursor::new(Vec::new());
        doc.pack(&mut buf).unwrap();
        std::fs::write(&path, buf.into_inner()).unwrap();

        let text = extract_text(&path, AttachmentKind::Docx).unwrap().unwrap();
        assert!(text.contains("成绩表"), "got: {text}");
        assert!(text.contains("姓名\t分数"), "got: {text}");
        assert!(text.contains("张三\t95"), "got: {text}");
    }

    #[test]
    fn xlsx_extract_all_sheets() {
        // 用 rust_xlsxwriter 程序化生成 xlsx 再解析
        let dir = tmp_dir("xlsx");
        let path = dir.join("data.xlsx");
        let mut wb = rust_xlsxwriter::Workbook::new();
        let ws = wb.add_worksheet().set_name("成绩").unwrap();
        ws.write_string(0, 0, "姓名").unwrap();
        ws.write_string(0, 1, "分数").unwrap();
        ws.write_string(1, 0, "张三").unwrap();
        ws.write_number(1, 1, 95.0).unwrap();
        wb.save(&path).unwrap();

        let text = extract_text(&path, AttachmentKind::Xlsx).unwrap().unwrap();
        assert!(text.contains("# Sheet: 成绩"), "got: {text}");
        assert!(text.contains("姓名"));
        assert!(text.contains("张三"));
        assert!(text.contains("95"));
    }

    /// 程序化构造一个带正确 xref 表的最小 PDF(内容流输出 "Hello PDF")。
    fn minimal_pdf_bytes() -> Vec<u8> {
        let objs: Vec<String> = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".into(),
            {
                let stream = "BT /F1 24 Tf 100 700 Td (Hello PDF) Tj ET";
                format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len())
            }
            ,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        ];
        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (i, body) in objs.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{}\nendobj\n", i + 1, body));
        }
        let xref_pos = pdf.len();
        pdf.push_str(&format!("xref\n0 {}\n", objs.len() + 1));
        pdf.push_str("0000000000 65535 f \n");
        for off in &offsets {
            pdf.push_str(&format!("{:010} 00000 n \n", off));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
            objs.len() + 1,
            xref_pos
        ));
        pdf.into_bytes()
    }

    #[test]
    fn pdf_extract_text() {
        let dir = tmp_dir("pdf");
        let path = dir.join("hello.pdf");
        std::fs::write(&path, minimal_pdf_bytes()).unwrap();

        let text = extract_text(&path, AttachmentKind::Pdf).unwrap().unwrap();
        assert!(text.contains("Hello PDF"), "got: {text}");
    }

    #[test]
    fn html_extract_strips_tags() {
        let dir = tmp_dir("html");
        let path = dir.join("page.html");
        std::fs::write(
            &path,
            "<html><head><title>标题</title></head><body><h1>主标题</h1><p>正文段落</p></body></html>",
        )
        .unwrap();
        let text = extract_text(&path, AttachmentKind::Html).unwrap().unwrap();
        assert!(text.contains("标题"), "got: {text}");
        assert!(text.contains("主标题"), "got: {text}");
        assert!(text.contains("正文段落"), "got: {text}");
        assert!(!text.contains("<html>"), "got: {text}");
        assert!(!text.contains("<script>"), "got: {text}");
    }

    #[test]
    fn image_and_other_not_extracted() {
        let dir = tmp_dir("img");
        let path = dir.join("a.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\n").unwrap();
        assert_eq!(extract_text(&path, AttachmentKind::Image).unwrap(), None);
        assert_eq!(extract_text(&path, AttachmentKind::Other).unwrap(), None);
    }
}
