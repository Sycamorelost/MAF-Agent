//! 启动入口:幂等检查、并发串行化、失败退避,以及按配置拼装启动参数。

use super::McpRuntime;
use crate::mcp::config::McpServerConfig;
use tokio::time::Duration;

impl McpRuntime {
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
            let count = *self
                .failure
                .failure_counts
                .lock()
                .await
                .get(name)
                .unwrap_or(&0);
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
            return self
                .start_http(&cfg.name, &cfg.url, &cfg.headers, cfg.is_preset)
                .await;
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
}
