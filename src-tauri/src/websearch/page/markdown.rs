//! 轻量 HTML → Markdown:保留段落/标题/列表/链接,丢弃脚本与样式。
//!
//! 不引入完整 HTML 解析器:遍历标签 + 实体解码即可满足「读正文」的目标,
//! 输出经 `tidy` 压空白后交给 `render` 标注为不可信内容。
//! 标签名 / 属性 / 实体解码 / 整段丢弃规则与 `fs::formats::strip_tags` 共用
//! `fs::html_core`,避免两份解析规则各自漂移。

use crate::fs::html_core::{attr_value, skip_block, tag_name, unescape};

/// 抽取 `<title>` 文本。
pub(super) fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let gt = lower[start..].find('>')? + start;
    let end = lower[gt..].find("</title>")? + gt;
    let title = tidy(&unescape(&html[gt + 1..end]));
    (!title.is_empty()).then_some(title)
}

/// 轻量 HTML → Markdown:保留段落/标题/列表/链接,丢弃脚本与样式。
pub(super) fn html_to_markdown(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len().min(64 * 1024));
    let mut anchors: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < html.len() {
        let rest = &html[i..];
        if rest.starts_with('<') {
            if rest.starts_with("<!--")
                && let Some(end) = lower[i..].find("-->")
            {
                i += end + 3;
                continue;
            }
            let Some(end) = lower[i..].find('>') else {
                break;
            };
            let raw = &html[i + 1..i + end];
            let raw_lower = &lower[i + 1..i + end];
            i += end + 1;

            if raw_lower.starts_with('/') {
                let name = tag_name(raw_lower.strip_prefix('/').unwrap_or(raw_lower));
                match name.as_str() {
                    "p" | "div" | "section" | "article" | "header" | "footer" | "main" | "tr"
                    | "blockquote" | "pre" | "table" | "ul" | "ol" | "dl" | "h1" | "h2" | "h3"
                    | "h4" | "h5" | "h6" => push_newlines(&mut out, 2),
                    "a" => {
                        if let Some(href) = anchors.pop()
                            && !href.is_empty()
                        {
                            out.push_str(&format!("]({href})"));
                        }
                    }
                    _ => {}
                }
                continue;
            }

            let self_closing = raw_lower.trim_end().ends_with('/');
            let name = tag_name(raw_lower);
            match name.as_str() {
                // head/title(标题单独抽取)/script/style/noscript/template 整段丢弃
                "head" | "title" | "script" | "style" | "noscript" | "template" => {
                    i = skip_block(&lower, i, &name).unwrap_or(lower.len());
                }
                "br" => out.push('\n'),
                "p" | "div" | "section" | "article" | "header" | "footer" | "main" | "tr"
                | "blockquote" | "pre" | "table" | "ul" | "ol" | "dl" => {
                    push_newlines(&mut out, 2)
                }
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    let level = name[1..].parse::<usize>().unwrap_or(1).min(6);
                    push_newlines(&mut out, 2);
                    out.push_str(&"#".repeat(level));
                    out.push(' ');
                }
                "li" => {
                    push_newlines(&mut out, 1);
                    out.push_str("- ");
                }
                "td" | "th" => out.push_str(" | "),
                "a" if !self_closing => {
                    anchors.push(attr_value(raw, "href").unwrap_or_default());
                    out.push('[');
                }
                _ => {}
            }
            continue;
        }
        let c = rest.chars().next().unwrap_or(' ');
        out.push(c);
        i += c.len_utf8();
    }
    tidy(&unescape(&out))
}

/// 保证输出至少 n 个连续换行,避免重复插入。
fn push_newlines(out: &mut String, n: usize) {
    let existing = out.chars().rev().take_while(|c| *c == '\n').count();
    for _ in existing..n {
        out.push('\n');
    }
}

/// 压缩行内空白并保留段落分隔(最多一个空行)。
fn tidy(text: &str) -> String {
    let normalized = text.replace('\u{00a0}', " ");
    let mut lines: Vec<String> = Vec::new();
    for line in normalized.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            if lines.last().is_some_and(|l| l.is_empty()) {
                continue;
            }
            lines.push(String::new());
        } else {
            lines.push(line);
        }
    }
    lines.join("\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_converts_to_markdown_with_structure() {
        let html = r#"<html><head><title>测试 &amp; 标题</title><style>.a{}</style></head>
<body><h1>主标题</h1><p>第一段 <a href="https://example.com/x">链接</a>。</p>
<script>alert(1)</script>
<ul><li>条目一</li><li>条目二</li></ul></body></html>"#;
        let md = html_to_markdown(html);
        assert_eq!(extract_title(html).as_deref(), Some("测试 & 标题"));
        assert!(md.starts_with("# 主标题"), "实际:{md}");
        assert!(md.contains("[链接](https://example.com/x)"));
        assert!(md.contains("- 条目一"));
        assert!(!md.contains("alert(1)"), "脚本应被丢弃");
        assert!(!md.contains(".a{}"), "样式应被丢弃");
    }
}
