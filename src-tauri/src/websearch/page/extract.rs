//! 正文区域裁剪:先去站点模板,再删「纯链接行」。
//!
//! 不做完整 DOM 解析,只做两件确定性的事:
//! 1. `main_html`:丢掉 nav/aside/footer/form 等语义容器,以及 class/id 命中
//!    导航、侧栏、评论、推荐、分享、登录等关键词的块级容器(按同名标签嵌套计数整段跳过);
//! 2. `prune_link_lines`:Markdown 转好后,删掉「几乎全是链接」的行(菜单、相关推荐、
//!    分页),避免它们占满返回预算。
//!
//! 模板识别一律宁少勿多:正文里偶有链接/小标题,但整块丢弃只发生在明确的容器上。

use crate::fs::html_core::{attr_value, tag_name};

/// 整段丢弃的语义标签(正文不会用这些标签承载内容)。
const DROP_TAGS: [&str; 9] = [
    "nav", "aside", "footer", "form", "dialog", "iframe", "canvas", "noscript", "button",
];

/// class/id 命中这些词的块级容器视为模板。按子串匹配,覆盖 site-nav / nav-menu 等变体。
const DROP_ATTR_WORDS: [&str; 26] = [
    "nav",
    "menu",
    "sidebar",
    "footer",
    "breadcrumb",
    "share",
    "social",
    "comment",
    "related",
    "recommend",
    "copyright",
    "cookie",
    "banner",
    "advert",
    "promo",
    "login",
    "signin",
    "subscribe",
    "newsletter",
    "popup",
    "modal",
    "toolbar",
    "pagination",
    "pager",
    "site-header",
    "masthead",
];

/// 只对这些块级容器做 class/id 黑名单判定,避免误删正文里的行内元素。
const CONTAINER_TAGS: [&str; 9] = [
    "div", "section", "ul", "ol", "table", "dl", "header", "main", "article",
];

/// 自闭合/空元素,没有闭合标签,不参与整段跳过。
const VOID_TAGS: [&str; 13] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track",
];

/// 裁掉站点模板,返回更干净的 HTML(仍交给 `markdown` 继续转换)。
pub(super) fn main_html(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut i = 0usize;
    while i < html.len() {
        let Some(rel) = html[i..].find('<') else {
            out.push_str(&html[i..]);
            break;
        };
        let lt = i + rel;
        out.push_str(&html[i..lt]);
        if html[lt..].starts_with("<!--") {
            match lower[lt..].find("-->") {
                Some(e) => i = lt + e + 3,
                None => break,
            }
            continue;
        }
        let Some(gt_rel) = lower[lt..].find('>') else {
            break;
        };
        let gt = lt + gt_rel;
        let raw = &html[lt + 1..gt];
        let raw_lower = raw.trim_start();
        let closing = raw_lower.starts_with('/');
        let self_closing = raw.trim_end().ends_with('/');
        let name = tag_name(raw_lower.strip_prefix('/').unwrap_or(raw_lower));

        if !closing
            && !self_closing
            && !VOID_TAGS.contains(&name.as_str())
            && should_drop(&name, raw)
        {
            i = skip_element(&lower, lt, &name);
            continue;
        }
        out.push_str(&html[lt..=gt]);
        i = gt + 1;
    }
    out
}

/// 该开标签是否代表模板容器。
fn should_drop(name: &str, raw: &str) -> bool {
    if DROP_TAGS.contains(&name) {
        return true;
    }
    if !CONTAINER_TAGS.contains(&name) {
        return false;
    }
    let attrs = format!(
        "{} {}",
        attr_value(raw, "class").unwrap_or_default(),
        attr_value(raw, "id").unwrap_or_default()
    )
    .to_ascii_lowercase();
    attrs
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .filter(|w| w.len() >= 3)
        .any(|w| DROP_ATTR_WORDS.iter().any(|k| w.contains(k)))
}

/// 跳过整段同名元素:`from` 指向开标签的 `<`,返回闭合标签 `>` 之后的下标。
/// 与 `html_core::skip_block` 不同,这里统计同名标签嵌套层数(div 里套 div 很常见),
/// 且标签名后必须是空白 / `>` / `/`,避免 `<nav` 误命中 `<navbar`。
fn skip_element(lower: &str, from: usize, name: &str) -> usize {
    let mut depth = 0usize;
    let mut i = from;
    while i < lower.len() {
        let open = find_tag(lower, i, &format!("<{name}"));
        let close = find_tag(lower, i, &format!("</{name}"));
        match (open, close) {
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                i = o + name.len() + 1;
            }
            (_, Some(c)) => {
                depth = depth.saturating_sub(1);
                let end = lower[c..]
                    .find('>')
                    .map(|g| c + g + 1)
                    .unwrap_or(lower.len());
                if depth == 0 {
                    return end;
                }
                i = end;
            }
            (Some(o), None) => {
                depth += 1;
                i = o + name.len() + 1;
            }
            (None, None) => return lower.len(),
        }
    }
    lower.len()
}

