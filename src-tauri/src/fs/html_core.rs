//! HTML 轻量处理内核(§2.3 去重)。
//!
//! 只共享「解析规则」——标签名、属性值、常见实体解码、整段丢弃(script/style 等);
//! 输出策略由各调用方保留:
//!   - `fs::formats::strip_tags`:纯文本抽取(压成一行)
//!   - `websearch::page::markdown`:结构化转 Markdown(保留段落/标题/列表)
//!
//! 不引入完整 HTML 解析器:这里只做够用的确定性遍历。

/// 解码常见 HTML 实体。`&amp;` 必须最后处理,否则会把其它实体的 `&` 再解一次。
pub(crate) fn unescape(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// 从标签内部文本(不含 `<` `>`)解析出小写标签名。
pub(crate) fn tag_name(raw: &str) -> String {
    raw.trim()
        .split(|c: char| c.is_whitespace() || c == '/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// 读取标签属性值(仅支持带引号或空格分隔的简单形式)。
pub(crate) fn attr_value(raw: &str, name: &str) -> Option<String> {
    let lower = raw.to_ascii_lowercase();
    let mut from = 0usize;
    while let Some(p) = lower[from..].find(name) {
        let idx = from + p;
        let before_ok = idx == 0 || lower.as_bytes()[idx - 1].is_ascii_whitespace();
        let rest = raw[idx + name.len()..].trim_start();
        if before_ok && let Some(after_eq) = rest.strip_prefix('=') {
            let v = after_eq.trim_start();
            let (quote, body) = match v.chars().next() {
                Some(q @ ('"' | '\'')) => (Some(q), &v[1..]),
                _ => (None, v),
            };
            let end = match quote {
                Some(q) => body.find(q).unwrap_or(body.len()),
                None => body
                    .find(|c: char| c.is_whitespace() || c == '>')
                    .unwrap_or(body.len()),
            };
            return Some(body[..end].trim().to_string());
        }
        from = idx + name.len();
    }
    None
}

/// 跳过一整段 `<block ...> ... </block>`。
///
/// `from` 指向开标签的 `<`;返回闭合标签 `>` 之后的下标。
/// 找不到闭合标签时返回 None,由调用方决定兜底(通常是丢弃到文末)。
pub(crate) fn skip_block(lower: &str, from: usize, block: &str) -> Option<usize> {
    let close = format!("</{block}");
    let start = from + lower[from..].find(&close)?;
    Some(
        lower[start..]
            .find('>')
            .map(|g| start + g + 1)
            .unwrap_or(lower.len()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_handles_common_entities() {
        assert_eq!(
            unescape("a &amp; b &lt;c&gt; &#39;q&#x27; &quot;w&quot;&nbsp;z"),
            "a & b <c> 'q' \"w\" z"
        );
    }

    #[test]
    fn tag_name_and_attr_value_parse_simple_tags() {
        assert_eq!(tag_name("A href=\"x\""), "a");
        assert_eq!(tag_name("br/"), "br");
        assert_eq!(attr_value("a href=\"https://x/y\"", "href").as_deref(), Some("https://x/y"));
        // 属性名必须独立成词,不能命中 `data-href`
        assert_eq!(attr_value("a data-href=\"x\"", "href"), None);
        assert_eq!(attr_value("img src=logo.png", "src").as_deref(), Some("logo.png"));
    }

    #[test]
    fn skip_block_returns_index_after_closing_tag() {
        let lower = "<script>alert(1)</script><p>正文</p>";
        assert_eq!(skip_block(lower, 0, "script"), Some(25));
        assert_eq!(&lower[25..], "<p>正文</p>");
        // 无闭合标签 → None,交给调用方兜底
        assert_eq!(skip_block("<script>alert(1)", 0, "script"), None);
    }
}
