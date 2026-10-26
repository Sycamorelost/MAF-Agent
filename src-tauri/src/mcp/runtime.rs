//! MCP Runtime:spawn stdio MCP server,list_tools / call_tool,Drop 时 kill 子进程。
//!
//! rmcp 3.7.0 真实签名:
//!   .serve() → RunningService<RoleClient, ClientConfig>
//!   .peer()  → Peer<RoleClient>
//!   .service_client() → ClientTransport<RoleClient>
//!   CallToolResponse { Complete(CallToolResult), InputRequired, Task }  — 3.7.0 新增了 Response 枚举包装
//!   Tool.input_schema: Arc<serde_json::Map>
//!   Tool.name: Cow<'_, str>

use crate::mcp::config::McpServerConfig;
use rmcp::ServiceExt;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ClientCapabilities, ClientConfig,
    Implementation, ListToolsResult, Tool,
};
use rmcp::transport::TokioChildProcess;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;

/// 单条 MCP 文本结果上限与总预算(字符数),防止一次工具调用撑爆上下文。
const MAX_TOOL_TEXT_CHARS: usize = 32_000;
const MAX_TOOL_TOTAL_CHARS: usize = 64_000;

/// 工具描述(返回给前端或 ChatStream 的轻量结构)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// 工具调用请求(从 ChatStream 路由过来时的结构)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCall {
    pub server_name: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// 工具调用结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    pub ok: bool,
    pub content: String,
    pub error: Option<String>,
}

/// 全局 MCP Runtime —— tauri::State 注入。
#[derive(Default, Clone)]
pub struct McpRuntime {
    handles: Arc<Mutex<HashMap<String, McpHandle>>>,
}

/// 已启动的 MCP server handle。`S` 类型就是 `.serve()` 返回的第二个类型参数。
#[allow(dead_code)] // started_at 预留给 Phase 5d 设置页展示状态
struct McpHandle {
    runtime: rmcp::service::RunningService<rmcp::RoleClient, ClientConfig>,
    started_at: chrono::DateTime<chrono::Local>,
}

impl McpRuntime {
    /// 启动一个 server。
    pub async fn start_server(
        &self,
        name: &str,
        command: &str,
        args: &[String],
        env: &[(String, String)],
    ) -> Result<(), String> {
        let mut lock = self.handles.lock().await;
        if let Some(old) = lock.remove(name) {
            drop(old);
        }

        let client_info = ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::from_build_env(),
        );

        let mut cmd = build_command(command, args)?;
        // 注入环境变量(如 memory server 的 MEMORY_FILE_PATH,避免写进 npx 缓存目录)
        for (k, v) in env {
            cmd.env(k, v);
        }
        let transport =
            TokioChildProcess::new(cmd).map_err(|e| format!("启动子进程 `{command}` 失败: {e}"))?;

        // npx 首次运行要下载 npm 包,可能较慢;加超时避免一直卡住(持锁期间其他命令会阻塞)
        use tokio::time::{Duration, timeout};
        let runtime = timeout(Duration::from_secs(120), client_info.serve(transport))
            .await
            .map_err(|_| format!("启动 `{name}` 超时(120s):npx 首次下载较慢或网络不可达"))?
            .map_err(|e| format!("serve `{name}` 失败: {e}"))?;

        lock.insert(
            name.to_string(),
            McpHandle {
                runtime,
                started_at: chrono::Local::now(),
            },
        );

