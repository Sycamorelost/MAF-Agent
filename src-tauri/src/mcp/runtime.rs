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
    CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation, ListToolsResult,
};
use rmcp::transport::TokioChildProcess;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, timeout};

/// 单条 MCP 文本结果上限与总预算(字符数),防止一次工具调用撑爆上下文。
/// === 超时分档(C4):预置 server 用更短超时,用户自定义 server 保守给足 ===
/// 启动超时:预置 server(npx -y 官方包)15s 足够,首次 npm 下载也一般在 5–10s 内;
/// 用户自定义 server 可能是 npx/uvx/npm/yarn 首次拉包,保留 120s。
pub const PRESET_STARTUP_TIMEOUT_SECS: u64 = 15;
pub const CUSTOM_STARTUP_TIMEOUT_SECS: u64 = 120;

/// `tools/list` 单次超时:预置 server 本地应答一般 1–2s,5s 充足;
/// 自定义 server 可能较慢,给 10s。
pub const PRESET_LIST_TOOLS_TIMEOUT_SECS: u64 = 5;
pub const CUSTOM_LIST_TOOLS_TIMEOUT_SECS: u64 = 10;

/// 探活(peek_alive)超时:预置 server 走 list_tools,1.5s 足够;
/// 自定义 server 探活走相同 list_tools,给 3s。
pub const PRESET_PEEK_ALIVE_TIMEOUT_SECS: u64 = 2;
pub const CUSTOM_PEEK_ALIVE_TIMEOUT_SECS: u64 = 5;

/// 工具调用上限统一 60s(C4):directory_tree 扫大目录、memory 重建图谱正常远小于此;
/// 再长就是 server 死锁,硬上限防永久占住会话。
pub const CALL_TOOL_TIMEOUT_SECS: u64 = 60;

// === 指数退避重连(C3)常量与计数都在 reconnect.rs ===

// 工具调用/结果/元信息已抽到 mcp_call.rs,避免本文件超 700 行。
use crate::mcp::mcp_call::{handle_response, McpToolCall, McpToolInfo, McpToolResult, tool_to_info};

/// 全局 MCP Runtime —— tauri::State 注入。
#[derive(Default, Clone)]
pub struct McpRuntime {
    handles: Arc<Mutex<HashMap<String, McpHandle>>>,
    /// 正在启动中的 server 名。启动过程(npx 首次下载)最长 120s,
    /// 期间不能持有 `handles` 锁,否则会把所有 list_tools / call_tool 一起卡住。
    starting: Arc<Mutex<std::collections::HashSet<String>>>,
    /// 工具列表缓存(P1-5):server 的工具清单在进程生命周期内基本不变,
    /// 每轮对话实时 list_tools(最坏 10s×N 串行)是发送后空窗的主要来源。
    /// 生命周期事件(启动/重启/停止)发生时按名失效。
    tools_cache: Arc<Mutex<HashMap<String, Vec<McpToolInfo>>>>,
    /// C3:失败追踪 —— 连续失败自动禁用 + 指数退避重连逻辑独立模块。
    pub failure: crate::mcp::reconnect::FailureTracker,
}

/// 已启动的 MCP server handle。`S` 类型就是 `.serve()` 返回的第二个类型参数。
#[allow(dead_code)] // started_at 预留给 Phase 5d 设置页展示状态
struct McpHandle {
    /// 用 Arc 共享:RPC 时只克隆指针、立刻释放 `handles` 锁,
    /// 避免持锁跨 await(一个 server 无响应会拖死整层 MCP)。
    runtime: Arc<rmcp::service::RunningService<rmcp::RoleClient, ClientConfig>>,
    started_at: chrono::DateTime<chrono::Local>,
}

impl McpRuntime {
    /// 查询指定 server 是否已启动(仅检查 handle 存在,不做 RPC 探活,低开销)。
    pub async fn is_running(&self, name: &str) -> bool {
        self.handles.lock().await.contains_key(name)
    }

