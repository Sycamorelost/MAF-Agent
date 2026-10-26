use async_trait::async_trait;

use super::engine::{RawSearchResult, SearchEngine};
use super::fetch::{execute, read_body};

/// 智谱 BigModel web-search-pro(国内,GLM 用户的 web 搜索工具)。
/// 文档:`POST {base}/api/paas/v4/tools`,Bearer 鉴权。
pub struct Zhipu {
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) client: reqwest::Client,
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
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) client: reqwest::Client,
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
            return Err(format!(
                "DeepSeek 搜索 HTTP {}: {}{}",
                status.as_u16(),
                msg,
                hint
            ));
        }

        let v: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| format!("DeepSeek 搜索响应解析失败: {e}"))?;
        let mut out = Vec::new();
        if let Some(arr) = v.get("results").and_then(|r| r.as_array()) {
            for item in arr.iter().take(max_results as usize) {
                let url = item
                    .get("url")
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
