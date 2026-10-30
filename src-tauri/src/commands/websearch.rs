//! 联网搜索命令(P0 自 commands/research/start.rs 迁出,IPC 名不变):
//! 搜索配置读写与测试查询,聊天页联网开关与设置页共用。

use serde::{Deserialize, Serialize};

use crate::commands::provider::validate_base_url_scheme;
use crate::websearch::{self, SourceItem};

#[derive(Debug, Serialize)]
pub struct SearchConfigDto {
    /// "api" | "mcp"
    pub mode: String,
    pub engine: String,
    pub base_url: String,
    /// 当前选择是否已配好 Key(对 API 引擎 / 需要 Key 的 MCP server)
    pub has_api_key: bool,
    /// MCP 模式:server 名(空 = 预置「联网搜索」)与工具名(空 = 自动)
    pub mcp_server: String,
    pub mcp_tool: String,
    /// MCP 模式:该 server 是否需要 Key(零 Key 的 server 不需要 has_api_key=true)
    pub mcp_needs_key: bool,
    /// MCP 模式:该 server 是否可用(已运行,或配置里存在可懒启动的同名服务)
    pub mcp_ready: bool,
    /// MCP 模式:当前 server 的显示名(前端直接展示,不用自己猜)
    pub mcp_server_display: String,
    /// MCP 模式:该 server 支持的工具名(运行时实时探测;未启动时为空数组)
    pub mcp_tool_names: Vec<String>,
}