/// 从 `from` 起查找标签前缀,要求标签名后是空白、`>`、`/` 或结尾。
fn find_tag(lower: &str, from: usize, pat: &str) -> Option<usize> {
    let mut i = from;
    while let Some(p) = lower[i..].find(pat) {
        let idx = i + p;
        let after = lower.as_bytes().get(idx + pat.len()).copied();
        if matches!(
            after,
            Some(b' ')
                | Some(b'\t')
                | Some(b'\n')
                | Some(b'\r')
                | Some(b'>')
                | Some(b'/')
                | Some(b'=')
                | None
        ) {
            return Some(idx);
        }
        i = idx + pat.len();
    }
    None
}

/// 删除「几乎全是链接」的行(导航菜单、相关推荐、分页)。
///
/// 判定:链接部分占该行字符 60% 以上、整行不超过 300 字符。
/// 代码块(``` 围栏)、标题、空行一律保留。
pub(super) fn prune_link_lines(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut in_code = false;
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_code || trimmed.is_empty() || trimmed.starts_with('#') {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if is_link_heavy(line) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim_end().to_string()
}

/// 行内 Markdown 链接 `[标签](地址)` 占整行字符的比例是否超过 60%。
fn is_link_heavy(line: &str) -> bool {
    let total = line.chars().count();
    if total == 0 || total > 300 {
        return false;
    }
    let mut link_chars = 0usize;
    let mut rest = line;
    while let Some(lb) = rest.find('[') {
        let Some(close) = rest[lb..].find("](").map(|p| lb + p) else {
            break;
        };
        let Some(rp) = rest[close..].find(')').map(|p| close + p) else {
            break;
        };
        link_chars += rest[lb..=rp].chars().count();
        rest = &rest[rp + 1..];
    }
    link_chars > 0 && (link_chars as f32 / total as f32) > 0.6
}

/// 参考文献 / 外部链接这类尾部章节标题(小写比较)。
const REF_TITLES: [&str; 12] = [
    "references",
    "reference",
    "external links",
    "see also",
    "bibliography",
    "citations",
    "参考文献",
    "参考链接",
    "外部链接",
    "扩展阅读",
    "延伸阅读",
    "相关链接",
];

/// 丢掉「参考文献 / 外部链接」整节:命中标题后跳到下一个同级标题。
pub(super) fn drop_reference_sections(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut skip_level: Option<usize> = None;
    for line in markdown.lines() {
        let trimmed = line.trim();
        if let Some(level) = heading_level(trimmed) {
            if let Some(skip) = skip_level {
                if level > skip {
                    // 仍在被丢弃章节的子标题下
                    continue;
                }
                skip_level = None;
            }
            let title = trimmed.trim_start_matches('#').trim().to_ascii_lowercase();
            if REF_TITLES.iter().any(|k| title.starts_with(k)) {
                skip_level = Some(level);
                continue;
            }
        } else if skip_level.is_some() {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim_end().to_string()
}

fn heading_level(line: &str) -> Option<usize> {
    let n = line.chars().take_while(|c| *c == '#').count();
    (n > 0 && line.chars().nth(n) == Some(' ')).then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_nav_footer_and_sidebar_but_keeps_article() {
        let html = r#"<html><body>
            <nav class="top"><a href="/">首页</a><a href="/p">产品</a></nav>
            <div class="sidebar"><p>热门推荐</p><p>广告</p></div>
            <article><h1>标题</h1><p>这是正文段落,需要保留。</p><div class="comment"><p>评论内容</p></div></article>
            <footer>版权所有</footer>
        </body></html>"#;
        let out = main_html(html);
        assert!(out.contains("这是正文段落"));
        assert!(!out.contains("热门推荐"));
        assert!(!out.contains("评论内容"));
        assert!(!out.contains("版权所有"));
    }

    #[test]
    fn nested_containers_are_skipped_to_the_right_closing_tag() {
        let html = r#"<div class="sidebar"><div><p>导航</p></div><p>更多</p></div><p>正文</p>"#;
        let out = main_html(html);
        assert_eq!(out.trim(), "<p>正文</p>");
    }

    #[test]
    fn link_only_menu_lines_are_removed_and_normal_links_kept() {
        let md = "[首页](/) [产品](/p) [文档](/d) [价格](/price)\n正文里提到 [官方文档](https://x) 可以解决问题。";
        let out = prune_link_lines(md);
        assert!(!out.contains("[首页]"));
        assert!(out.contains("正文里提到"));
    }

    #[test]
    fn reference_sections_are_dropped_until_next_heading() {
        let md = "# 正文\n要保留的内容。\n\n## References\n- [1](https://a) 引用一\n- [2](https://b) 引用二\n\n## 后续章节\n这一段要保留。";
        let out = drop_reference_sections(md);
        assert!(out.contains("要保留的内容"));
        assert!(out.contains("这一段要保留"));
        assert!(!out.contains("引用一"));
        assert!(!out.contains("## References"));
    }
}
