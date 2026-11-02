//! 抓取主流程:URL 校验(协议 + 私网)、手动逐跳重定向、限长读取与解码。

use std::time::Duration;

use super::chunk;
use super::extract;
use super::markdown::{extract_title, html_to_markdown};
use super::robots::robots_allows;
use super::Page;

/// 默认返回正文长度;上限防止单次抓取撑爆上下文。
const DEFAULT_MAX_CHARS: usize = 12_000;
const MAX_CHARS_CAP: usize = 100_000;
/// 响应体上限(正常网页远小于此值)。
const MAX_BYTES: usize = 10 * 1024 * 1024;
const TIMEOUT_SECS: u64 = 30;
const MAX_HOPS: u32 = 5;

/// 浏览器 UA:不少站点对非浏览器 UA 直接返回 403,这里只用于取正文。
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

/// 抓取网页正文。`max_chars` 省略时用默认值,超出上限自动收敛。
///
/// - `query`:检索意图。命中时按 BM25 选最相关段落(最相关在前),不再按原文顺序截断;
/// - `start_index`:不传 query(或 query 未命中)时从第 N 个字符开始读,用于续读长页面。
pub async fn fetch_page(
    raw_url: &str,
    max_chars: Option<usize>,
    start_index: Option<usize>,
    query: Option<&str>,
) -> Result<Page, String> {
    let url = validate_url(raw_url)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(10))
        // 与搜索链路一致:禁止自动跟随,统一手动逐跳复检
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(USER_AGENT)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    if !robots_allows(&client, &url).await {
        return Err(format!(
            "目标站点的 robots.txt 禁止抓取该路径(`{}`),已停止",
            url.path()
        ));
    }

    let resp = get_following(&client, url.clone()).await?;
    let final_url = resp.url().clone();
    let status = resp.status();
    if !status.is_success() {
        let hint = match status.as_u16() {
            401 | 403 => "(可能需要登录,或被反爬策略拦截)",
            404 => "(页面不存在)",
            429 => "(请求过于频繁)",
            _ => "",
        };
        return Err(format!("HTTP {}{hint}", status.as_u16()));
    }

    let ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let media = ctype.split(';').next().unwrap_or("").trim().to_string();
    let charset = ctype
        .split("charset=")
        .nth(1)
        .map(|s| s.split(';').next().unwrap_or("").trim().trim_matches('"').to_string());

    let body = read_limited(resp).await?;
    let is_html = match classify(&media, &body) {
        Ok(k) => k == Kind::Html,
        Err(e) => return Err(e),
    };
    let text = decode_body(&body, charset.as_deref());

    let title = if is_html {
        extract_title(&text).unwrap_or_else(|| final_url.host_str().unwrap_or("网页正文").to_string())
    } else {
        final_url.host_str().unwrap_or("网页正文").to_string()
    };
    let content = if is_html {
        webpage_markdown(&text)
    } else {
        text.trim().to_string()
    };
    if content.is_empty() {
        return Err("未提取到正文(页面可能依赖登录或前端渲染)".into());
    }

    let max = max_chars.unwrap_or(DEFAULT_MAX_CHARS).clamp(200, MAX_CHARS_CAP);
    let total_chars = content.chars().count();
    let query = query.map(str::trim).filter(|q| !q.is_empty());
    let mut query_selected = false;

    let (content, offset, truncated) = match query {
        Some(q) => {
            let sel = chunk::select(&content, q, max);
            if sel.matched {
                query_selected = true;
                let returned = sel.text.chars().count();
                (sel.text, 0, total_chars > returned)
            } else {
                slice_from(&content, start_index.unwrap_or(0), max)
            }
        }
        None => slice_from(&content, start_index.unwrap_or(0), max),
    };

    Ok(Page {
        title,
        url: final_url.to_string(),
        content,
        truncated,
        total_chars,
        offset,
        query_selected,
    })
}

/// 把页面 HTML 变成「干净正文」:先去模板容器,再转 Markdown,最后删纯链接行。
fn webpage_markdown(html: &str) -> String {
    let main = extract::main_html(html);
    let source = if main.trim().is_empty() { html } else { &main };
    let md = html_to_markdown(source);
    let pruned = extract::drop_reference_sections(&extract::prune_link_lines(&md));
    if pruned.trim().is_empty() { md } else { pruned }
}

/// 从第 `offset` 个字符起截取 `max` 个字符;返回(内容, 实际起点, 是否还有后续)。
fn slice_from(text: &str, offset: usize, max: usize) -> (String, usize, bool) {
    let total = text.chars().count();
    let start = offset.min(total);
    let body: String = text.chars().skip(start).take(max).collect();
    let truncated = total > start + body.chars().count();
    (body, start, truncated)
}

