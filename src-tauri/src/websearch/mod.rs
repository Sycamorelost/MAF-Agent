#![allow(dead_code)]

//! 通用联网搜索:搜索引擎抽象与配置持久化(P0 自 research/search 迁出,IPC 名不变)。
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

mod engine;
mod fetch;
mod rank;

use serde::{Deserialize, Serialize};

#[allow(unused_imports)]
pub use engine::{
    DEFAULT_MCP_SERVER, ENGINES, RawSearchResult, SEARCH_MODES, SearchConfig, SearchEngine,
    build_engine, build_engine_from, engine_needs_key, has_engine_key, load_config, save_config,
    save_engine_key,
};
#[allow(unused_imports)]
pub use fetch::{Bocha, SearXng, Tavily};
#[allow(unused_imports)]
pub use rank::{DeepseekSearch, Zhipu};

/// 一条已分配编号的引用来源(联网搜索结果 DTO,研究事件协议复用同一结构)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceItem {
    pub id: u32,
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub engine: String,
    pub published: Option<String>,
}
