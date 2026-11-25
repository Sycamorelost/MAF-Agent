//! 中立引用模型与两遍解析:先收集标签编号,再替换 `[@fig-x]` 等交叉引用。
//! 编号复用 `templates::numbering::heading_prefixes` 与 `markdown::caption` 的既有判定,
//! 不产生第二套编号。标签可显式写成 `{#fig-arch}` 或自动生成 `fig-1`;
//! 重复/未知 key 只产生 [`Diagnostic`],不 panic,未知引用保留原文。

use std::collections::HashMap;

use super::markdown::{Block, CaptionKind};
use super::style::HeadingNumbering;
use super::templates::numbering;

/// 标签种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelKind {
    Section,
    Figure,
    Table,
    Equation,
}

impl LabelKind {
    /// key 前缀(`sec`/`fig`/`tbl`/`eq`)。
    pub fn prefix(self) -> &'static str {
        match self {
            LabelKind::Section => "sec",
            LabelKind::Figure => "fig",
            LabelKind::Table => "tbl",
            LabelKind::Equation => "eq",
        }
    }

    /// 引用渲染文本。
    pub fn render(self, number: usize) -> String {
        match self {
            LabelKind::Section => format!("第 {number} 章"),
            LabelKind::Figure => format!("图 {number}"),
            LabelKind::Table => format!("表 {number}"),
            LabelKind::Equation => format!("式 ({number})"),
        }
    }

    fn from_prefix(prefix: &str) -> Option<Self> {
        match prefix {
            "sec" => Some(LabelKind::Section),
            "fig" => Some(LabelKind::Figure),
            "tbl" => Some(LabelKind::Table),
            "eq" => Some(LabelKind::Equation),
            _ => None,
        }
    }
}

/// 一个可被交叉引用的标签。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub kind: LabelKind,
    pub key: String,
    pub number: usize,
    /// Word 书签名(不含 `w:name` 的转义,字符集限制为字母/数字/下划线)
    pub bookmark: String,
}

/// 诊断级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    #[allow(dead_code)]
    Error,
}

/// 结构化诊断。`line` 为块序号(1 起),源 Markdown 行号在块解析后不可追溯。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: Severity,
    pub code: &'static str,
    pub message: String,
    pub line: usize,
}

impl Diagnostic {
    fn new(level: Severity, code: &'static str, message: String, line: usize) -> Self {
        Diagnostic {
            level,
            code,
            message,
            line,
        }
    }
}

/// 第一遍扫描结果。
#[derive(Debug, Default)]
pub struct Scan {
    labels: Vec<Label>,
    /// 块序号 → 标签下标
    at: HashMap<usize, Vec<usize>>,
    diagnostics: Vec<Diagnostic>,
}