/// 校验链接:协议只允许 http/https,并拦截本机/私网地址。
fn validate_url(raw: &str) -> Result<reqwest::Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("缺少 url 参数(需完整的 http/https 链接)".into());
    }
    let url = reqwest::Url::parse(trimmed).map_err(|e| format!("链接格式无法解析: {e}"))?;
    match url.scheme() {
        "http" | "https" => {}
        other => return Err(format!("只支持 http/https 链接(收到 `{other}`)")),
    }
    let host = url.host_str().ok_or("链接缺少主机名")?;
    if is_private_host(host) {
        return Err(format!(
            "已拦截本机/内网地址 `{host}`(防 SSRF);如确需读取,请先下载到本地再用 file_extract"
        ));
    }
    Ok(url)
}

/// 本机、私网与云元数据地址判定(IPv4 字面量 + 常见主机名 + IPv6 前缀)。
fn is_private_host(host: &str) -> bool {
    let h = host.trim().to_ascii_lowercase();
    if h.is_empty()
        || h == "localhost"
        || h.ends_with(".local")
        || h.ends_with(".localhost")
    {
        return true;
    }
    if h.contains(':') {
        // IPv6:回环、未指定、链路本地、唯一本地
        return h == "::1"
            || h == "::"
            || h.starts_with("fe80")
            || h.starts_with("fc")
            || h.starts_with("fd");
    }
    let parts: Vec<&str> = h.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    let Ok(octets) = parts
        .iter()
        .map(|p| p.parse::<u8>())
        .collect::<Result<Vec<_>, _>>()
    else {
        return false;
    };
    let (a, b, c) = (octets[0], octets[1], octets[2]);
    a == 0
        || a == 127
        || a == 10
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 169 && b == 254) // 含 169.254.169.254 云元数据
        || (a == 100 && (64..=126).contains(&b)) // 运营商级 NAT
        || (a == 192 && b == 0 && c == 0)
        || a == 255
}

