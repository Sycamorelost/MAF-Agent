//! 远端 MCP:Streamable HTTP 传输。
//!
//! 仅负责建立连接与握手;端点 / 请求头的安全校验在 [`crate::mcp::safety`],
//! 配置分派在 [`crate::mcp::runtime::McpRuntime::start_from_config`]。

use rmcp::ServiceExt;
use rmcp::model::{ClientCapabilities, ClientConfig, Implementation};
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;

/// 已握手的客户端连接。
pub type Running = rmcp::service::RunningService<rmcp::RoleClient, ClientConfig>;

/// 握手超时:远端不可达时不应无限等待(单次工具调用另有 30s 超时)。
const HANDSHAKE_TIMEOUT_SECS: u64 = 45;

/// 连接远端 MCP 端点并完成 initialize 握手。
pub async fn connect(
    name: &str,
    url: &str,
    headers: &HashMap<String, String>,
) -> Result<Running, String> {
    let mut custom = HashMap::new();
    for (k, v) in headers {
        let hn = http::header::HeaderName::from_bytes(k.trim().as_bytes())
            .map_err(|_| format!("非法请求头名称 `{k}`"))?;
        let hv = http::header::HeaderValue::from_str(v)
            .map_err(|_| format!("请求头 `{k}` 的值含非法字符"))?;
        custom.insert(hn, hv);
    }

    let config = StreamableHttpClientTransportConfig::with_uri(url.trim().to_string())
        .custom_headers(custom);
    let transport = StreamableHttpClientTransport::from_config(config);

    let client_info = ClientConfig::new(
        ClientCapabilities::default(),
        Implementation::from_build_env(),
    );

    timeout(
        Duration::from_secs(HANDSHAKE_TIMEOUT_SECS),
        client_info.serve(transport),
    )
    .await
    .map_err(|_| format!("连接 `{name}` 超时({HANDSHAKE_TIMEOUT_SECS}s):端点不可达或响应过慢"))?
    .map_err(|e| format!("连接 `{name}` 失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::CallToolRequestParams;

    /// 最小 Streamable HTTP MCP server(JSON 响应模式,不支持 SSE 推送):
    /// initialize / tools/list / tools/call 三个方法,足够覆盖真实握手与调用。
    const SERVER_JS: &str = r#"
const http = require('http');
const port = Number(process.argv[2]);
const server = http.createServer((req, res) => {
  let body = '';
  req.on('data', (c) => (body += c));
  req.on('end', () => {
    if (req.method === 'GET') { res.writeHead(405); return res.end(); }
    if (req.method === 'DELETE') { res.writeHead(200); return res.end(); }
    const msg = body ? JSON.parse(body) : {};
    if (msg.id === undefined || msg.id === null) { res.writeHead(202); return res.end(); }
    let result;
    if (msg.method === 'initialize') {
      result = {
        protocolVersion: (msg.params || {}).protocolVersion || '2025-06-18',
        capabilities: { tools: {} },
        serverInfo: { name: 'fake-http-mcp', version: '1.0.0' }
      };
    } else if (msg.method === 'tools/list') {
      result = { tools: [{ name: 'echo', description: 'echo back',
        inputSchema: { type: 'object', properties: { text: { type: 'string' } } } }] };
    } else if (msg.method === 'tools/call') {
      result = { content: [{ type: 'text',
        text: 'echo:' + (((msg.params || {}).arguments || {}).text || '') }] };
    } else {
      res.writeHead(400, { 'Content-Type': 'application/json' });
      return res.end(JSON.stringify({ jsonrpc: '2.0', id: msg.id,
        error: { code: -32601, message: 'method not found' } }));
    }
    res.writeHead(200, {
      'Content-Type': 'application/json',
      'mcp-session-id': 'test-session'
    });
    res.end(JSON.stringify({ jsonrpc: '2.0', id: msg.id, result }));
  });
});
server.listen(port, '127.0.0.1');
"#;

    /// 本机没装 Node 时跳过(与 stdio e2e 测试同一策略)。
    async fn node_available() -> bool {
        tokio::process::Command::new("node")
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn free_port() -> Option<u16> {
        std::net::TcpListener::bind("127.0.0.1:0")
            .ok()
            .and_then(|l| l.local_addr().ok())
            .map(|a| a.port())
    }

    /// HTTP 传输端到端:握手 → tools/list → tools/call。
    #[tokio::test(flavor = "multi_thread")]
    async fn http_transport_round_trip() {
        if !node_available().await {
            return;
        }
        let Some(port) = free_port() else { return };

        let dir = std::env::temp_dir().join(format!("maf-http-mcp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("server.js");
        std::fs::write(&script, SERVER_JS).unwrap();

        let mut child = match tokio::process::Command::new("node")
            .arg(&script)
            .arg(port.to_string())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => return,
        };
        tokio::time::sleep(Duration::from_millis(700)).await;

        let url = format!("http://127.0.0.1:{port}/mcp");
        assert!(
            crate::mcp::safety::validate_server_url(&url).is_ok(),
            "本机回环明文地址应被放行"
        );

        let mut headers = HashMap::new();
        headers.insert("X-API-Key".to_string(), "test-key".to_string());

        let client = connect("fake-http", &url, &headers)
            .await
            .expect("HTTP MCP 握手失败");
        let tools = client
            .peer()
            .list_tools(None)
            .await
            .expect("tools/list 失败");
        assert!(
            tools.tools.iter().any(|t| t.name == "echo"),
            "未取到 echo 工具"
        );

        let mut args = serde_json::Map::new();
        args.insert("text".into(), serde_json::Value::String("hi".into()));
        let resp = client
            .call_tool_once(CallToolRequestParams::new("echo").with_arguments(args))
            .await
            .expect("tools/call 失败");
        assert!(
            format!("{resp:?}").contains("echo:hi"),
            "工具结果异常: {resp:?}"
        );

        let _ = client.cancel().await;
        let _ = child.kill().await;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
