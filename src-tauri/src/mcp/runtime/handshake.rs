//! spawn 子进程 / 建立 http 连接并完成 MCP 握手;探活 RPC。

use super::{
    CUSTOM_PEEK_ALIVE_TIMEOUT_SECS, CUSTOM_STARTUP_TIMEOUT_SECS, McpHandle, McpRuntime,
    PRESET_PEEK_ALIVE_TIMEOUT_SECS, PRESET_STARTUP_TIMEOUT_SECS,
};
use rmcp::ServiceExt;
use rmcp::model::{ClientCapabilities, ClientConfig, Implementation};
use rmcp::transport::TokioChildProcess;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{Duration, timeout};

impl McpRuntime {
    /// 真正 spawn 子进程并握手(不持任何全局锁)。
    ///
    /// C6 环境变量 scrub:移除包含 KEY/PASSWORD/SECRET/TOKEN 的变量与 `MYAGENT_*`,
    /// 借鉴 DSH:MCP 子进程不应继承宿主的 API key / token 等敏感信息。
    ///
    /// 注意:不能改用 `.env_clear()` —— Windows 子进程缺少 `SystemRoot` / `PATHEXT` /
    /// `COMSPEC` 等基础变量时,npx / node / cmd 会直接启动失败。
    /// C1 PATH 补全:给子进程注入增强 PATH,包含 homebrew / ~/.local/bin / ~/.cargo/bin 等。
    /// C4 超时分档:预置 server 用短超时(15s),自定义 server 保守给 120s。
    pub(super) async fn spawn_handle(
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

        let runtime = timeout(
            Duration::from_secs(startup_secs),
            client_info.serve(transport),
        )
        .await
        .map_err(|_| format!("启动 `{name}` 超时({startup_secs}s):npx 首次下载较慢或网络不可达"))?
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
}
