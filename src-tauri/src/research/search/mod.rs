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

mod engine;
mod fetch;
mod rank;

#[allow(unused_imports)]
pub use engine::{
    ENGINES, RawSearchResult, SearchConfig, SearchEngine, build_engine, engine_needs_key,
    has_engine_key, load_config, save_config, save_engine_key,
};
#[allow(unused_imports)]
pub use fetch::{Bocha, SearXng, Tavily};
#[allow(unused_imports)]
pub use rank::{DeepseekSearch, Zhipu};
