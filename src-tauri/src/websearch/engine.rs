#![allow(dead_code)]

use std::{fs, path::PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::mcp::{McpRuntime, McpToolCall, McpToolInfo};
use crate::provider::keychain;

use super::fetch::{Bocha, SearXng, Tavily, http_client};
use super::rank::{DeepseekSearch, Zhipu};

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

/// 调用方式:api = 内建直连引擎;mcp = 走 MCP 服务器(默认预置「联网搜索」= agent-search-mcp,零 Key 9 引擎并行)。
pub const SEARCH_MODES: &[&str] = &["api", "mcp"];

/// MCP 模式下默认使用的预置 server 名(设置页可改)。
pub const DEFAULT_MCP_SERVER: &str = "联网搜索";

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
    /// "api" | "mcp" —— 调用方式
    #[serde(default = "default_mode")]
    pub mode: String,
    /// "searxng" | "tavily" …
    pub engine: String,
    /// SearXNG 实例地址;Tavily 可留空(用默认 https://api.tavily.com)
    pub base_url: String,
    /// MCP 模式下发起的 server 名(空 = 「联网搜索」预置)
    #[serde(default)]
    pub mcp_server: String,
    /// MCP 模式使用的工具名(空 = 自动挑含 search 的工具)
    #[serde(default)]
    pub mcp_tool: String,
}

