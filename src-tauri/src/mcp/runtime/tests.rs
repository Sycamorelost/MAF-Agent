//! 真实 stdio MCP server 的端到端回归(node 不可用时跳过)。

use super::*;
use crate::mcp::mcp_call::McpToolCall;

/// 极简 MCP server(stdio JSON-RPC):initialize / tools/list / tools/call。
/// 只为验证 rmcp 客户端握手 + 工具调用链路,不承担业务功能。
const SERVER_JS: &str = r#"
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
      serverInfo: { name: 'e2e-fake', version: '0.0.1' } } });
  } else if (msg.method === 'tools/list') {
    send({ jsonrpc: '2.0', id, result: { tools: [ {
      name: 'echo', description: '原样回显',
      inputSchema: { type: 'object', properties: { text: { type: 'string' } }, required: ['text'] } } ] } });
  } else if (msg.method === 'tools/call') {
    const text = (msg.params.arguments && msg.params.arguments.text) || '';
    send({ jsonrpc: '2.0', id, result: { content: [{ type: 'text', text: 'echo:' + text }], isError: false } });
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

/// 全链路:真实 spawn 一个 stdio MCP server → 握手 → 列工具 → 调工具 → 清理。
/// node 不可用时跳过(不视为失败,避免环境差异阻塞 CI)。
#[tokio::test(flavor = "multi_thread")]
async fn real_stdio_mcp_server_round_trip() {
    if !node_available() {
        eprintln!("skip: 本机无 node,跳过 MCP E2E");
        return;
    }
    let dir = std::env::temp_dir().join("maf-mcp-e2e");
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("server.js");
    std::fs::write(&script, SERVER_JS).unwrap();

    let runtime = McpRuntime::default();
    let name = "e2e-fake";
    runtime
        .start_server(
            name,
            "node",
            &[script.to_string_lossy().to_string()],
            &[],
            false,
            true,
        )
        .await
        .expect("MCP server 启动失败");

    let tools = runtime
        .list_tools(name, false)
        .await
        .expect("tools/list 失败");
    assert!(
        tools.iter().any(|t| t.name == "echo"),
        "未取到 echo 工具,实际:{:?}",
        tools.iter().map(|t| t.name.clone()).collect::<Vec<_>>()
    );

    let result = runtime
        .call_tool(
            McpToolCall {
                server_name: name.into(),
                tool_name: "echo".into(),
                arguments: serde_json::json!({ "text": "hi" }),
            },
            dir.clone(),
        )
        .await;
    assert!(result.ok, "工具调用失败:{:?}", result.error);
    assert!(
        result.content.contains("echo:hi"),
        "内容异常:{}",
        result.content
    );

    // 反查:按工具名定位 server(dispatcher 依赖该方法路由)
    let found = runtime.find_server_for_tool("echo").await;
    assert_eq!(found.as_deref(), Some(name));

    runtime.stop_all().await;
}

/// 未启动的 server 反查必须返回 None(否则会误路由到不存在的连接)。
#[tokio::test(flavor = "multi_thread")]
async fn find_server_for_unknown_tool_returns_none() {
    let runtime = McpRuntime::default();
    assert!(runtime.find_server_for_tool("no_such_tool").await.is_none());
}