        log::info!("[MCP] server `{name}` started");
        Ok(())
    }

    /// 按配置启动 server —— 统一入口,UI 手动启动与对话前懒启动都走这里。
    /// stdio 会把 `allowed_dirs` 追加为参数并注入 memory 环境变量;http 只校验端点与请求头。
    pub async fn start_from_config(
        &self,
        cfg: &McpServerConfig,
        data_dir: &std::path::Path,
    ) -> Result<(), String> {
        if cfg.is_http() {
            crate::mcp::safety::validate_server_url(&cfg.url)?;
            crate::mcp::safety::validate_headers(&cfg.headers)?;
            return self.start_http(&cfg.name, &cfg.url, &cfg.headers).await;
        }

        crate::mcp::safety::validate_server_command(&cfg.command)?;
        // 文件系统类 server 把允许目录作为启动参数传入;
        // 未配置时回落沙箱目录(与安全预检共用 effective_allowed_dirs,边界一致),
        // 目录不存在时先创建,避免 server 因参数目录缺失起步失败
        let dirs = crate::mcp::config::effective_allowed_dirs(cfg, data_dir);
        for d in &dirs {
            if !d.exists() {
                let _ = std::fs::create_dir_all(d);
            }
        }
        let mut args = cfg.args.clone();
        args.extend(dirs.iter().map(|d| d.display().to_string()));
        let mut env = crate::mcp::config::memory_env(&args, data_dir);
        // 国内镜像:npx 首次下载 npm 包走 npmmirror,uvx/pip 走清华 PyPI(默认开,设置页可关)
        if crate::mcp::load_or_init(data_dir).china_mirror {
            env.extend(crate::mcp::config::mirror_env(&cfg.command));
        }
        self.start_server(&cfg.name, &cfg.command, &args, &env)
            .await
    }

    /// 启动远端 Streamable HTTP MCP server(无子进程,无本地执行面)。
    pub async fn start_http(
        &self,
        name: &str,
        url: &str,
        headers: &HashMap<String, String>,
    ) -> Result<(), String> {
        // 先握手(不持锁),成功后再替换旧连接 —— 新连接失败时旧连接仍可用
        let runtime = crate::mcp::http::connect(name, url, headers).await?;

        let mut lock = self.handles.lock().await;
        if let Some(old) = lock.remove(name) {
            drop(old);
        }
        lock.insert(
            name.to_string(),
            McpHandle {
                runtime,
                started_at: chrono::Local::now(),
            },
        );

        log::info!("[MCP] http server `{name}` started ({url})");
        Ok(())
    }

    /// 列出指定 server 的所有工具。
    pub async fn list_tools(&self, name: &str) -> Result<Vec<McpToolInfo>, String> {
        let handles = self.handles.clone();
        let lock = handles.lock().await;
        let handle = lock
            .get(name)
            .ok_or_else(|| format!("server `{name}` 未启动"))?;

        let resp: ListToolsResult = handle
            .runtime
            .peer()
            .list_tools(None)
            .await
            .map_err(|e| format!("list_tools `{name}` 失败: {e}"))?;

        Ok(resp.tools.into_iter().map(tool_to_info).collect())
    }

    /// 按工具名反查所属 server —— ChatStream 只拿到工具名,需要路由到已启动的 server。
    /// 先收集名字列表再逐个查询,避免持锁 await(防止死锁)。
    pub async fn find_server_for_tool(&self, tool: &str) -> Option<String> {
        let names: Vec<String> = {
            let lock = self.handles.lock().await;
            lock.keys().cloned().collect()
        };
        for name in names {
            if let Ok(tools) = self.list_tools(&name).await
                && tools.iter().any(|t| t.name == tool)
            {
                return Some(name);
            }
        }
        None
    }

    /// 调用工具,30s 超时 + 审计日志。
    pub async fn call_tool(&self, call: McpToolCall, data_dir: PathBuf) -> McpToolResult {
        use tokio::time::{Duration, timeout};

        let handles = self.handles.clone();
        let lock = handles.lock().await;

        let handle = match lock.get(&call.server_name) {
            Some(h) => h,
            None => {
                return McpToolResult {
                    ok: false,
                    content: String::new(),
                    error: Some(format!("server `{}` 未启动", call.server_name)),
                };
            }
        };

        let tool_name = call.tool_name.clone();
        let args_obj = call.arguments.as_object().cloned().unwrap_or_default();
        let params = CallToolRequestParams::new(tool_name).with_arguments(args_obj);

        let before = std::time::Instant::now();
        // 单次工具调用上限 120s:directory_tree 扫大目录、memory 重建图谱都可能超过 30s;
        // 仍保留上限防 server 死锁永久占住会话
        let result = timeout(
            Duration::from_secs(120),
            handle.runtime.call_tool_once(params),
        )
        .await;

        let elapsed = before.elapsed();
        crate::mcp::audit(
            &data_dir,
            "call_tool",
            &call.server_name,
            &call.tool_name,
            &format!("elapsed={}ms ok={}", elapsed.as_millis(), result.is_ok()),
        );

        match result {
            Ok(Ok(resp)) => handle_response(resp),
            Ok(Err(e)) => McpToolResult {
                ok: false,
                content: String::new(),
                error: Some(format!("call_tool 协议错误: {e}")),
            },
            Err(_) => McpToolResult {
                ok: false,
                content: String::new(),
                error: Some("30s 超时".into()),
            },
        }
    }

    /// 停止所有 server —— app 退出时调,kill 所有子进程。
    #[allow(dead_code)] // Phase 5e ChatStream 退出时调
    pub async fn stop_all(&self) {
        let mut lock = self.handles.lock().await;
        let names: Vec<_> = lock.keys().cloned().collect();
        lock.clear();
        for n in names {
            log::info!("[MCP] server `{n}` stopped");
        }
    }
}