fn default_mode() -> String {
    "api".into()
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            engine: "searxng".into(),
            base_url: String::new(),
            mcp_server: String::new(),
            mcp_tool: String::new(),
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
    cleaned.mode = cleaned.mode.trim().to_lowercase();
    if cleaned.mode.is_empty() {
        cleaned.mode = default_mode();
    }
    if !SEARCH_MODES.contains(&cleaned.mode.as_str()) {
        return Err(format!("未知调用方式: {}", cleaned.mode));
    }
    cleaned.mcp_server = cleaned.mcp_server.trim().to_string();
    cleaned.mcp_tool = cleaned.mcp_tool.trim().to_string();
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
/// 每次调用都重新读盘,不缓存、不保留上一个引擎(保证设置页切换即时生效)。
#[allow(dead_code)]
pub fn build_engine(runtime: &McpRuntime) -> Result<Box<dyn SearchEngine>, String> {
    let cfg = load_config()?.ok_or("尚未配置搜索引擎,请先在深度研究页完成设置")?;
    build_engine_from(cfg, runtime, &crate::paths::data_dir())
}

/// 由给定配置构造引擎(测试接口可传入未保存的草稿,不依赖全局配置文件)。
pub fn build_engine_from(
    cfg: SearchConfig,
    runtime: &McpRuntime,
    data_dir: &std::path::Path,
) -> Result<Box<dyn SearchEngine>, String> {
    if cfg.mode == "mcp" {
        return Ok(Box::new(McpSearch {
            runtime: runtime.clone(),
            server: if cfg.mcp_server.is_empty() {
                DEFAULT_MCP_SERVER.to_string()
            } else {
                cfg.mcp_server.clone()
            },
            tool: cfg.mcp_tool.clone(),
            data_dir: data_dir.to_path_buf(),
        }));
    }
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

// ---------------- MCP 模式搜索引擎 ----------------

/// 通过 MCP server 的工具检索:与内建引擎同接口,mode = "mcp" 时替代直连 API。
/// 未启动时按配置懒启动(默认预置「联网搜索」= agent-search-mcp)。
struct McpSearch {
    runtime: McpRuntime,
    server: String,
    /// 用户指定的工具名;空 = 自动挑含 search 的工具
    tool: String,
    /// MCP 配置与审计日志所在目录
    data_dir: PathBuf,
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
fn mcp_search_args(tool: &McpToolInfo, query: &str, max_results: u32) -> serde_json::Value {
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
fn pick_search_tool<'a>(tools: &'a [McpToolInfo], wanted: &str) -> Option<&'a McpToolInfo> {
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
fn parse_text_results(text: &str) -> Vec<RawSearchResult> {
    let mut out: Vec<RawSearchResult> = Vec::new();
    let mut cur: Option<(String, String, String)> = None;
    fn flush(out: &mut Vec<RawSearchResult>, cur: &mut Option<(String, String, String)>) {
        if let Some((title, url, content)) = cur.take()
            && !url.trim().is_empty()
        {
            out.push(RawSearchResult {
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
        } else if let Some(c) = line.strip_prefix("Content: ") {
            if let Some((_, _, content)) = cur.as_mut()
                && content.is_empty()
            {
                *content = c.to_string();
            }
        }
    }
    flush(&mut out, &mut cur);
    out
}

/// 把 MCP 返回文本解析成检索结果:优先 JSON(results 数组),其次 Tavily 文本格式,
/// 最后整段作为单条摘要。
fn parse_mcp_results(text: &str) -> Vec<RawSearchResult> {
    fn item_from(v: &serde_json::Value) -> Option<RawSearchResult> {
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
        Some(RawSearchResult {
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
        vec![RawSearchResult {
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

    async fn search(&self, query: &str, max_results: u32) -> Result<Vec<RawSearchResult>, String> {
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
            ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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

    #[test]
    fn legacy_config_defaults_to_api_mode() {
        // 旧版 search_config.json 只有 engine/base_url:缺省字段必须回落 api
        let cfg: SearchConfig =
            serde_json::from_str(r#"{"engine":"tavily","base_url":""}"#).unwrap();
        assert_eq!(cfg.mode, "api");
        assert!(cfg.mcp_server.is_empty());
        assert!(cfg.mcp_tool.is_empty());
    }

    #[test]
    fn config_rejects_unknown_mode() {
        let _g = TEST_LOCK.lock().unwrap();
        let cfg = SearchConfig {
            mode: "grpc".into(),
            ..Default::default()
        };
        assert!(save_config(&cfg).is_err());
        assert!(SEARCH_MODES.contains(&"mcp"));
    }

    #[test]
    fn mcp_result_json_is_parsed() {
        let text = r#"{"results":[
          {"title":"Rust 官方","url":"https://www.rust-lang.org/","content":"系统编程语言","published_date":"2026-01-01"},
          {"title":"无链接条目","url":""}
        ]}"#;
        let got = parse_mcp_results(text);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].url, "https://www.rust-lang.org/");
        assert_eq!(got[0].snippet, "系统编程语言");
        assert_eq!(got[0].published.as_deref(), Some("2026-01-01"));
    }

    #[test]
    fn mcp_result_plain_text_falls_back() {
        let got = parse_mcp_results("  Tavily 返回的纯文本摘要  ");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].snippet, "Tavily 返回的纯文本摘要");
        assert!(parse_mcp_results("   ").is_empty());
    }

    /// 真实 tavily-mcp 返回的是这种文本格式(带 URL),必须能拆成多条并保留链接。
    #[test]
    fn mcp_result_tavily_text_format_is_parsed() {
        let text = "Answer: Rust 是系统编程语言\n\
Detailed Results:\n\
\n\
Title: Rust 官网\n\
URL: https://www.rust-lang.org/\n\
Content: Rust 兼顾安全与性能\n\
Raw Content: 很长的正文\n\
Favicon: https://www.rust-lang.org/favicon.ico\n\
\n\
Title: 第二篇\n\
URL: https://example.com/a\n\
Content: 第二段内容\n\
\n\
Images:\n\
[1] URL: https://img.example.com/x.png\n";
        let got = parse_mcp_results(text);
        assert_eq!(got.len(), 2, "文本结果条数异常:{got:?}");
        assert_eq!(got[0].title, "Rust 官网");
        assert_eq!(got[0].url, "https://www.rust-lang.org/");
        assert!(got[0].snippet.contains("系统编程语言") || got[0].snippet.contains("安全与性能"));
        assert_eq!(got[1].url, "https://example.com/a");
        assert!(
            !got.iter().any(|r| r.url.contains("img.example.com")),
            "Images 区污染了结果 URL:{got:?}"
        );
    }

    #[test]
    fn mcp_tool_pick_and_args_follow_schema() {
        let tools = vec![
            McpToolInfo {
                name: "tavily-extract".into(),
                description: String::new(),
                input_schema: serde_json::json!({"type":"object","properties":{"urls":{"type":"array"}}}),
            },
            McpToolInfo {
                name: "tavily-search".into(),
                description: String::new(),
                input_schema: serde_json::json!({"type":"object","properties":{"query":{"type":"string"},"max_results":{"type":"integer"}}}),
            },
        ];
        let picked = pick_search_tool(&tools, "").unwrap();
        assert_eq!(picked.name, "tavily-search");
        // 显式指定优先
        assert_eq!(pick_search_tool(&tools, "tavily-extract").unwrap().name, "tavily-extract");
        // 只填 schema 声明过的键
        let args = mcp_search_args(picked, "rust", 5);
        assert_eq!(args["query"], "rust");
        assert_eq!(args["max_results"], 5);
        assert!(args.get("include_images").is_none());
    }

    // ---------------- E2E:两条调用方式真实跑通、切换不复用 ----------------

    /// 极简检索 MCP server(stdio JSON-RPC):tools/list 暴露 search / extract,search 返回固定 JSON。
    const E2E_SEARCH_SERVER_JS: &str = r#"
const readline = require('readline');
const rl = readline.createInterface({ input: process.stdin });
const send = (o) => process.stdout.write(JSON.stringify(o) + '\n');
rl.on('line', (line) => {
  let msg; try { msg = JSON.parse(line); } catch { return; }
  const id = msg.id;
  if (msg.method === 'initialize') {
    send({ jsonrpc: '2.0', id, result: {
      protocolVersion: msg.params.protocolVersion,
      capabilities: { tools: {} },
      serverInfo: { name: 'e2e-search', version: '0.0.1' } } });
  } else if (msg.method === 'tools/list') {
    send({ jsonrpc: '2.0', id, result: { tools: [
      { name: 'tavily-extract', description: '抽取网页',
        inputSchema: { type: 'object', properties: { urls: { type: 'array' } }, required: ['urls'] } },
      { name: 'tavily-search', description: '检索',
        inputSchema: { type: 'object', properties: { query: { type: 'string' }, max_results: { type: 'integer' } }, required: ['query'] } } ] } });
  } else if (msg.method === 'tools/call') {
    const a = msg.params.arguments || {};
    const q = a.query || '';
    const n = a.max_results || 3;
    const results = [];
    for (let i = 0; i < n; i++) {
      results.push({ title: '结果' + i + ':' + q, url: 'https://example.com/' + i,
        content: '摘要 ' + q + ' #' + i, published_date: '2026-10-30' });
    }
    send({ jsonrpc: '2.0', id, result: { content: [{ type: 'text', text: JSON.stringify({ results }) }], isError: false } });
  } else if (id !== undefined) {
    send({ jsonrpc: '2.0', id, error: { code: -32601, message: 'method not found' } });
  }
});
"#;

    fn node_available() -> bool {
        std::process::Command::new("node")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// 准备隔离的数据目录:写入 mock server 脚本 + mcp-servers.json(不碰全局配置)。
    fn e2e_data_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "maf-websearch-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        let script = dir.join("server.js");
        fs::write(&script, E2E_SEARCH_SERVER_JS).unwrap();
        let cfg = serde_json::json!({
            "servers": [{
                "name": "e2e-search",
                "enabled": true,
                "transport": "stdio",
                "command": "node",
                "args": [script.to_string_lossy()],
                "url": "",
                "headers": {},
                "allowed_dirs": [],
                "env": {}
            }],
            "builtin_tools": true,
            "tools_enabled": false,
            "presets_version": crate::mcp::config::PRESETS_VERSION,
            "china_mirror": false
        });
        fs::write(
            crate::mcp::config_path(&dir),
            serde_json::to_string_pretty(&cfg).unwrap(),
        )
        .unwrap();
        dir
    }

    fn mcp_cfg(server: &str) -> SearchConfig {
        SearchConfig {
            mode: "mcp".into(),
            mcp_server: server.into(),
            ..Default::default()
        }
    }

    /// 真实 spawn stdio MCP server 跑搜索 → 切到 API → 再切回 MCP。
    /// 断言三件事:① MCP 模式真的调用了 server(结果里带本次 query);
    /// ② 第二次调用复用已启动 handle(不再 spawn);③ 切到 API 后拿到的是
    /// SearXNG 的连接错误而非上一次的 MCP 结果(即不存在"沿用前一个引擎")。
    #[tokio::test(flavor = "multi_thread")]
    async fn e2e_mcp_search_then_switch_to_api_does_not_reuse_previous_engine() {
        if !node_available() {
            eprintln!("skip: 本机无 node,跳过联网搜索 E2E");
            return;
        }
        let dir = e2e_data_dir("switch");
        let runtime = McpRuntime::default();

        // --- MCP 调用 ---
        let engine = build_engine_from(mcp_cfg("e2e-search"), &runtime, &dir).unwrap();
        assert_eq!(engine.name(), "mcp");

        let t0 = std::time::Instant::now();
        let first = engine.search("rust 生命周期", 3).await.expect("MCP 搜索失败");
        let first_ms = t0.elapsed().as_millis();
        assert_eq!(first.len(), 3, "MCP 结果条数异常:{first:?}");
        assert_eq!(first[0].url, "https://example.com/0");
        assert!(first[0].title.contains("rust 生命周期"), "查询未传给 server:{}", first[0].title);
        assert!(runtime.is_running("e2e-search").await, "server 未启动");

        let t1 = std::time::Instant::now();
        let second = engine.search("tauri 打包", 2).await.expect("MCP 二次搜索失败");
        let warm_ms = t1.elapsed().as_millis();
        assert_eq!(second.len(), 2);
        assert!(
            second[0].title.contains("tauri 打包"),
            "复用 handle 后查询串了:{}",
            second[0].title
        );
        println!("[E2E] MCP 首次(启动+搜索) {first_ms}ms / 复用 {warm_ms}ms");

        // --- 切到 API 调用:SearXNG 指向必然拒绝连接的地址 ---
        let api_cfg = SearchConfig {
            mode: "api".into(),
            engine: "searxng".into(),
            base_url: "http://127.0.0.1:1".into(),
            ..Default::default()
        };
        let api = build_engine_from(api_cfg, &runtime, &dir).unwrap();
        assert_eq!(api.name(), "searxng", "切到 API 后引擎没换");
        let err = api.search("x", 3).await.expect_err("死端口不应返回结果");
        assert!(
            !err.contains("example.com") && !err.contains("结果0"),
            "切到 API 后仍拿到上一个 MCP 引擎的结果:{err}"
        );

        // --- 切回 MCP:同一个 runtime 立即恢复 ---
        let back = build_engine_from(mcp_cfg("e2e-search"), &runtime, &dir).unwrap();
        assert_eq!(back.name(), "mcp");
        let third = back.search("deepseek", 2).await.expect("切回 MCP 后搜索失败");
        assert_eq!(third.len(), 2);
        assert!(third[0].title.contains("deepseek"));

        let _ = fs::remove_dir_all(&dir);
    }

    /// 真实 API 延迟(需真实 Key):`MYAGENT_DATA_DIR` 指向已配置 DeepSeek Key 的数据目录。
    /// 运行:`cargo test --lib perf_real_api_search_latency -- --ignored --nocapture`
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "真实网络 + 真实 Key"]
    async fn perf_real_api_search_latency() {
        let dir = crate::paths::data_dir();
        let cfg = SearchConfig {
            mode: "api".into(),
            engine: "deepseek".into(),
            ..Default::default()
        };
        let engine = build_engine_from(cfg, &McpRuntime::default(), &dir)
            .expect("构造 deepseek 引擎失败(检查 MYAGENT_DATA_DIR / Key)");
        for i in 0..3 {
            let t = std::time::Instant::now();
            let got = engine.search("Rust 2026 新特性", 5).await;
            match got {
                Ok(v) => println!("[perf] API(deepseek) 第{}次 {}ms / {} 条", i + 1, t.elapsed().as_millis(), v.len()),
                Err(e) => panic!("API 搜索失败({}ms): {e}", t.elapsed().as_millis()),
            }
        }
    }

    /// 真实 MCP 冷启动开销:spawn `npx -y tavily-mcp@latest` 并握手列工具。
    /// 运行:`cargo test --lib perf_real_tavily_mcp_startup -- --ignored --nocapture`
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "会真实 spawn npx tavily-mcp"]
    async fn perf_real_tavily_mcp_startup() {
        let rt = McpRuntime::default();
        let t = std::time::Instant::now();
        rt.start_server(
            "perf-tavily",
            "npx",
            &["-y".into(), "tavily-mcp@latest".into()],
            &[],
        )
        .await
        .expect("tavily-mcp 启动失败");
        println!("[perf] tavily-mcp 冷启动(进程+握手) {}ms", t.elapsed().as_millis());
        let t2 = std::time::Instant::now();
        let tools = rt.list_tools("perf-tavily").await.unwrap_or_default();
        println!(
            "[perf] tools/list {}ms,{} 个工具:{:?}",
            t2.elapsed().as_millis(),
            tools.len(),
            tools.iter().map(|x| x.name.clone()).collect::<Vec<_>>()
        );
        assert!(!tools.is_empty(), "tavily-mcp 未暴露任何工具");
    }

    /// 真实 agent-search-mcp 冷启动 + 首次联网检索。
    /// 运行:`cargo test --lib perf_real_agent_search_mcp -- --ignored --nocapture`
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "真实联网;spawn npx agent-search-mcp"]
    async fn perf_real_agent_search_mcp() {
        let rt = McpRuntime::default();
        let t0 = std::time::Instant::now();
        rt.start_server(
            "perf-agent",
            "npx",
            &["-y".into(), "agent-search-mcp@latest".into()],
            &[],
        )
        .await
        .expect("agent-search-mcp 启动失败");
        println!("[perf] agent-search-mcp 冷启动 {}ms", t0.elapsed().as_millis());
        let tools = rt.list_tools("perf-agent").await.unwrap_or_default();
        println!(
            "[perf] tools: {:?}",
            tools.iter().map(|t| t.name.clone()).collect::<Vec<_>>()
        );
        assert!(
            tools.iter().any(|t| t.name.contains("search")),
            "agent-search-mcp 未暴露 search 工具"
        );
        // 真实搜一次
        use crate::mcp::McpToolCall;
        let picked = pick_search_tool(&tools, "").expect("pick 失败");
        let args = mcp_search_args(picked, "Rust 2026 新特性", 5);
        let call = McpToolCall {
            server_name: "perf-agent".into(),
            tool_name: picked.name.clone(),
            arguments: args,
        };
        let t1 = std::time::Instant::now();
        let res = rt.call_tool(call, crate::paths::data_dir()).await;
        let elapsed = t1.elapsed().as_millis();
        println!(
            "[perf] agent-search-mcp.search {}ms ok={} error={:?}",
            elapsed,
            res.ok,
            res.error
        );
        assert!(res.ok, "agent-search-mcp 调用失败:{:?}", res.error);
        let parsed = parse_mcp_results(&res.content);
        println!("[perf] 解析到 {} 条结果", parsed.len());
        assert!(!parsed.is_empty(), "agent-search-mcp 返回内容解析为 0 条");
        for (i, r) in parsed.iter().take(3).enumerate() {
            println!("  [{}] {} → {}", i, r.title.chars().take(60).collect::<String>(), r.url);
        }
    }
}
