use std::{fs, path::PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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
