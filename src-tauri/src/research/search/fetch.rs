use std::time::Duration;

use async_trait::async_trait;

use super::engine::{RawSearchResult, SearchEngine};

pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        // M-7:禁止 reqwest 自动跟随重定向;统一走 execute() 手动逐跳校验,
        // 防止被投毒/恶意的搜索实例把请求重定向到任意地址(盲 SSRF)。
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// 搜索响应体上限:10MB(正常 JSON 结果不足 1MB)。拦截超大响应防内存放大(M-7)。
const SEARCH_MAX_BYTES: usize = 10 * 1024 * 1024;

/// 发起搜索请求并手动跟随重定向(最多 5 跳)。
/// 搜索实例允许部署在内网(用户自建 SearXNG),故不做私网拦截,但:
/// 每跳协议必须是 http/https、跳数受限,杜绝 file:// 等降级与重定向环。
pub(crate) async fn execute(
    client: &reqwest::Client,
    req: reqwest::RequestBuilder,
    label: &str,
) -> Result<reqwest::Response, String> {
    let mut resp = req
        .send()
        .await
        .map_err(|e| format!("{label} 请求失败: {e}"))?;
    let mut hops = 0u32;
    while resp.status().is_redirection() {
        hops += 1;
        if hops > 5 {
            return Err(format!("{label} 重定向超过 5 跳,已拒绝"));
        }
        let loc = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| format!("{label} 重定向响应缺少 Location,已拒绝"))?;
        let next = resp
            .url()
            .join(loc)
            .map_err(|e| format!("{label} 重定向地址无法解析: {e}"))?
            .to_string();
        if !next.starts_with("http://") && !next.starts_with("https://") {
            return Err(format!("{label} 重定向到非 HTTP(S) 地址,已拒绝"));
        }
        resp = client
            .get(&next)
            .send()
            .await
            .map_err(|e| format!("{label} 跟随重定向失败: {e}"))?;
    }
    Ok(resp)
}

/// 读取搜索响应体,全程施加 10MB 上限。
pub(crate) async fn read_body(resp: reqwest::Response, label: &str) -> Result<String, String> {
    let mut buf: Vec<u8> = Vec::new();
    let mut resp = resp;
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("{label} 读取响应失败: {e}"))?
    {
        if buf.len() + chunk.len() > SEARCH_MAX_BYTES {
            return Err(format!("{label} 响应超过 10MB 上限"));
        }
        buf.extend_from_slice(&chunk);
    }
    String::from_utf8(buf).map_err(|e| format!("{label} 响应非 UTF-8: {e}"))
}

/// SearXNG 自托管实例(需在实例设置里开启 JSON format)。
pub struct SearXng {
    pub(crate) base_url: String,
    pub(crate) client: reqwest::Client,
}

#[async_trait]
impl SearchEngine for SearXng {
    fn name(&self) -> &str {
        "searxng"
    }

    async fn search(&self, query: &str, max_results: u32) -> Result<Vec<RawSearchResult>, String> {
        let url = format!("{}/search", self.base_url);
        let resp = execute(
            &self.client,
            self.client
                .get(&url)
                .query(&[
                    ("q", query),
                    ("format", "json"),
                    ("number_of_results", &max_results.to_string()),
                ])
                .header("Accept", "application/json"),
            "SearXNG",
        )
        .await?;

        let status = resp.status();
        let body = read_body(resp, "SearXNG").await?;
        if !status.is_success() {
            let hint = if status.as_u16() == 403 {
                "(该实例可能未开启 JSON 输出,请在 SearXNG settings.yml 的 search.formats 中加入 json)"
            } else {
                ""
            };
            return Err(format!("SearXNG HTTP {}: {}{}", status.as_u16(), body.chars().take(200).collect::<String>(), hint));
        }

        let v: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| format!("SearXNG 响应解析失败: {e}"))?;
        let mut out = Vec::new();
        if let Some(arr) = v.get("results").and_then(|r| r.as_array()) {
            for item in arr.iter().take(max_results as usize) {
                let url = item.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
                if url.is_empty() {
                    continue;
                }
                out.push(RawSearchResult {
                    title: item
                        .get("title")
                        .and_then(|t| t.as_str())
                        .unwrap_or("(无标题)")
                        .to_string(),
                    url,
                    snippet: item
                        .get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string(),
                    published: item
                        .get("publishedDate")
                        .and_then(|d| d.as_str())
                        .map(|s| s.to_string()),
                });
            }
        }
        Ok(out)
    }
}