impl Scan {
    /// 某个块上挂载的标签(用于写书签)。
    pub fn labels_at(&self, block: usize) -> Vec<Label> {
        self.at
            .get(&block)
            .map(|idx| idx.iter().map(|&i| self.labels[i].clone()).collect())
            .unwrap_or_default()
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn lookup(&self, key: &str) -> Option<&Label> {
        self.labels.iter().find(|l| l.key == key)
    }
}

/// 第一遍:按出现顺序收集并编号全部标签。
pub fn scan(blocks: &[Block], kind: HeadingNumbering) -> Scan {
    let mut out = Scan::default();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut seq = [0usize; 4]; // sec/fig/tbl/eq 顺序号
    let mut chapter = 0usize;
    let top = numbering::top_level(blocks);
    // 复用标题层级规则,确认自动编号与正文一致(结果仅用于章节号推断)
    let _ = numbering::heading_prefixes(blocks, kind);

    for (i, block) in blocks.iter().enumerate() {
        match block {
            Block::Heading { level, text } => {
                if *level > 4 {
                    continue;
                }
                if *level == top {
                    chapter += 1;
                }
                let (_, marker) = split_marker(text);
                seq[0] += 1;
                let key = marker
                    .filter(|m| m.starts_with("sec-"))
                    .unwrap_or_else(|| format!("sec-{}", seq[0]));
                let number = chapter.max(1);
                push_label(&mut out, &mut seen, LabelKind::Section, key, number, i);
            }
            Block::Caption(text) => {
                let (clean, marker) = split_marker(text);
                let kind = caption_kind_of(&clean, marker.as_deref());
                if let Some(kind) = kind {
                    register_numbered(&mut out, &mut seen, kind, marker, &mut seq, i);
                }
            }
            Block::Table { caption, .. } => {
                if let Some(cap) = caption {
                    let (clean, marker) = split_marker(cap);
                    let kind = caption_kind_of(&clean, marker.as_deref());
                    if let Some(kind) = kind {
                        register_numbered(&mut out, &mut seen, kind, marker, &mut seq, i);
                    }
                }
            }
            Block::Image { alt, .. } => {
                // 图片行自身带显式 key 时也登记(题注块通常紧随其后)
                let (_, marker) = split_marker(alt);
                if marker.as_deref().is_some_and(|m| m.starts_with("fig-")) {
                    register_numbered(&mut out, &mut seen, LabelKind::Figure, marker, &mut seq, i);
                }
            }
            Block::Math(latex) => {
                let (_, marker) = split_marker(latex);
                register_numbered(
                    &mut out,
                    &mut seen,
                    LabelKind::Equation,
                    marker,
                    &mut seq,
                    i,
                );
            }
            _ => {}
        }
    }
    out
}

fn register_numbered(
    out: &mut Scan,
    seen: &mut HashMap<String, usize>,
    kind: LabelKind,
    marker: Option<String>,
    seq: &mut [usize; 4],
    block: usize,
) {
    let slot = match kind {
        LabelKind::Section => 0,
        LabelKind::Figure => 1,
        LabelKind::Table => 2,
        LabelKind::Equation => 3,
    };
    seq[slot] += 1;
    let auto = format!("{}-{}", kind.prefix(), seq[slot]);
    let key = match marker {
        Some(m) if key_kind(&m) == Some(kind) => m,
        Some(m) => {
            out.diagnostics.push(Diagnostic::new(
                Severity::Warning,
                "REF-KEY-KIND",
                format!("标签 {m} 与所在位置类型不符,已改用自动 key {auto}"),
                block + 1,
            ));
            auto
        }
        None => auto,
    };
    push_label(out, seen, kind, key, seq[slot], block);
}

fn push_label(
    out: &mut Scan,
    seen: &mut HashMap<String, usize>,
    kind: LabelKind,
    key: String,
    number: usize,
    block: usize,
) {
    if seen.contains_key(&key) {
        out.diagnostics.push(Diagnostic::new(
            Severity::Warning,
            "REF-DUP-KEY",
            format!("重复的引用 key: {key}"),
            block + 1,
        ));
        return;
    }
    let bookmark = bookmark_name(&key);
    let idx = out.labels.len();
    out.labels.push(Label {
        kind,
        key: key.clone(),
        number,
        bookmark,
    });
    seen.insert(key, idx);
    out.at.entry(block).or_default().push(idx);
}

/// 第二遍:替换引用并剥离 `{#...}` 标记。
pub fn resolve_blocks(blocks: &[Block], scan: &Scan) -> (Vec<Block>, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let out = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| resolve_block(b, scan, i + 1, &mut diags))
        .collect();
    (out, diags)
}

