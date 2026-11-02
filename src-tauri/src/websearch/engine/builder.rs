//! 按配置构造搜索引擎:API 直连引擎与 MCP 引擎的工厂。

use crate::mcp::McpRuntime;
use crate::provider::keychain;

use super::super::fetch::{Bocha, SearXng, Tavily, http_client};
use super::super::rank::{DeepseekSearch, Zhipu};
use super::SearchEngine;
use super::config::{
    BOCHA_DEFAULT_BASE, DEEPSEEK_DEFAULT_BASE, DEFAULT_MCP_SERVER, SearchConfig,
    TAVILY_DEFAULT_BASE, ZHIPU_DEFAULT_BASE, engine_display, key_id, load_config,
};
use super::mcp::McpSearch;

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