    /// 启动一个 server。已在运行的 server 不会被 kill+respawn(幂等)。
    /// 探活失败才清理 stale handle 后重新 spawn。
    ///
    /// 关键约束:**绝不在启动(最长 120s)期间持有 `handles` 锁** ——
    /// 否则启动 npx server 的同时,任何对话的 list_tools/call_tool 都会被阻塞,
    /// 表现为「已显示正在思考但一直没有反应」。
    ///
    /// C3 指数退避:连续失败累加计数,耗尽 MAX_FAILURE_RETRIES 后标记 disabled_by_failure,
    /// 后续调用直接返回错误,直到用户手动启用或应用重启。
    /// `manual=true` 时使用完整退避;懒启动/自动重启把等待上限压到 1s,
    /// 避免一次失败拖慢用户发消息。
    pub async fn start_server(
        &self,
        name: &str,
        command: &str,
        args: &[String],
        env: &[(String, String)],
        is_preset: bool,
        manual: bool,
    ) -> Result<(), String> {
        // --- C3:disabled_by_failure 快速拒绝 ---
        if self.failure.disabled_by_failure.lock().await.contains(name) {
            return Err(format!(
                "server `{name}` 已因连续失败被自动禁用;请在设置页重新启用或应用重启后再试"
            ));
        }

        // --- 第一段:幂等检查(不持锁做 RPC 探活) ---
        if self.handles.lock().await.contains_key(name) {
            // 探活:list_tools 成功说明健康,幂等跳过
            if self.peek_alive(name, is_preset).await.is_ok() {
                log::debug!("[MCP] server `{name}` 已在运行,跳过启动");
                return Ok(());
            }
            // stale:清理 + 失败计数累加
            {
                let mut lock = self.handles.lock().await;
                if let Some(old) = lock.remove(name) {
                    drop(old);
                    self.invalidate_tools_cache(name).await;
                    log::info!("[MCP] server `{name}` 连接已断,清理旧进程后重启");
                }
            }
            self.failure.increment(name).await;
            // C3:指数退避等待后再试一次,本次 stale 清理计入失败计数
            let count = *self.failure.failure_counts.lock().await.get(name).unwrap_or(&0);
            if self.failure.is_disabled(name).await {
                return Err(format!("server `{name}` 连续失败 {count} 次,已自动禁用"));
            }
            let backoff = crate::mcp::reconnect::backoff_ms(count);
            // 自动链路(对话前懒启动)不能为退避等几十秒,压到 1s 内
            let wait = if manual { backoff } else { backoff.min(1_000) };
            tokio::time::sleep(Duration::from_millis(wait)).await;
        }

        // --- 第二段:同一 server 的并发启动串行化(等待上限 ~125s) ---
        // 不持 handles 锁,只用一个轻量标记集合,避免重复 spawn 出两个子进程。
        for _ in 0..625 {
            if !self.starting.lock().await.contains(name) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        // 等待期间可能已被其他任务成功启动
        if self.handles.lock().await.contains_key(name) {
            return Ok(());
        }
        self.starting.lock().await.insert(name.to_string());

        let res = self.spawn_handle(name, command, args, env, is_preset).await;
        self.starting.lock().await.remove(name);
        if res.is_err() {
            // C3:spawn / 握手失败同样是「一次失败」,否则自动禁用只统计 stale 断连
            self.failure.increment(name).await;
        }
        res
    }

    /// 真正 spawn 子进程并握手(不持任何全局锁)。
    ///
    /// C6 环境变量 scrub:移除包含 KEY/PASSWORD/SECRET/TOKEN 的变量与 `MYAGENT_*`,
    /// 借鉴 DSH:MCP 子进程不应继承宿主的 API key / token 等敏感信息。
    ///
    /// 注意:不能改用 `.env_clear()` —— Windows 子进程缺少 `SystemRoot` / `PATHEXT` /
    /// `COMSPEC` 等基础变量时,npx / node / cmd 会直接启动失败。
    /// C1 PATH 补全:给子进程注入增强 PATH,包含 homebrew / ~/.local/bin / ~/.cargo/bin 等。
    /// C4 超时分档:预置 server 用短超时(15s),自定义 server 保守给 120s。
    async fn spawn_handle(
        &self,
        name: &str,
        command: &str,
        args: &[String],
        env: &[(String, String)],
        is_preset: bool,
    ) -> Result<(), String> {
        let client_info = ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::from_build_env(),
        );

        // 命令构造(Windows 解析 .cmd/.bat 等)在 util 中,便于单测
        let mut cmd = crate::mcp::util::build_command(command, args)?;

        // === C6:按敏感关键字移除宿主继承的环境变量(保留系统必需变量) ===
        for (k, _) in std::env::vars_os() {
            if let Some(key) = k.to_str()
                && crate::mcp::util::is_sensitive_env_key(key)
            {
                cmd.env_remove(key);
            }
        }

        // C1:构造增强 PATH,覆盖宿主默认 PATH(宿主 GUI 进程 PATH 往往过短)。
        let enhanced_path = crate::mcp::util::build_enhanced_path();
        cmd.env("PATH", enhanced_path);

        // C6:显式配置的 env 后注入,可覆盖上面的移除结果(包含预置/用户配置/镜像/数据路径)
        for (k, v) in env {
            cmd.env(k, v);
        }

        let transport =
            TokioChildProcess::new(cmd).map_err(|e| format!("启动子进程 `{command}` 失败: {e}"))?;

        // C4 超时分档:预置 server 15s,自定义 server 120s
        let startup_secs = if is_preset {
            PRESET_STARTUP_TIMEOUT_SECS
        } else {
            CUSTOM_STARTUP_TIMEOUT_SECS
        };

        let runtime = timeout(Duration::from_secs(startup_secs), client_info.serve(transport))
            .await
            .map_err(|_| {
                format!(
                    "启动 `{name}` 超时({startup_secs}s):npx 首次下载较慢或网络不可达"
                )
            })?
            .map_err(|e| format!("serve `{name}` 失败: {e}"))?;

        self.handles.lock().await.insert(
            name.to_string(),
            McpHandle {
                runtime: Arc::new(runtime),
                started_at: chrono::Local::now(),
            },
        );
        // 新进程的工具清单可能与旧实例不同,缓存即刻作废(P1-5)
        self.invalidate_tools_cache(name).await;
        // C3:启动成功,重置失败计数
        self.failure.failure_counts.lock().await.remove(name);
        self.failure.disabled_by_failure.lock().await.remove(name);

        log::info!("[MCP] server `{name}` started");
        Ok(())
    }

