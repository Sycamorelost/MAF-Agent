//! 搜索引擎配置:模式/引擎常量、配置读写与 Key 状态。

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::provider::keychain;

/// 各引擎的 API Key 在 keychain 中的 id 前缀(`{engine}-search`)。
pub(crate) fn key_id(engine: &str) -> String {
    format!("{engine}-search")
}

pub(crate) const TAVILY_DEFAULT_BASE: &str = "https://api.tavily.com";
pub(crate) const BOCHA_DEFAULT_BASE: &str = "https://api.bochaai.com";
pub(crate) const ZHIPU_DEFAULT_BASE: &str = "https://open.bigmodel.cn";
pub(crate) const DEEPSEEK_DEFAULT_BASE: &str = "https://api.deepseek.com";

/// 引擎是否已有可用配置(供前端提示;searxng 看 base_url,其余看 key)。
pub const ENGINES: &[&str] = &["bocha", "zhipu", "deepseek", "searxng", "tavily"];

/// 调用方式:api = 内建直连引擎;mcp = 走 MCP 服务器(默认预置「联网搜索」= agent-search-mcp,零 Key 9 引擎并行)。
pub const SEARCH_MODES: &[&str] = &["api", "mcp"];

/// MCP 模式下默认使用的预置 server 名(设置页可改)。
pub const DEFAULT_MCP_SERVER: &str = "联网搜索";

pub(crate) fn engine_display(engine: &str) -> &'static str {
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

pub(crate) fn config_path() -> PathBuf {
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
