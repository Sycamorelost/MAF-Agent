//! 文档分块、摘要卡与内容指纹(纯算法,不依赖数据库)。

use serde::Serialize;
use sha2::{Digest, Sha256};

/// 单块目标字符数(不含空白)。
pub const CHUNK_TARGET: usize = 1000;

// ============ 分块 ============

/// 一个文本块:页码(PDF/其它分页来源)与所属章节可为空。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocChunk {
    pub page: Option<u32>,
    pub section: String,
    pub content: String,
}

/// `【第 N 页】` 标记解析(纯文本层 PDF 由 `attachment::extract_pdf` 逐页拼出)。
fn parse_page_marker(line: &str) -> Option<u32> {
    let t = line.trim();
    let rest = t.strip_prefix("【第")?;
    let num = rest.strip_suffix("页】")?.trim();
    num.parse::<u32>().ok()
}

/// 形如章节标题的行(标题行不入正文块,只更新后续块的 section 元数据)。
fn is_heading(line: &str) -> bool {
    let t = line.trim();
    let n = t.chars().count();
    if n == 0 || n > 40 {
        return false;
    }
    if t.starts_with('#') {
        return true;
    }
    if t.starts_with("第") && (t.contains('章') || t.contains('节')) {
        return true;
    }
    const CN: [char; 10] = ['一', '二', '三', '四', '五', '六', '七', '八', '九', '十'];
    let mut it = t.chars();
    matches!(it.next(), Some(c) if CN.contains(&c)) && it.next() == Some('、')
}

fn clean_heading(line: &str) -> String {
    line.trim().trim_start_matches('#').trim().to_string()
}

fn flush(out: &mut Vec<DocChunk>, buf: &mut String, page: Option<u32>, section: &str) {
    let content = buf.trim();
    if !content.is_empty() {
        out.push(DocChunk {
            page,
            section: section.to_string(),
            content: content.to_string(),
        });
    }
    buf.clear();
}

/// 按行切块:遇到页码标记/章节标题即断开,正文累积到 `CHUNK_TARGET` 后换块。
pub fn chunk_text(text: &str) -> Vec<DocChunk> {
    let mut out: Vec<DocChunk> = Vec::new();
    let mut page: Option<u32> = None;
    let mut section = String::new();
    let mut buf = String::new();
    for raw in text.lines() {
        let line = raw.trim_end();
        if let Some(p) = parse_page_marker(line) {
            flush(&mut out, &mut buf, page, &section);
            page = Some(p);
            section.clear();
            continue;
        }
        if is_heading(line) {
            flush(&mut out, &mut buf, page, &section);
            section = clean_heading(line);
            continue;
        }
        if line.trim().is_empty() {
            if !buf.is_empty() {
                buf.push('\n');
            }
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
        if buf.chars().filter(|c| !c.is_whitespace()).count() >= CHUNK_TARGET {
            flush(&mut out, &mut buf, page, &section);
        }
    }
    flush(&mut out, &mut buf, page, &section);
    out
}

/// 文档摘要卡:开头正文 + 章节骨架 + 规模,替代「只取前 200 字」。
/// 卡片随每轮对话注入,必须在几百字符内让人/模型判断「这份文档讲什么、去哪找」。
pub fn summary(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let head: String = flat.chars().take(320).collect();
    // 章节骨架:最多 12 个标题,去重、跳过「第 N 页」这类页码标记
    let mut sections: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if !(t.starts_with("## ") || t.starts_with("# ") || t.starts_with("### ")) {
            continue;
        }
        let title = t.trim_start_matches('#').trim();
        if title.is_empty() || title.starts_with("第 ") {
            continue;
        }
        if !sections.iter().any(|s| s == title) {
            sections.push(title.to_string());
        }
        if sections.len() >= 12 {
            break;
        }
    }
    let chars = text.chars().count();
    let mut out = head;
    if chars > 320 {
        out.push('…');
    }
    out.push_str(&format!("\n(全文约 {chars} 字符"));
    if !sections.is_empty() {
        out.push_str(&format!(";章节:{})", sections.join(" / ")));
    } else {
        out.push(')');
    }
    out
}

/// 文档指纹:正文内容的 sha256 前 12 位。
/// 用内容而不是路径,同一文件重复上传(哪怕落盘路径不同)也视为同一文档,
/// 不会重复建块(R14)。
pub fn doc_id_of(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    hex[..12].to_string()
}