    /// 探活:对已持有的 handle 做一次轻量 RPC,成功即认为健康。
    /// C4 超时分档:预置 server 2s,自定义 server 5s,避免 server 进程挂住时无限阻塞其他 MCP 操作。
    /// pub:commands/mcp.rs 的 mcp_doctor 诊断命令需调用。
    pub async fn peek_alive(&self, name: &str, is_preset: bool) -> Result<(), String> {
        let timeout_secs = if is_preset {
            PRESET_PEEK_ALIVE_TIMEOUT_SECS
        } else {
            CUSTOM_PEEK_ALIVE_TIMEOUT_SECS
        };
        // 必须走真实 RPC —— 缓存命中不能证明进程还活着,否则「假活」永远不会被清理
        timeout(
            Duration::from_secs(timeout_secs),
            self.list_tools(name, is_preset),
        )
        .await
        .map_err(|_| format!("probe `{name}` 超时({timeout_secs}s,进程可能已挂)"))?
        .map(|_| ())
    }

    /// 按配置启动 server —— 统一入口,UI 手动启动与对话前懒启动都走这里。
    /// stdio 会把 `allowed_dirs` 追加为参数并注入 memory 环境变量;http 只校验端点与请求头。
    /// C3:`manual=true`(用户显式点击启用)才重置失败计数;懒启动/自动重启不清零,
    /// 否则每轮对话都会把计数抹掉,连续失败自动禁用永远不会触发。
    pub async fn start_from_config(
        &self,
        cfg: &McpServerConfig,
        data_dir: &std::path::Path,
        manual: bool,
    ) -> Result<(), String> {
        if manual {
            self.failure.clear(&cfg.name).await;
        }

        if cfg.is_http() {
            crate::mcp::safety::validate_server_url(&cfg.url)?;
            crate::mcp::safety::validate_headers(&cfg.headers)?;
            return self.start_http(&cfg.name, &cfg.url, &cfg.headers, cfg.is_preset).await;
        }

        crate::mcp::safety::validate_server_command(&cfg.command)?;
        // 文件系统类 server 把允许目录作为启动参数传入;
        // 未配置目录时传入本机全部磁盘根(不设沙箱);用户显式指定时只传这些目录,
        // 目录不存在时先创建,避免 server 因参数目录缺失起步失败
        let dirs = crate::mcp::config::launch_dirs(cfg);
        for d in &dirs {
            if !d.exists() {
                let _ = std::fs::create_dir_all(d);
            }
        }
        let dir_strings: Vec<String> = dirs.iter().map(|d| d.display().to_string()).collect();
        let mut args = cfg.args.clone();
        args.extend(dir_strings.iter().cloned());
        let mut env = crate::mcp::config::memory_env(&args, data_dir);
        // 用户显式配置的环境变量(如 TAVILY_API_KEY)优先;预置注入只补空缺
        for (k, v) in &cfg.env {
            env.retain(|(ek, _)| ek != k);
            env.push((k.clone(), v.clone()));
        }
        env.extend(crate::mcp::config::preset_env(&args, &cfg.env));
        // 国内镜像:npx 首次下载 npm 包走 npmmirror,uvx/pip 走清华 PyPI(默认开,设置页可关)
        if crate::mcp::load_or_init(data_dir).china_mirror {
            env.extend(crate::mcp::config::mirror_env(&cfg.command));
        }
        // P0b:原生二进制优先(默认关闭;未 opt-in 或未命中 manifest 时逐字节沿用 npx)
        let launch = crate::mcp::native::resolve(
            &cfg.name,
            &cfg.command,
            &args,
            &dir_strings,
            cfg.is_preset,
        );
        self.start_server(
            &cfg.name,
            &launch.command,
            &launch.args,
            &env,
            cfg.is_preset,
            manual,
        )
        .await
    }