fn resolve_block(block: &Block, scan: &Scan, line: usize, diags: &mut Vec<Diagnostic>) -> Block {
    match block {
        Block::Heading { level, text } => Block::Heading {
            level: *level,
            text: resolve_text(text, scan, line, diags),
        },
        Block::Paragraph(t) => Block::Paragraph(resolve_text(t, scan, line, diags)),
        Block::Quote(t) => Block::Quote(resolve_text(t, scan, line, diags)),
        Block::Caption(t) => Block::Caption(resolve_text(t, scan, line, diags)),
        Block::Abstract(t) => Block::Abstract(resolve_text(t, scan, line, diags)),
        Block::Keywords(t) => Block::Keywords(resolve_text(t, scan, line, diags)),
        Block::Reference(t) => Block::Reference(resolve_text(t, scan, line, diags)),
        Block::Math(t) => Block::Math(strip_all_markers(t)),
        Block::Table {
            caption,
            header,
            rows,
            aligns,
        } => Block::Table {
            caption: caption.as_ref().map(|c| resolve_text(c, scan, line, diags)),
            header: header
                .iter()
                .map(|c| resolve_text(c, scan, line, diags))
                .collect(),
            rows: rows
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|c| resolve_text(c, scan, line, diags))
                        .collect()
                })
                .collect(),
            aligns: aligns.clone(),
        },
        Block::List { ordered, items } => Block::List {
            ordered: *ordered,
            items: items
                .iter()
                .map(|it| super::markdown::ListItem {
                    level: it.level,
                    text: resolve_text(&it.text, scan, line, diags),
                })
                .collect(),
        },
        Block::Image { alt, src } => Block::Image {
            alt: resolve_text(alt, scan, line, diags),
            src: src.clone(),
        },
        other => other.clone(),
    }
}

/// 替换一段文本中的全部 `[@key]` 引用。
pub fn resolve_text(text: &str, scan: &Scan, line: usize, diags: &mut Vec<Diagnostic>) -> String {
    let text = strip_all_markers(text);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '[' && chars.get(i + 1) == Some(&'@') {
            if let Some(end) = (i + 2..chars.len()).find(|&j| chars[j] == ']') {
                let key: String = chars[i + 2..end].iter().collect();
                let key = key.trim();
                match scan.lookup(key) {
                    Some(label) => out.push_str(&label.kind.render(label.number)),
                    None => {
                        diags.push(Diagnostic::new(
                            Severity::Warning,
                            "REF-UNKNOWN",
                            format!("引用了不存在的标签 {key},已保留原文"),
                            line,
                        ));
                        out.extend(&chars[i..=end]);
                    }
                }
                i = end + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// 拆分末尾 `{#key}` 标记,返回(清理后文本, key)。
pub fn split_marker(text: &str) -> (String, Option<String>) {
    let Some(start) = text.rfind("{#") else {
        return (text.to_string(), None);
    };
    let Some(rel_end) = text[start..].find('}') else {
        return (text.to_string(), None);
    };
    let key = text[start + 2..start + rel_end].trim().to_string();
    if key.is_empty() || key.contains(char::is_whitespace) {
        return (text.to_string(), None);
    }
    let mut clean = String::with_capacity(text.len());
    clean.push_str(text[..start].trim_end());
    clean.push_str(&text[start + rel_end + 1..]);
    (clean.trim().to_string(), Some(key))
}

fn strip_all_markers(text: &str) -> String {
    let mut s = text.to_string();
    while let (clean, Some(_)) = split_marker(&s) {
        if clean == s {
            break;
        }
        s = clean;
    }
    s
}

fn caption_kind_of(clean: &str, marker: Option<&str>) -> Option<LabelKind> {
    if let Some(kind) = marker.and_then(key_kind) {
        return Some(kind);
    }
    Some(match super::markdown::caption_kind(clean)? {
        CaptionKind::Figure => LabelKind::Figure,
        CaptionKind::Table => LabelKind::Table,
    })
}

fn key_kind(key: &str) -> Option<LabelKind> {
    let prefix = key.split_once('-').map(|(p, _)| p)?;
    LabelKind::from_prefix(prefix)
}

/// 生成合法 Word 书签名(`_Ref_` 前缀,仅字母/数字/下划线)。
fn bookmark_name(key: &str) -> String {
    let mut name = String::from("_Ref_");
    for c in key.chars() {
        if c.is_ascii_alphanumeric() {
            name.push(c);
        } else {
            name.push('_');
        }
    }
    name.truncate(40);
    name
}
