//! MCP 模式搜索引擎:通过 MCP server 的工具检索,与内建引擎同接口。

use std::path::PathBuf;

use async_trait::async_trait;

use crate::mcp::{McpRuntime, McpToolCall, McpToolInfo};

use super::SearchEngine;
use super::config::DEFAULT_MCP_SERVER;

/// 通过 MCP server 的工具检索:与内建引擎同接口,mode = "mcp" 时替代直连 API。
/// 未启动时按配置懒启动(默认预置「联网搜索」= agent-search-mcp)。
pub(crate) struct McpSearch {
    pub(crate) runtime: McpRuntime,
    pub(crate) server: String,
    /// 用户指定的工具名;空 = 自动挑含 search 的工具
    pub(crate) tool: String,
    /// MCP 配置与审计日志所在目录
    pub(crate) data_dir: PathBuf,
}

impl McpSearch {
    /// 确保 server 在运行(已运行直接返回,不重复 spawn)。
    async fn ensure_started(&self) -> Result<(), String> {
        if self.runtime.is_running(&self.server).await {
            return Ok(());
        }
        let data_dir = &self.data_dir;
        let cfg = crate::mcp::load_or_init(data_dir);
        let target = cfg
            .servers
            .iter()
            .find(|s| s.name == self.server)
            .or_else(|| cfg.servers.iter().find(|s| s.name == DEFAULT_MCP_SERVER))
            .ok_or_else(|| format!("未找到 MCP 服务「{}」,请在设置 · MCP 工具中添加", self.server))?;
        self.runtime.start_from_config(target, data_dir).await
    }
}

/// 调工具的参数:只填 schema 里声明过的键,避免因多余字段被 server 拒绝。
pub(crate) fn mcp_search_args(
    tool: &McpToolInfo,
    query: &str,
    max_results: u32,
) -> serde_json::Value {
    let props = tool
        .input_schema
        .get("properties")
        .and_then(|v| v.as_object());
    let mut map = serde_json::Map::new();
    if props.is_some_and(|p| p.contains_key("query")) || props.is_none() {
        map.insert("query".into(), serde_json::Value::String(query.into()));
    }
    if props.is_some_and(|p| p.contains_key("max_results")) {
        map.insert("max_results".into(), serde_json::Value::from(max_results));
    }
    serde_json::Value::Object(map)
}

/// 选工具:优先用户指定,其次名字含 search,最后第一个可用工具。
pub(crate) fn pick_search_tool<'a>(
    tools: &'a [McpToolInfo],
    wanted: &str,
) -> Option<&'a McpToolInfo> {
    if !wanted.is_empty()
        && let Some(t) = tools.iter().find(|t| t.name == wanted)
    {
        return Some(t);
    }
    tools
        .iter()
        .find(|t| t.name.to_lowercase().contains("search"))
        .or_else(|| tools.first())
}

/// 解析 Tavily 官方 MCP 的纯文本结果格式(`tools/call` 返回 markdown 而非 JSON):
///   Detailed Results:
///   Title: xxx
///   URL: https://…
///   Content: …
fn parse_text_results(text: &str) -> Vec<super::RawSearchResult> {
    let mut out: Vec<super::RawSearchResult> = Vec::new();
    let mut cur: Option<(String, String, String)> = None;
    fn flush(out: &mut Vec<super::RawSearchResult>, cur: &mut Option<(String, String, String)>) {
        if let Some((title, url, content)) = cur.take()
            && !url.trim().is_empty()
        {
            out.push(super::RawSearchResult {
                title: if title.trim().is_empty() {
                    url.clone()
                } else {
                    title.trim().to_string()
                },
                url: url.trim().to_string(),
                snippet: content.trim().chars().take(2000).collect(),
                published: None,
            });
        }
    }
    for raw in text.lines() {
        let line = raw.trim();
        // 结果区结束后的区块(图片/爬取/站点地图)不再当作检索结果
        if matches!(line, "Images:" | "Crawl Results:" | "Map Results:") {
            flush(&mut out, &mut cur);
            break;
        }
        if let Some(t) = line.strip_prefix("Title: ") {
            flush(&mut out, &mut cur);
            cur = Some((t.to_string(), String::new(), String::new()));
        } else if let Some(u) = line.strip_prefix("URL: ") {
            if let Some((_, url, _)) = cur.as_mut()
                && url.is_empty()
            {
                *url = u.to_string();
            }
        } else if let Some(c) = line.strip_prefix("Content: ")
            && let Some((_, _, content)) = cur.as_mut()
            && content.is_empty()
        {
            *content = c.to_string();
        }
    }
    flush(&mut out, &mut cur);
    out
}

/// 把 MCP 返回文本解析成检索结果:优先 JSON(results 数组),其次 Tavily 文本格式,
/// 最后整段作为单条摘要。
pub(crate) fn parse_mcp_results(text: &str) -> Vec<super::RawSearchResult> {
    fn item_from(v: &serde_json::Value) -> Option<super::RawSearchResult> {
        let url = v.get("url").and_then(|x| x.as_str())?.trim().to_string();
        if url.is_empty() {
            return None;
        }
        let title = v
            .get("title")
            .and_then(|x| x.as_str())
            .unwrap_or(url.as_str())
            .trim()
            .to_string();
        let snippet = ["content", "snippet", "description", "text"]
            .iter()
            .find_map(|k| v.get(*k).and_then(|x| x.as_str()))
            .unwrap_or("")
            .trim()
            .to_string();
        let published = v
            .get("published_date")
            .or_else(|| v.get("published"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        Some(super::RawSearchResult {
            title,
            url,
            snippet,
            published,
        })
    }

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        let arr = v
            .get("results")
            .and_then(|x| x.as_array())
            .or_else(|| v.as_array());
        if let Some(arr) = arr {
            let out: Vec<_> = arr.iter().filter_map(item_from).collect();
            if !out.is_empty() {
                return out;
            }
        }
    }
    // Tavily 官方 MCP 的文本结果格式(含 URL,可保留引用)
    let from_text = parse_text_results(text);
    if !from_text.is_empty() {
        return from_text;
    }
    // 非 JSON 且无法识别结构(纯文本摘要):保留文本,URL 留空由上层展示兜底
    let trimmed = text.trim();
    if trimmed.is_empty() {
        Vec::new()
    } else {
        vec![super::RawSearchResult {
            title: "MCP 检索结果".into(),
            url: String::new(),
            snippet: trimmed.chars().take(2000).collect(),
            published: None,
        }]
    }
}

#[async_trait]
impl SearchEngine for McpSearch {
    fn name(&self) -> &str {
        "mcp"
    }

    async fn search(
        &self,
        query: &str,
        max_results: u32,
    ) -> Result<Vec<super::RawSearchResult>, String> {
        self.ensure_started().await?;
        let tools = self.runtime.list_tools(&self.server).await?;
        let tool = pick_search_tool(&tools, &self.tool).ok_or_else(|| {
            format!("MCP 服务「{}」未暴露任何工具", self.server)
        })?;
        let res = self
            .runtime
            .call_tool(
                McpToolCall {
                    server_name: self.server.clone(),
                    tool_name: tool.name.clone(),
                    arguments: mcp_search_args(tool, query, max_results),
                },
                self.data_dir.clone(),
            )
            .await;
        if !res.ok {
            return Err(res.error.unwrap_or_else(|| "MCP 工具调用失败".into()));
        }
        let out = parse_mcp_results(&res.content);
        if out.is_empty() {
            return Err("MCP 搜索无结果(检查 MCP 服务是否已配置 Key)".into());
        }
        Ok(out)
    }
}