    /// 启动远端 Streamable HTTP MCP server(无子进程,无本地执行面)。
    /// 幂等:已在运行且探活成功时跳过重连;连接已断才重连。
    pub async fn start_http(
        &self,
        name: &str,
        url: &str,
        headers: &HashMap<String, String>,
        is_preset: bool,
    ) -> Result<(), String> {
        // --- 第一段:幂等检查 ---
        {
            let exists = self.handles.lock().await.contains_key(name);
            if exists {
                if let Ok(()) = self.peek_alive(name, is_preset).await {
                    log::debug!("[MCP] http server `{name}` 已在运行,跳过启动");
                    return Ok(());
                }
                // stale:清理
                let mut lock = self.handles.lock().await;
                if let Some(old) = lock.remove(name) {
                    drop(old);
                    self.invalidate_tools_cache(name).await;
                    log::info!("[MCP] http server `{name}` 连接已断,清理旧连接后重启");
                }
            }
        }

        // 真正重连(不持锁)
        let runtime = crate::mcp::http::connect(name, url, headers).await?;

        let mut lock = self.handles.lock().await;
        lock.insert(
            name.to_string(),
            McpHandle {
                runtime: Arc::new(runtime),
                started_at: chrono::Local::now(),
            },
        );
        drop(lock);
        self.invalidate_tools_cache(name).await;

        log::info!("[MCP] http server `{name}` started ({url})");
        Ok(())
    }

    /// 列出指定 server 的所有工具。
    /// 只持锁取一份 Arc 快照,不持锁跨 await RPC —— 单个 server 无响应不能拖死整层 MCP。
    /// C4 超时分档:预置 server 用 PRESET_LIST_TOOLS_TIMEOUT_SECS,自定义 server 用 CUSTOM_LIST_TOOLS_TIMEOUT_SECS。
    pub async fn list_tools(&self, name: &str, is_preset: bool) -> Result<Vec<McpToolInfo>, String> {
        let service = {
            let lock = self.handles.lock().await;
            lock.get(name).map(|h| h.runtime.clone())
        }
        .ok_or_else(|| format!("server `{name}` 未启动"))?;

        let timeout_secs = if is_preset {
            PRESET_LIST_TOOLS_TIMEOUT_SECS
        } else {
            CUSTOM_LIST_TOOLS_TIMEOUT_SECS
        };

        let resp: ListToolsResult = timeout(
            Duration::from_secs(timeout_secs),
            service.peer().list_tools(None),
        )
        .await
        .map_err(|_| format!("list_tools `{name}` 超时({timeout_secs}s,进程可能已挂)"))?
        .map_err(|e| format!("list_tools `{name}` 失败: {e}"))?;

        Ok(resp.tools.into_iter().map(tool_to_info).collect())
    }

