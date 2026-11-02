//! 搜索引擎抽象与配置(本模块门面)。
//!
//! 职责拆分:
//! - `config.rs`:模式/引擎常量、`SearchConfig` 读写与 Key 状态;
//! - `builder.rs`:按配置构造 API 直连引擎;
//! - `mcp.rs`:MCP 模式搜索引擎(工具选择 / 参数拼装 / 结果解析);
//! - 本文件:`SearchEngine` trait 与 `RawSearchResult`,并重导出全部公开项。

#![allow(dead_code)]

mod builder;
mod config;
mod mcp;

#[cfg(test)]
mod tests;

use async_trait::async_trait;

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

#[allow(unused_imports)]
pub use builder::{build_engine, build_engine_from};
#[allow(unused_imports)]
pub use config::{
    DEFAULT_MCP_SERVER, ENGINES, SEARCH_MODES, SearchConfig, engine_needs_key, has_engine_key,
    load_config, save_config, save_engine_key,
};
// 测试与应用内兄弟模块使用的私有 helper(不扩大为 pub)。
#[allow(unused_imports)]
pub(crate) use config::{config_path, key_id};
#[allow(unused_imports)]
pub(crate) use mcp::{mcp_search_args, parse_mcp_results, pick_search_tool};