/// 手动跟随重定向:每一跳都重新做协议与私网校验。
async fn get_following(
    client: &reqwest::Client,
    mut url: reqwest::Url,
) -> Result<reqwest::Response, String> {
    for _ in 0..=MAX_HOPS {
        let resp = client
            .get(url.clone())
            .header(reqwest::header::ACCEPT, "text/html,application/json,text/plain;q=0.9,*/*;q=0.5")
            .send()
            .await
            .map_err(|e| format!("请求失败: {e}"))?;
        if !resp.status().is_redirection() {
            return Ok(resp);
        }
        let loc = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or("重定向响应缺少 Location,已拒绝")?;
        url = url
            .join(loc)
            .map_err(|e| format!("重定向地址无法解析: {e}"))?;
        validate_url(url.as_str())?;
    }
    Err("重定向超过 5 跳,已拒绝".into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Html,
    Text,
}

/// 按 content-type 分类;缺头时看前 200 字节是否像 HTML。
fn classify(media: &str, body: &[u8]) -> Result<Kind, String> {
    if media.contains("html") || media == "application/xhtml+xml" {
        return Ok(Kind::Html);
    }
    if media.starts_with("text/")
        || matches!(
            media,
            "application/json" | "application/xml" | "application/ld+json"
        )
    {
        return Ok(Kind::Text);
    }
    if media.is_empty() {
        let head = String::from_utf8_lossy(&body[..body.len().min(200)])
            .trim_start()
            .to_ascii_lowercase();
        return Ok(if head.starts_with("<!doctype html") || head.starts_with("<html") {
            Kind::Html
        } else {
            Kind::Text
        });
    }
    Err(format!(
        "该链接返回的是 `{media}` 二进制内容,无法直接阅读;可下载到本地后用 file_extract 提取"
    ))
}

/// 分段读取响应体,全程施加 10MB 上限。
async fn read_limited(mut resp: reqwest::Response) -> Result<Vec<u8>, String> {
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("读取响应失败: {e}"))?
    {
        if buf.len() + chunk.len() > MAX_BYTES {
            return Err("响应超过 10MB 上限,已中止".into());
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// 解码正文:优先响应头 charset,其次 UTF-8,最后按 GBK(中文站点常见)。
fn decode_body(bytes: &[u8], charset: Option<&str>) -> String {
    if let Some(label) = charset
        && let Some(enc) = encoding_rs::Encoding::for_label(label.as_bytes())
    {
        let (text, _, _) = enc.decode(bytes);
        return text.into_owned();
    }
    match String::from_utf8(bytes.to_vec()) {
        Ok(s) => s,
        Err(_) => {
            let (text, _, _) = encoding_rs::GBK.decode(bytes);
            text.into_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_url_rejects_non_http_and_private_hosts() {
        assert!(validate_url("file:///C:/a.txt").is_err());
        assert!(validate_url("ftp://example.com").is_err());
        for host in [
            "http://127.0.0.1/a",
            "http://localhost:8080/",
            "http://192.168.1.2/",
            "http://169.254.169.254/latest/meta-data",
            "https://10.0.0.5/",
            "http://172.20.1.1/",
        ] {
            assert!(validate_url(host).is_err(), "应拦截 {host}");
        }
        assert!(validate_url("https://example.com/a").is_ok());
        assert!(validate_url("http://8.8.8.8/").is_ok());
    }

    #[test]
    fn decode_body_uses_charset_then_falls_back_to_gbk() {
        let gbk = [0xD6u8, 0xD0, 0xCE, 0xC4]; // “中文”
        assert_eq!(decode_body(&gbk, Some("gbk")), "中文");
        assert_eq!(decode_body(b"hello", None), "hello");
        assert_eq!(decode_body(&gbk, None), "中文");
    }

    #[test]
    fn classify_filters_binary_and_sniffs_html() {
        assert_eq!(classify("text/html; charset=utf-8", b"").unwrap(), Kind::Html);
        assert_eq!(classify("text/plain", b"").unwrap(), Kind::Text);
        assert_eq!(classify("application/json", b"").unwrap(), Kind::Text);
        assert_eq!(
            classify("", b"<!doctype html><html><body>hi").unwrap(),
            Kind::Html
        );
        assert_eq!(classify("", b"plain words").unwrap(), Kind::Text);
        assert!(classify("application/pdf", b"").is_err());
    }

    #[test]
    fn webpage_markdown_drops_boilerplate_and_menu_links() {
        let html = r#"<html><body>
            <nav><a href="/">首页</a><a href="/p">产品</a></nav>
            <div class="sidebar"><p>热门推荐</p></div>
            <article><h1>安装说明</h1><p>这里才是正文。</p></article>
            <footer>版权所有</footer>
        </body></html>"#;
        let md = webpage_markdown(html);
        assert!(md.contains("这里才是正文"));
        assert!(!md.contains("热门推荐"));
        assert!(!md.contains("版权所有"));
    }

    #[test]
    fn slice_from_reports_offset_and_continuation() {
        let text: String = ('a'..='z').collect::<String>();
        let (body, start, truncated) = slice_from(&text, 10, 5);
        assert_eq!((body.as_str(), start, truncated), ("klmno", 10, true));

        let (tail, start, truncated) = slice_from(&text, 24, 10);
        assert_eq!((tail.as_str(), start, truncated), ("yz", 24, false));

        let (empty, start, truncated) = slice_from(&text, 999, 5);
        assert!(empty.is_empty());
        assert_eq!(start, 26);
        assert!(!truncated);
    }

    /// 真实网络冒烟测试(默认忽略,手动执行):
    /// `cargo test live_fetch_picks_relevant -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_fetch_picks_relevant() {
        let page = fetch_page(
            "https://en.wikipedia.org/wiki/Rust_(programming_language)",
            Some(2000),
            None,
            Some("ownership borrow checker"),
        )
        .await
        .expect("抓取失败");
        assert!(page.query_selected, "query 应命中正文");
        assert!(
            page.content.to_ascii_lowercase().contains("borrow"),
            "返回内容应包含相关段落: {}",
            page.content
        );
        println!(
            "原文 {} 字符 → 返回 {} 字符",
            page.total_chars,
            page.content.chars().count()
        );

        // 中文页面:二元组分词同样应命中「所有权」
        let zh = fetch_page(
            "https://zh.wikipedia.org/wiki/Rust_(%E7%BC%96%E7%A8%8B%E8%AF%AD%E8%A8%80)",
            Some(1500),
            None,
            Some("所有权 借用检查"),
        )
        .await
        .expect("中文页面抓取失败");
        println!(
            "中文页: 原文 {} 字符 / 返回 {} 字符 / matched={} / 开头: {}",
            zh.total_chars,
            zh.content.chars().count(),
            zh.query_selected,
            zh.content.chars().take(200).collect::<String>()
        );
        assert!(zh.query_selected, "中文 query 应命中");
        assert!(
            zh.content.contains("所有权") || zh.content.contains("借用"),
            "中文返回内容应相关: {}",
            zh.content
        );

        // GitHub 仓库页:模板导航不应挤掉 README 正文
        let gh = fetch_page(
            "https://github.com/modelcontextprotocol/servers",
            Some(1500),
            None,
            Some("fetch server installation"),
        )
        .await
        .expect("GitHub 页面抓取失败");
        assert!(
            gh.content.to_ascii_lowercase().contains("server"),
            "GitHub 返回内容应包含正文: {}",
            gh.content
        );
    }
}
