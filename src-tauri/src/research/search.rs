//! 搜索引擎抽象与配置持久化。
//!
//! 支持五种引擎(国内优先):
//!   - 博查 Bocha(国内,API Key):`POST {base_url}/v1/web-search`
//!   - 智谱 BigModel(国内,API Key):`POST {base_url}/api/paas/v4/tools`(web-search-pro)
//!   - DeepSeek 搜索(官方,API Key):`POST {base_url}/v1/tools/search`(可复用 DeepSeek 对话 Key)
//!   - SearXNG(自托管,免 Key):`GET {base_url}/search?format=json&q=...`
//!   - Tavily(海外,API Key):`POST {base_url}/search`
//!
//! 配置落盘 `data/search_config.json`(只含 engine / base_url,不含 Key);
//! Key 按 `{engine}-search` 为 id 走 keychain 加密落盘。

use std::{fs, path::PathBuf, time::Duration};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::provider::keychain;

/// 各引擎的 API Key 在 keychain 中的 id 前缀(`{engine}-search`)。
fn key_id(engine: &str) -> String {
    format!("{engine}-search")
}

const TAVILY_DEFAULT_BASE: &str = "https://api.tavily.com";
const BOCHA_DEFAULT_BASE: &str = "https://api.bochaai.com";
const ZHIPU_DEFAULT_BASE: &str = "https://open.bigmodel.cn";
const DEEPSEEK_DEFAULT_BASE: &str = "https://api.deepseek.com";

/// 引擎是否已有可用配置(供前端提示;searxng 看 base_url,其余看 key)。
pub const ENGINES: &[&str] = &["bocha", "zhipu", "deepseek", "searxng", "tavily"];

fn engine_display(engine: &str) -> &'static str {
    match engine {
        "bocha" => "博查 Bocha",
        "zhipu" => "智谱 BigModel",
        "deepseek" => "DeepSeek 搜索",
        "searxng" => "SearXNG",
        "tavily" => "Tavily",
        _ => "未知引擎",
    }
}

// ---------------- 配置 ----------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// "searxng" | "tavily"
    pub engine: String,
    /// SearXNG 实例地址;Tavily 可留空(用默认 https://api.tavily.com)
    pub base_url: String,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            engine: "searxng".into(),
            base_url: String::new(),
        }
    }
}

fn config_path() -> PathBuf {
    crate::paths::data_dir().join("search_config.json")
}

pub fn load_config() -> Result<Option<SearchConfig>, String> {
    let p = config_path();
    if !p.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&p).map_err(|e| format!("read search config failed: {e}"))?;
    let cfg: SearchConfig =
        serde_json::from_str(&text).map_err(|e| format!("parse search config failed: {e}"))?;
    Ok(Some(cfg))
}

pub fn save_config(cfg: &SearchConfig) -> Result<(), String> {
    let p = config_path();
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir data failed: {e}"))?;
    }
    let mut cleaned = cfg.clone();
    cleaned.engine = cleaned.engine.trim().to_lowercase();
    if !ENGINES.contains(&cleaned.engine.as_str()) {
        return Err(format!("未知搜索引擎: {}", cleaned.engine));
    }
    cleaned.base_url = cleaned.base_url.trim().trim_end_matches('/').to_string();
    let text = serde_json::to_string_pretty(&cleaned)
        .map_err(|e| format!("serialize search config failed: {e}"))?;
    fs::write(&p, text).map_err(|e| format!("write search config failed: {e}"))?;
    Ok(())
}

/// 保存某引擎的 API Key(searxng 不需要,不会调用)。
pub fn save_engine_key(engine: &str, api_key: &str) -> Result<(), String> {
    keychain::store(&key_id(engine), api_key)
}

/// 某引擎是否已存 API Key。
/// deepseek 特殊:可复用 DeepSeek 对话 provider 的 Key(任一槽位)。
pub fn has_engine_key(engine: &str) -> Result<bool, String> {
    if keychain::load(&key_id(engine))?.is_some() {
        return Ok(true);
    }
    if engine == "deepseek" {
        return Ok(!keychain::list_slots("deepseek")?.is_empty());
    }
    Ok(false)
}

/// 该引擎是否需要 API Key。
pub fn engine_needs_key(engine: &str) -> bool {
    engine != "searxng"
}

// ---------------- 引擎抽象 ----------------

/// 单条原始检索结果(未分配引用编号)。
#[derive(Debug, Clone)]
pub struct RawSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub published: Option<String>,
}

#[async_trait]
pub trait SearchEngine: Send + Sync {
    fn name(&self) -> &str;
    async fn search(&self, query: &str, max_results: u32) -> Result<Vec<RawSearchResult>, String>;
}

