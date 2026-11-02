//! 搜索引擎配置、MCP 结果解析与端到端切换的测试。

use std::{fs, path::PathBuf};

use crate::mcp::{McpRuntime, McpToolInfo};

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