/// 读取搜索引擎配置(不回传 key,只给 has_api_key)。
#[tauri::command]
pub async fn get_search_config(
    runtime: tauri::State<'_, crate::mcp::McpRuntime>,
) -> Result<SearchConfigDto, String> {
    let cfg = websearch::load_config()?.unwrap_or_default();
    // 默认 server 名(前端保存空值或未指定时)
    let server = if cfg.mcp_server.is_empty() {
        websearch::DEFAULT_MCP_SERVER.to_string()
    } else {
        cfg.mcp_server.clone()
    };
    // 判断当前 MCP server 是否需要 Key:默认 agent-search-mcp 零 Key,其余保守 true
    let data_dir = crate::paths::data_dir();
    let mcp_needs_key = if cfg.mode == "mcp" {
        let preset = crate::mcp::load_or_init(&data_dir);
        match preset.servers.iter().find(|s| s.name == server.as_str()) {
            Some(s) => !s.args.iter().any(|a| a.contains("agent-search-mcp")),
            None => true, // 自定义 server 未知,保守要求 Key
        }
    } else {
        false
    };
    // mcp 模式下 has_api_key:仅对 needs_key 的 server 才返回 true
    let has_key = if cfg.mode == "mcp" {
        if mcp_needs_key {
            websearch::has_engine_key("tavily")?
                || websearch::has_engine_key("bocha")?
                || websearch::has_engine_key("zhipu")?
        } else {
            true // 零 Key server 直接算 has_api_key=true,避免 engineReady 永远 false
        }
    } else if websearch::engine_needs_key(&cfg.engine) {
        websearch::has_engine_key(&cfg.engine)?
    } else {
        false
    };
    let mcp_ready = runtime.is_running(&server).await
        || crate::mcp::load_or_init(&data_dir)
            .servers
            .iter()
            .any(|s| s.name == server);
    // 已启动则实时探测工具名;未启动返回空数组(前端显示"未启动")
    let mcp_tool_names = if runtime.is_running(&server).await {
        runtime
            .list_tools(&server)
            .await
            .map(|tools| tools.into_iter().map(|t| t.name).collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    // server 显示名:从配置取
    let mcp_server_display = {
        let preset = crate::mcp::load_or_init(&data_dir);
        preset
            .servers
            .iter()
            .find(|s| s.name == server.as_str())
            .map(|s| format!("{}（{}）", s.name, s.args.join(" ")))
            .unwrap_or_else(|| server.clone())
    };
    Ok(SearchConfigDto {
        mode: cfg.mode,
        engine: cfg.engine,
        base_url: cfg.base_url,
        has_api_key: has_key,
        mcp_server: cfg.mcp_server,
        mcp_tool: cfg.mcp_tool,
        mcp_needs_key,
        mcp_ready,
        mcp_server_display,
        mcp_tool_names,
    })
}

#[derive(Debug, Deserialize)]
pub struct SaveSearchConfigInput {
    /// "api" | "mcp";缺省 api(前端旧版不传)
    #[serde(default = "default_search_mode")]
    pub mode: String,
    pub engine: String,
    pub base_url: String,
    /// key 类引擎用;非空才覆盖保存(searxng 忽略)
    pub api_key: Option<String>,
    #[serde(default)]
    pub mcp_server: String,
    #[serde(default)]
    pub mcp_tool: String,
}

fn default_search_mode() -> String {
    "api".into()
}

/// 保存搜索引擎配置(api_key 走 keychain 加密落盘,按引擎分别存储)。
#[tauri::command]
pub async fn save_search_config(
    runtime: tauri::State<'_, crate::mcp::McpRuntime>,
    input: SaveSearchConfigInput,
) -> Result<(), String> {
    // key 类引擎(bocha/zhipu/tavily)留空 base_url 时使用官方默认地址,仅非空时校验。
    // searxng 必填,留空会在 build_engine 处给出可读错误。
    let base_url = input.base_url.trim();
    if !base_url.is_empty() {
        validate_base_url_scheme(base_url)?;
    }
    websearch::save_config(&websearch::SearchConfig {
        mode: input.mode.clone(),
        engine: input.engine.clone(),
        base_url: input.base_url,
        mcp_server: input.mcp_server,
        mcp_tool: input.mcp_tool,
    })?;
    if let Some(k) = input.api_key {
        let k = k.trim();
        // mcp 模式下 Key 是可选付费档(Tavily/Bocha/Zhipu)的复用;agent-search-mcp 默认零 Key
        let key_engine = if input.mode.trim().eq_ignore_ascii_case("mcp") {
            // 付费档 Key 按用户输入的 engine 存,默认 tavily
            if k.is_empty() {
                // 空 Key:只清 tavily 档(付费档默认 key 槽位);不碰 bocha/zhipu
                "tavily"
            } else if !input.engine.is_empty() {
                &input.engine
            } else {
                "tavily"
            }
        } else {
            input.engine.as_str()
        };
        if !k.is_empty() && websearch::engine_needs_key(key_engine) {
            websearch::save_engine_key(key_engine, k)?;
            // 已运行的 agent-search-mcp 不会自动读取新付费 Key:停掉,下次懒启动重新走 preset_env
            if input.mode.trim().eq_ignore_ascii_case("mcp") {
                let server = websearch::load_config()?
                    .map(|c| c.mcp_server)
                    .unwrap_or_default();
                let server = if server.trim().is_empty() {
                    websearch::DEFAULT_MCP_SERVER.to_string()
                } else {
                    server
                };
                runtime.stop_server(&server).await;
            }
        }
    }
    Ok(())
}

/// 测试接口的草稿覆盖:设置页改了「调用方式」但还没保存时,按当前选择测(不落盘)。
#[derive(Debug, Default, Deserialize)]
pub struct SearchTestDraft {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub engine: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub mcp_server: Option<String>,
    #[serde(default)]
    pub mcp_tool: Option<String>,
}

impl SearchTestDraft {
    fn apply(self, base: websearch::SearchConfig) -> websearch::SearchConfig {
        let pick = |v: Option<String>, d: String| match v {
            Some(s) if !s.trim().is_empty() => s,
            _ => d,
        };
        websearch::SearchConfig {
            mode: pick(self.mode, base.mode),
            engine: pick(self.engine, base.engine),
            base_url: pick(self.base_url, base.base_url),
            mcp_server: self.mcp_server.unwrap_or(base.mcp_server),
            mcp_tool: self.mcp_tool.unwrap_or(base.mcp_tool),
        }
    }
}

/// 用当前配置跑一次测试查询,返回编号后的结果(验证配置可用)。
/// mode = "mcp" 时经 MCP runtime 调用预置「联网搜索」工具。
/// `draft` 非空时以草稿为准(设置页未保存也能验证切换是否真的生效)。
#[tauri::command]
pub async fn search_test(
    runtime: tauri::State<'_, crate::mcp::McpRuntime>,
    query: String,
    draft: Option<SearchTestDraft>,
) -> Result<Vec<SourceItem>, String> {
    let base = websearch::load_config()?.ok_or("尚未配置搜索引擎,请先在深度研究页完成设置")?;
    let cfg = match draft {
        Some(d) => d.apply(base),
        None => base,
    };
    let engine = websearch::build_engine_from(cfg, &runtime, &crate::paths::data_dir())?;
    let raws = engine.search(&query, 5).await?;
    Ok(raws
        .into_iter()
        .enumerate()
        .map(|(i, r)| SourceItem {
            id: (i + 1) as u32,
            title: r.title,
            url: r.url,
            snippet: r.snippet,
            engine: engine.name().to_string(),
            published: r.published,
        })
        .collect())
}