fn http_client() -> reqwest::Client {
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
async fn execute(
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
async fn read_body(resp: reqwest::Response, label: &str) -> Result<String, String> {
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
    base_url: String,
    client: reqwest::Client,
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
    base_url: String,
    api_key: String,
    client: reqwest::Client,
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
    base_url: String,
    api_key: String,
    client: reqwest::Client,
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

/// 智谱 BigModel web-search-pro(国内,GLM 用户的 web 搜索工具)。
/// 文档:`POST {base}/api/paas/v4/tools`,Bearer 鉴权。
pub struct Zhipu {
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

#[async_trait]
impl SearchEngine for Zhipu {
    fn name(&self) -> &str {
        "zhipu"
    }

    async fn search(&self, query: &str, max_results: u32) -> Result<Vec<RawSearchResult>, String> {
        let url = format!("{}/api/paas/v4/tools", self.base_url);
        let resp = execute(
            &self.client,
            self.client
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&serde_json::json!({
                    "tool": "web-search-pro",
                    "stream": false,
                    "messages": [{"role": "user", "content": query}],
                })),
            "智谱",
        )
        .await?;

        let status = resp.status();
        let body = read_body(resp, "智谱").await?;
        if !status.is_success() {
            return Err(format!(
                "智谱 HTTP {}: {}",
                status.as_u16(),
                body.chars().take(200).collect::<String>()
            ));
        }

        let v: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| format!("智谱响应解析失败: {e}"))?;
        if let Some(err) = v.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("未知错误");
            return Err(format!("智谱错误: {msg}"));
        }

        // 结果在 choices[0].message.tool_calls[] 中 type == "search_result" 的项里
        let mut out: Vec<RawSearchResult> = Vec::new();
        if let Some(tcs) = v
            .pointer("/choices/0/message/tool_calls")
            .and_then(|t| t.as_array())
        {
            for tc in tcs {
                if tc.get("type").and_then(|t| t.as_str()) != Some("search_result") {
                    continue;
                }
                if let Some(arr) = tc.get("search_result").and_then(|r| r.as_array()) {
                    for item in arr {
                        let url = item
                            .get("link")
                            .or_else(|| item.get("url"))
                            .and_then(|u| u.as_str())
                            .unwrap_or("")
                            .to_string();
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
                                .get("publish_date")
                                .and_then(|d| d.as_str())
                                .map(|s| s.to_string()),
                        });
                        if out.len() >= max_results as usize {
                            break;
                        }
                    }
                }
            }
        }
        out.truncate(max_results as usize);
        Ok(out)
    }
}

/// DeepSeek 官方 Search API(2026-10-17 开放,自家搜索引擎)。
/// 文档:`POST {base}/v1/tools/search`,Bearer 鉴权(与 DeepSeek 对话 API 同 Key)。
/// 计费:仅成功且有结果的请求计费(¥5/千次);失败/空结果不计费。限流 5 QPS。
pub struct DeepseekSearch {
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

#[async_trait]
impl SearchEngine for DeepseekSearch {
    fn name(&self) -> &str {
        "deepseek"
    }

    async fn search(&self, query: &str, max_results: u32) -> Result<Vec<RawSearchResult>, String> {
        let url = format!("{}/v1/tools/search", self.base_url);
        let resp = execute(
            &self.client,
            self.client
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&serde_json::json!({
                    "query": query,
                    "count": max_results.clamp(1, 50),
                    // 不拉全文:控制延迟与上下文体积,snippet/summary 已够注入与展示
                    "need_content": false,
                })),
            "DeepSeek 搜索",
        )
        .await?;

        let status = resp.status();
        let body = read_body(resp, "DeepSeek 搜索").await?;
        if !status.is_success() {
            // OpenAI 兼容错误格式:{error:{message,...}}
            let msg = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| {
                    v.pointer("/error/message")
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| body.chars().take(200).collect());
            let hint = match status.as_u16() {
                401 => "(API Key 无效,请检查设置)",
                402 => "(账户余额不足,请到 DeepSeek 开放平台充值)",
                429 => "(超过 5 QPS 限流,请稍后重试)",
                _ => "",
            };
            return Err(format!("DeepSeek 搜索 HTTP {}: {}{}", status.as_u16(), msg, hint));
        }

        let v: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| format!("DeepSeek 搜索响应解析失败: {e}"))?;
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
                    // snippet(100-200 字,官方定位为 UI 展示)优先;为空时退化 summary
                    snippet: item
                        .get("snippet")
                        .and_then(|c| c.as_str())
                        .filter(|s| !s.is_empty())
                        .or_else(|| item.get("summary").and_then(|c| c.as_str()))
                        .unwrap_or("")
                        .to_string(),
                    published: item
                        .get("date")
                        .and_then(|d| d.as_str())
                        .map(|s| s.to_string()),
                });
            }
        }
        Ok(out)
    }
}