/// 构造子进程命令。
///
/// Windows 上 `Command::new("npx")` 必然失败 —— CreateProcess 只会自动补 `.exe`,
/// 而 npm 装的 npx 是 `npx.cmd`。这里先按 PATH + 常见扩展名解析出真实文件,
/// `.cmd/.bat` 再交给 `cmd.exe /C` 执行。
#[cfg(windows)]
fn build_command(command: &str, args: &[String]) -> Result<Command, String> {
    let (exe, needs_shell) = resolve_windows_command(command)?;
    if needs_shell {
        // 走 cmd.exe 时参数会二次解析,必须拒绝 shell 元字符
        if let Some(bad) = args.iter().find(|a| has_shell_metachar(a)) {
            return Err(format!("参数包含 shell 元字符,已拒绝: `{bad}`"));
        }
        let mut c = Command::new("cmd");
        c.raw_arg("/C");
        c.raw_arg(build_cmd_line(&exe, args));
        return Ok(c);
    }
    let mut c = Command::new(&exe);
    c.args(args);
    Ok(c)
}

#[cfg(not(windows))]
fn build_command(command: &str, args: &[String]) -> Result<Command, String> {
    let mut c = Command::new(command);
    c.args(args);
    Ok(c)
}

/// 拼 `cmd /C` 的命令行:整条命令再包一层引号 `"\"exe\" \"a1\""`。
/// 不这么写的话,参数里带空格时 cmd 会剥掉最外层引号,把 `C:\Program` 当成命令。
#[cfg(windows)]
fn build_cmd_line(exe: &str, args: &[String]) -> String {
    let mut line = format!("\"\"{exe}\"");
    for a in args {
        line.push_str(" \"");
        line.push_str(a);
        line.push('"');
    }
    line.push('"');
    line
}

/// 按 PATH 与扩展名优先级解析命令;返回(真实路径, 是否需要 cmd.exe 启动)。
#[cfg(windows)]
fn resolve_windows_command(command: &str) -> Result<(String, bool), String> {
    use std::path::Path;

    let is_shell = |p: &Path| {
        matches!(
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .as_deref(),
            Some("cmd") | Some("bat")
        )
    };

    if command.contains('\\') || command.contains('/') {
        let p = Path::new(command);
        return if p.is_file() {
            Ok((command.to_string(), is_shell(p)))
        } else {
            Err(format!("命令不存在: `{command}`"))
        };
    }

    let dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    // .exe 优先(原生可执行),其次才是需要 shell 的 .cmd/.bat
    for ext in ["exe", "cmd", "bat", "com"] {
        for dir in &dirs {
            let cand = dir.join(format!("{command}.{ext}"));
            if cand.is_file() {
                return Ok((cand.display().to_string(), ext == "cmd" || ext == "bat"));
            }
        }
    }
    Err(format!(
        "找不到命令 `{command}`。请确认已安装 Node.js(自带 npx),或改用绝对路径。"
    ))
}

#[cfg(windows)]
fn has_shell_metachar(arg: &str) -> bool {
    arg.chars().any(|c| {
        matches!(
            c,
            '&' | '|' | '>' | '<' | '^' | '%' | '!' | '"' | '\n' | '\r'
        )
    })
}

fn tool_to_info(t: Tool) -> McpToolInfo {
    McpToolInfo {
        name: t.name.into_owned(),
        description: t.description.map(|c| c.into_owned()).unwrap_or_default(),
        input_schema: serde_json::Value::Object(t.input_schema.as_ref().clone()),
    }
}

fn handle_response(resp: CallToolResponse) -> McpToolResult {
    match resp {
        CallToolResponse::Complete(result) => handle_complete(result),
        CallToolResponse::InputRequired(_) => McpToolResult {
            ok: false,
            content: String::new(),
            error: Some("server 需要额外输入(MRTR 扩展,暂不支持)".into()),
        },
        CallToolResponse::Task(_) => McpToolResult {
            ok: false,
            content: String::new(),
            error: Some("server 把调用 materialize 为 task,暂不支持".into()),
        },
        _ => McpToolResult {
            ok: false,
            content: String::new(),
            error: Some("未知 response 类型".into()),
        },
    }
}

fn handle_complete(result: CallToolResult) -> McpToolResult {
    let mut parts: Vec<String> = Vec::new();
    let mut budget = MAX_TOOL_TOTAL_CHARS;
    for c in result.content {
        if let rmcp::model::ContentBlock::Text(t) = c {
            if budget == 0 {
                break;
            }
            let capped =
                crate::mcp::safety::truncate_chars(&t.text, MAX_TOOL_TEXT_CHARS.min(budget));
            budget = budget.saturating_sub(capped.chars().count());
            parts.push(capped);
        }
    }
    let text = parts.join("\n");

    let is_err = result.is_error.unwrap_or(false);
    McpToolResult {
        ok: !is_err,
        content: text,
        error: if is_err {
            Some("server reported error".into())
        } else {
            None
        },
    }
}

#[cfg(test)]
mod e2e_tests {
    use super::*;

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
            .start_server(name, "node", &[script.to_string_lossy().to_string()], &[])
            .await
            .expect("MCP server 启动失败");

        let tools = runtime.list_tools(name).await.expect("tools/list 失败");
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
}

#[cfg(all(test, windows))]
mod win_tests;
