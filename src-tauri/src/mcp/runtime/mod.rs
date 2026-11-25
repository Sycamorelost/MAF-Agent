//! MCP Runtime:spawn stdio MCP server,list_tools / call_tool,Drop 时 kill 子进程。
//!
//! 分层:
//! - 本文件:运行时状态(`McpRuntime`)与查询/调用/停止等无启动副作用的方法;
//! - `handshake.rs`:真正 spawn 子进程 / 建立 http 连接并握手;
//! - `lifecycle.rs`:启动(含懒启动、幂等、失败退避)入口。
//!
//! rmcp 3.7.0 真实签名:
//!   .serve() → RunningService<RoleClient, ClientConfig>
//!   .peer()  → Peer<RoleClient>
//!   .service_client() → ClientTransport<RoleClient>
//!   CallToolResponse { Complete(CallToolResult), InputRequired, Task }  — 3.7.0 新增了 Response 枚举包装
//!   Tool.input_schema: Arc<serde_json::Map>
//!   Tool.name: Cow<'_, str>

mod handshake;
mod lifecycle;

#[cfg(test)]
mod tests;

use crate::mcp::mcp_call::{
    McpToolCall, McpToolInfo, McpToolResult, handle_response, tool_to_info,
};
use rmcp::model::{CallToolRequestParams, ListToolsResult};
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
pub(crate) struct McpHandle {
    /// 用 Arc 共享:RPC 时只克隆指针、立刻释放 `handles` 锁,
    /// 避免持锁跨 await(一个 server 无响应会拖死整层 MCP)。
    pub(crate) runtime:
        Arc<rmcp::service::RunningService<rmcp::RoleClient, rmcp::model::ClientConfig>>,
    pub(crate) started_at: chrono::DateTime<chrono::Local>,
}

impl McpRuntime {
    /// 查询指定 server 是否已启动(仅检查 handle 存在,不做 RPC 探活,低开销)。
    pub async fn is_running(&self, name: &str) -> bool {
        self.handles.lock().await.contains_key(name)
    }

    /// 列出指定 server 的所有工具。
    /// 只持锁取一份 Arc 快照,不持锁跨 await RPC —— 单个 server 无响应不能拖死整层 MCP。
    /// C4 超时分档:预置 server 用 PRESET_LIST_TOOLS_TIMEOUT_SECS,自定义 server 用 CUSTOM_LIST_TOOLS_TIMEOUT_SECS。
    pub async fn list_tools(
        &self,
        name: &str,
        is_preset: bool,
    ) -> Result<Vec<McpToolInfo>, String> {
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
    pub(crate) async fn invalidate_tools_cache(&self, name: &str) {
        self.tools_cache.lock().await.remove(name);
    }

    /// 按工具名反查所属 server —— ChatStream 只拿到工具名,需要路由到已启动的 server。
    /// 先收集名字列表再并行查询,避免持锁 await,也避免逐个串行把超时累加。
    pub async fn find_server_for_tool(&self, tool: &str) -> Option<String> {
        self.resolve_tool(tool).await.map(|(server, _)| server)
    }

    /// 解析模型调用的工具名 → (server, 原始工具名)。
    ///
    /// 同时接受两种写法:命名空间名 `mcp__{server}__{tool}`(新,首选)与裸名
    /// (旧对话/兼容)。候选 server 按名称排序,结果确定;不再依赖 HashMap 迭代顺序。
    pub async fn resolve_tool(&self, called: &str) -> Option<(String, String)> {
        let mut names: Vec<String> = {
            let lock = self.handles.lock().await;
            lock.keys().cloned().collect()
        };
        if names.is_empty() {
            return None;
        }
        names.sort();
        let lists =
            futures::future::join_all(names.iter().map(|n| self.list_tools_cached(n))).await;
        // 1) 原始名精确匹配(兼容旧调用),2) 命名空间下发展名匹配
        for (name, res) in names.iter().zip(&lists) {
            if let Ok(tools) = res
                && let Some(t) = tools.iter().find(|t| t.name == called)
            {
                return Some((name.clone(), t.name.clone()));
            }
        }
        for (name, res) in names.iter().zip(lists) {
            if let Ok(tools) = res
                && let Some(t) = tools
                    .iter()
                    .find(|t| crate::mcp::namespace::qualify(name, &t.name) == called)
            {
                return Some((name.clone(), t.name.clone()));
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
            Ok(Ok(resp)) => handle_response(resp, &data_dir),
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

#[cfg(all(test, windows))]
mod win_tests;