/// 读取某引擎的 API Key;base_url 为空时用默认地址。
fn build_keyed_engine(
    cfg: &SearchConfig,
    engine: &str,
    default_base: &str,
) -> Result<(String, String), String> {
    let key = keychain::load(&key_id(engine))?
        .ok_or_else(|| format!("未配置 {} 的 API Key", engine_display(engine)))?;
    let base = if cfg.base_url.is_empty() {
        default_base.to_string()
    } else {
        cfg.base_url.clone()
    };
    Ok((base, key))
}

/// 按已保存配置构造搜索引擎;未配置时返回可读错误。
pub fn build_engine() -> Result<Box<dyn SearchEngine>, String> {
    let cfg = load_config()?.ok_or("尚未配置搜索引擎,请先在深度研究页完成设置")?;
    match cfg.engine.as_str() {
        "searxng" => {
            if cfg.base_url.is_empty() {
                return Err("SearXNG 未配置实例地址(base_url)".into());
            }
            Ok(Box::new(SearXng {
                base_url: cfg.base_url,
                client: http_client(),
            }))
        }
        "tavily" => {
            let (base, key) = build_keyed_engine(&cfg, "tavily", TAVILY_DEFAULT_BASE)?;
            Ok(Box::new(Tavily {
                base_url: base,
                api_key: key,
                client: http_client(),
            }))
        }
        "bocha" => {
            let (base, key) = build_keyed_engine(&cfg, "bocha", BOCHA_DEFAULT_BASE)?;
            Ok(Box::new(Bocha {
                base_url: base,
                api_key: key,
                client: http_client(),
            }))
        }
        "zhipu" => {
            let (base, key) = build_keyed_engine(&cfg, "zhipu", ZHIPU_DEFAULT_BASE)?;
            Ok(Box::new(Zhipu {
                base_url: base,
                api_key: key,
                client: http_client(),
            }))
        }
        "deepseek" => {
            // Key 解析顺序:引擎独立 Key(deepseek-search) → 对话 provider Key(slot 0)
            // → provider 任意槽位。让用户「配过 DeepSeek 聊天即开箱可用联网」。
            let key = keychain::load(&key_id("deepseek"))?
                .or(keychain::load("deepseek").ok().flatten())
                .or_else(|| {
                    keychain::list_slots("deepseek")
                        .ok()?
                        .into_iter()
                        .find_map(|s| keychain::load_slot("deepseek", s).ok().flatten())
                })
                .ok_or_else(|| {
                    "未配置 DeepSeek API Key(可在模型服务或此处配置,两处通用)".to_string()
                })?;
            let base = if cfg.base_url.is_empty() {
                DEEPSEEK_DEFAULT_BASE.to_string()
            } else {
                cfg.base_url.clone()
            };
            Ok(Box::new(DeepseekSearch {
                base_url: base,
                api_key: key,
                client: http_client(),
            }))
        }
        other => Err(format!("未知搜索引擎: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 配置文件是共享资源,相关测试必须串行。
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn config_save_load_roundtrip() {
        let _g = TEST_LOCK.lock().unwrap();
        let cfg = SearchConfig {
            engine: "SearXNG".into(),
            base_url: "http://127.0.0.1:8080/".into(),
        };
        save_config(&cfg).unwrap();
        let got = load_config().unwrap().unwrap();
        assert_eq!(got.engine, "searxng"); // 归一化为小写
        assert_eq!(got.base_url, "http://127.0.0.1:8080"); // 去尾斜杠
        let _ = fs::remove_file(config_path());
    }

    #[test]
    fn config_rejects_unknown_engine() {
        let cfg = SearchConfig {
            engine: "bing".into(),
            base_url: "x".into(),
        };
        assert!(save_config(&cfg).is_err());
    }

    #[test]
    fn config_accepts_all_engines() {
        let _g = TEST_LOCK.lock().unwrap();
        for e in ENGINES {
            let cfg = SearchConfig {
                engine: e.to_string(),
                base_url: "http://x".into(),
            };
            save_config(&cfg).unwrap_or_else(|err| panic!("engine {e} should be accepted: {err}"));
        }
        let _ = fs::remove_file(config_path());
    }

    #[test]
    fn key_id_is_engine_scoped() {
        assert_eq!(key_id("bocha"), "bocha-search");
        assert_eq!(key_id("zhipu"), "zhipu-search");
        assert_eq!(key_id("deepseek"), "deepseek-search");
        assert!(engine_needs_key("bocha"));
        assert!(engine_needs_key("deepseek"));
        assert!(!engine_needs_key("searxng"));
    }
}