    /// 带缓存的 list_tools(P1-5):命中即回,未命中走真实 RPC 并回填缓存。
    /// 对话链路(收集工具定义/按工具名路由)走这里;探活(peek_alive)仍走实时 RPC。
    pub async fn list_tools_cached(&self, name: &str) -> Result<Vec<McpToolInfo>, String> {
        if let Some(tools) = self.tools_cache.lock().await.get(name) {
            return Ok(tools.clone());
        }
        let is_preset = crate::mcp::config::preset_servers()
            .iter()
            .any(|s| s.name == name);
        let tools = self.list_tools(name, is_preset).await?;
        self.tools_cache
            .lock()
            .await
            .insert(name.to_string(), tools.clone());
        Ok(tools)
    }

    /// 生命周期事件(启动/重启/停止)后调用:该 server 的工具清单缓存即刻作废。
    async fn invalidate_tools_cache(&self, name: &str) {
        self.tools_cache.lock().await.remove(name);
    }

    /// 按工具名反查所属 server —— ChatStream 只拿到工具名,需要路由到已启动的 server。
    /// 先收集名字列表再并行查询,避免持锁 await,也避免逐个串行把超时累加。
    pub async fn find_server_for_tool(&self, tool: &str) -> Option<String> {
        let names: Vec<String> = {
            let lock = self.handles.lock().await;
            lock.keys().cloned().collect()
        };
        if names.is_empty() {
            return None;
        }
        let lists = futures::future::join_all(names.iter().map(|n| self.list_tools_cached(n))).await;
        for (name, res) in names.iter().zip(lists) {
            if let Ok(tools) = res
                && tools.iter().any(|t| t.name == tool)
            {
                return Some(name.clone());
            }
        }
        None
    }

    /// 调用工具,上限 120s + 审计日志。
    /// 同样只持锁取 Arc 快照后立即释放:长调用(以及超时)期间不能让其他 MCP 操作排队。
    pub async fn call_tool(&self, call: McpToolCall, data_dir: PathBuf) -> McpToolResult {
        let service = {
            let lock = self.handles.lock().await;
            lock.get(&call.server_name).map(|h| h.runtime.clone())
        };
        let service = match service {
            Some(s) => s,
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
        let result = timeout(
            Duration::from_secs(CALL_TOOL_TIMEOUT_SECS),
            service.call_tool_once(params),
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
                error: Some(format!("调用超时({CALL_TOOL_TIMEOUT_SECS}s)")),
            },
        }
    }

    /// 停止并移除指定 server —— 移除 handle 后 Drop 会 kill 子进程(同 stop_all)。
    /// 返回是否有运行中的实例被停止。
    pub async fn stop_server(&self, name: &str) -> bool {
        let mut lock = self.handles.lock().await;
        let stopped = lock.remove(name).is_some();
        drop(lock);
        if stopped {
            self.invalidate_tools_cache(name).await;
            log::info!("[MCP] server `{name}` stopped");
        }
        stopped
    }

    /// 停止所有 server —— app 退出时调,kill 所有子进程。
    #[allow(dead_code)] // Phase 5e ChatStream 退出时调
    pub async fn stop_all(&self) {
        let mut lock = self.handles.lock().await;
        let names: Vec<_> = lock.keys().cloned().collect();
        lock.clear();
        drop(lock);
        self.tools_cache.lock().await.clear();
        for n in names {
            log::info!("[MCP] server `{n}` stopped");
        }
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
            .start_server(name, "node", &[script.to_string_lossy().to_string()], &[], false, true)
            .await
            .expect("MCP server 启动失败");

        let tools = runtime.list_tools(name, false).await.expect("tools/list 失败");
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