/// Tavily 托管检索 API。
pub struct Tavily {
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) client: reqwest::Client,
}

#[async_trait]
impl SearchEngine for Tavily {
    fn name(&self) -> &str {
        "tavily"
    }

    async fn search(&self, query: &str, max_results: u32) -> Result<Vec<RawSearchResult>, String> {
        let url = format!("{}/search", self.base_url);
        let resp = execute(
            &self.client,
            self.client.post(&url).json(&serde_json::json!({
                "api_key": self.api_key,
                "query": query,
                "max_results": max_results,
                "include_answer": false,
            })),
            "Tavily",
        )
        .await?;

        let status = resp.status();
        let body = read_body(resp, "Tavily").await?;
        if !status.is_success() {
            return Err(format!(
                "Tavily HTTP {}: {}",
                status.as_u16(),
                body.chars().take(200).collect::<String>()
            ));
        }

        let v: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| format!("Tavily 响应解析失败: {e}"))?;
        let mut out = Vec::new();
        if let Some(arr) = v.get("results").and_then(|r| r.as_array()) {
            for item in arr.iter().take(max_results as usize) {
                let url = item.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
                if url.is_empty() {
                    continue;
                }
                out.push(RawSearchResult {
                    title: item
                        .get("title")
                        .and_then(|t| t.as_str())
                        .unwrap_or("(无标题)")
                        .to_string(),
                    url,
                    snippet: item
                        .get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string(),
                    published: item
                        .get("published_date")
                        .and_then(|d| d.as_str())
                        .map(|s| s.to_string()),
                });
            }
        }
        Ok(out)
    }
}

/// 博查 Bocha(国内,专为 AI 场景的中文搜索 API)。
/// 文档:`POST {base}/v1/web-search`,Bearer 鉴权。
pub struct Bocha {
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) client: reqwest::Client,
}

#[async_trait]
impl SearchEngine for Bocha {
    fn name(&self) -> &str {
        "bocha"
    }

    async fn search(&self, query: &str, max_results: u32) -> Result<Vec<RawSearchResult>, String> {
        let url = format!("{}/v1/web-search", self.base_url);
        let resp = execute(
            &self.client,
            self.client
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&serde_json::json!({
                    "query": query,
                    "count": max_results.min(50),
                    "summary": false,
                    "freshness": "noLimit",
                })),
            "博查",
        )
        .await?;

        let status = resp.status();
        let body = read_body(resp, "博查").await?;
        if !status.is_success() {
            return Err(format!(
                "博查 HTTP {}: {}",
                status.as_u16(),
                body.chars().take(200).collect::<String>()
            ));
        }

        let v: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| format!("博查响应解析失败: {e}"))?;
        if let Some(code) = v.get("code").and_then(|c| c.as_i64()) {
            if code != 200 {
                let msg = v.get("msg").and_then(|m| m.as_str()).unwrap_or("未知错误");
                return Err(format!("博查错误 {code}: {msg}"));
            }
        }

        let mut out = Vec::new();
        if let Some(arr) = v.pointer("/data/webPages/value").and_then(|r| r.as_array()) {
            for item in arr.iter().take(max_results as usize) {
                let url = item.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
                if url.is_empty() {
                    continue;
                }
                out.push(RawSearchResult {
                    title: item
                        .get("name")
                        .and_then(|t| t.as_str())
                        .unwrap_or("(无标题)")
                        .to_string(),
                    url,
                    snippet: item
                        .get("snippet")
                        .or_else(|| item.get("summary"))
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string(),
                    published: item
                        .get("datePublished")
                        .and_then(|d| d.as_str())
                        .map(|s| s.to_string()),
                });
            }
        }
        Ok(out)
    }
}
