//! 题注与专用标签识别:图/表题注、摘要、关键词、参考文献标题、图片行。

use super::Block;

pub(super) fn is_caption(t: &str) -> bool {
    let probe = t.trim_start_matches(['*', '_', ' ']).trim_start();
    for prefix in ["表", "图"] {
        if let Some(rest) = probe.strip_prefix(prefix)
            && rest
                .trim_start()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
        {
            return true;
        }
    }
    for prefix in ["Table", "Figure", "Fig."] {
        if let Some(rest) = probe.strip_prefix(prefix) {
            let rest = rest.trim_start();
            if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

/// 「摘要：…」/「**关键词**：…」识别为专用段落(仅认冒号形式,避免误吞普通句子)。
pub(super) fn labeled_block(t: &str) -> Option<Block> {
    let idx = t.find(['：', ':'])?;
    let (label, rest) = t.split_at(idx);
    let mut chars = rest.chars();
    chars.next();
    let text = chars.as_str().trim().to_string();
    let norm: String = label
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '*' | '_' | '#' | '【' | '】' | '[' | ']'))
        .collect::<String>()
        .to_ascii_lowercase();
    match norm.as_str() {
        "摘要" | "abstract" => Some(Block::Abstract(text)),
        "关键词" | "关键字" | "keywords" | "keyword" => Some(Block::Keywords(text)),
        _ => None,
    }
}

pub(super) fn is_reference_heading(text: &str) -> bool {
    let norm: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '*' | '_' | '#'))
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(
        norm.as_str(),
        "参考文献" | "引用文献" | "references" | "bibliography"
    )
}

pub(super) fn image_line(t: &str) -> Option<(String, String)> {
    let rest = t.strip_prefix("![")?;
    let close = rest.find("](")?;
    let alt = rest[..close].to_string();
    let after = &rest[close + 2..];
    let end = after.find(')')?;
    Some((alt, after[..end].to_string()))
}
