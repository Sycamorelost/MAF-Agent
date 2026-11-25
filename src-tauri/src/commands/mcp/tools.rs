//! 工具定义下发:按会话历史与完全信任模式选择内置 / MCP 工具。

use super::McpState;
use crate::mcp::{self, McpRuntime, McpServerConfig};
use crate::paths::data_dir as data_dir_fn;
use tauri::State;

/// 查询某会话历史里已调用过的工具名集合(tool_call_log)。
/// 用于「按需激活」:没调过的工具不下发,省 token。
fn history_tool_names(session_id: Option<&str>) -> std::collections::HashSet<String> {
    let Some(sid) = session_id else {
        return std::collections::HashSet::new();
    };
    let Ok(conn) = crate::db::conn().lock() else {
        return std::collections::HashSet::new();
    };
    let mut stmt =
        match conn.prepare("SELECT DISTINCT tool_name FROM tool_call_log WHERE session_id = ?1") {
            Ok(s) => s,
            Err(_) => return std::collections::HashSet::new(),
        };
    match stmt.query_map([sid], |r| r.get::<_, String>(0)) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => std::collections::HashSet::new(),
    }
}

/// 汇总内置工具与已启动 MCP server 的工具,转成 OpenAI function-calling 定义。
/// R4/D2:只读内置工具默认下发(长文档 doc_search 不再依赖工具总开关);
/// 写入/执行类工具与 MCP server 工具仍需打开总开关。
///
/// P1-4:工具按「高频/必用」与「按需」分层 —— 后者仅当会话显式加载(tool_load)
/// 或历史调用过时才下发,避免简单问答白白消耗 7000+ tokens 在没用的 tool spec 上。
#[tauri::command]
pub async fn mcp_collect_tools(
    runtime: State<'_, McpRuntime>,
    state: State<'_, McpState>,
    session_id: Option<String>,
) -> Result<Vec<crate::provider::ToolDef>, String> {
    let dir = data_dir_fn();
    let (tools_enabled, builtin_enabled) = state
        .0
        .lock()
        .map(|c| (c.tools_enabled, c.builtin_tools))
        .unwrap_or((false, false));
    let policy = mcp::approval::load_policy(&dir);
    let full_trust = mcp::approval::effective_mode(&policy, &dir, session_id.as_deref())
        == mcp::ApprovalMode::Full;

    // 按需激活:查 session 历史里调过的工具名
    let history_names = history_tool_names(session_id.as_deref());

    let mut out: Vec<crate::provider::ToolDef> = Vec::new();

    // 1) 内置工具:核心集常驻;其余按会话已加载 / 关键词预取 / 智能体白名单按需下发,
    //    避免每轮把全部工具定义(约占输入 token 80%)重复塞给模型。
    if builtin_enabled {
        out.extend(mcp::on_demand::collect_session_defs(
            session_id.as_deref(),
            tools_enabled,
        ));
    }

    // 2) 完全信任模式:GUI 自动化工具(ui_*)也下发
    if full_trust {
        out.extend(crate::gui::tools::tool_defs());
    }

    // 总开关关闭:不再下发 MCP server 工具
    if !tools_enabled {
        return Ok(out);
    }
    // 只收集「已启用」配置里的 server,防止用户停用后仍被调用
    let servers: Vec<McpServerConfig> = state
        .0
        .lock()
        .map(|c| {
            c.servers
                .iter()
                .filter(|s| {
                    if !s.enabled {
                        return false;
                    }
                    // 安全:配置被手工改成 shell 包装器或非法 URL 时,同样拒绝启动
                    let safe = if s.is_http() {
                        crate::mcp::safety::validate_server_url(&s.url)
                            .and_then(|_| crate::mcp::safety::validate_headers(&s.headers))
                    } else {
                        crate::mcp::safety::validate_server_command(&s.command)
                    };
                    match safe {
                        Ok(()) => true,
                        Err(e) => {
                            log::warn!("跳过不安全的 MCP server `{}`: {e}", s.name);
                            false
                        }
                    }
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    // P1-5:并行收集各 server 工具定义 —— 此前为串行循环,单个 server 的 list_tools
    // 超时(10s)会逐个累加,直接表现为发送后数秒空窗;启动预热后此处基本全部命中缓存。
    let per_server = futures::future::join_all(servers.iter().map(|server| async {
        let name = server.name.clone();
        // 懒启动:应用启动时已统一拉起过所有 enabled server,
        // 这里只在 handle 不存在或已 stale(进程死了)时才启动/重启一次;
        // 幂等保护在 start_server 内部:健康运行的 server 不会被 kill+respawn。
        match runtime.list_tools_cached(&name).await {
            Ok(t) => t,
            Err(_) => {
                let was_running = runtime.is_running(&name).await;
                match runtime.start_from_config(server, &dir, false).await {
                    Ok(()) => match runtime.list_tools_cached(&name).await {
                        Ok(t) => t,
                        Err(e) => {
                            log::warn!("重启后仍 list_tools 失败 `{name}`: {e}");
                            Vec::new()
                        }
                    },
                    Err(e) => {
                        log::warn!(
                            "{} MCP server `{name}` 失败: {e}",
                            if was_running {
                                "stale 重启"
                            } else {
                                "懒启动"
                            }
                        );
                        Vec::new()
                    }
                }
            }
        }
    }))
    .await;
    // 会话级「按工具」激活状态(P1-4):
    //   - `mcp:{server}`  → 整台 server 已由 tool_load 加载;
    //   - `mcp__{server}__{tool}` → 单个工具已加载;
    //   - tool_call_log 里出现过的名字(旧日志为裸名)同样视为已用。
    let activated: std::collections::HashSet<String> = session_id
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| crate::db::session_tools::list(s).into_iter().collect())
        .unwrap_or_default();
    let activated_servers: std::collections::HashSet<&str> = activated
        .iter()
        .filter_map(|s| s.strip_prefix("mcp:"))
        .collect();

    // 紧凑目录(server 名 + 工具数):只用于告诉模型「可以加载什么」,不展开 spec。
    let catalog: Vec<String> = servers
        .iter()
        .zip(&per_server)
        .filter(|(_, ts)| !ts.is_empty())
        .map(|(s, ts)| format!("{} ({} 个工具)", s.name, ts.len()))
        .collect();

    // P1-4:真正的 per-tool 下发 —— 早期实现「用过该 server 任一工具 → 整台全量下发」,
    // 一次调用就会让 30+ 个无关工具常驻上下文;现在只下发「已加载 / 已用过」的单工具,
    // 外加小 server / 核心只读子集作为首次发现的引导集。
    let core_keywords = [
        "read", "list", "search", "get", "find", "think", "fetch", "extract",
    ];
    for (server_cfg, tools) in servers.iter().zip(per_server) {
        if tools.is_empty() {
            continue;
        }
        let whole_server_loaded = activated_servers.contains(server_cfg.name.as_str());
        // 引导集:小 server 全给;大 server 只给核心只读子集(启发式未命中兜底前 3 个)
        let bootstrap: Vec<&mcp::mcp_call::McpToolInfo> = if tools.len() <= 3 {
            tools.iter().collect()
        } else {
            let core: Vec<&mcp::mcp_call::McpToolInfo> = tools
                .iter()
                .filter(|t| {
                    let lower = t.name.to_lowercase();
                    core_keywords.iter().any(|kw| lower.contains(kw))
                })
                .collect();
            if core.is_empty() {
                tools.iter().take(3).collect()
            } else {
                core
            }
        };
        let bootstrap_names: std::collections::HashSet<&str> =
            bootstrap.iter().map(|t| t.name.as_str()).collect();

        let mut selected: Vec<&mcp::mcp_call::McpToolInfo> = Vec::new();
        for t in &tools {
            let qualified = mcp::namespace::qualify(&server_cfg.name, &t.name);
            let loaded = whole_server_loaded
                || activated.contains(&qualified)
                || activated.contains(&t.name)
                || history_names.contains(&qualified)
                || history_names.contains(&t.name);
            if loaded || bootstrap_names.contains(t.name.as_str()) {
                selected.push(t);
            }
        }

        log::debug!(
            "[MCP] server=`{}` total={} selected={} whole_loaded={}",
            server_cfg.name,
            tools.len(),
            selected.len(),
            whole_server_loaded
        );

        for t in selected {
            let params = if t.input_schema.is_null() {
                serde_json::json!({ "type": "object", "properties": {} })
            } else {
                t.input_schema.clone()
            };
            out.push(crate::provider::ToolDef {
                r#type: "function".into(),
                function: crate::provider::ToolFunctionDef {
                    // P1-4:跨 server 同名工具统一加命名空间,路由不再靠迭代顺序
                    name: mcp::namespace::qualify(&server_cfg.name, &t.name),
                    description: if t.description.is_empty() {
                        None
                    } else {
                        Some(t.description.clone())
                    },
                    parameters: Some(params),
                },
            });
        }
    }
    // 把紧凑目录附到 tool_load 描述上:模型先加载再调用,避免一次塞满全部 spec
    if !catalog.is_empty()
        && let Some(tl) = out.iter_mut().find(|d| d.function.name == "tool_load")
        && let Some(desc) = tl.function.description.as_mut()
    {
        desc.push_str(&format!(
            "\n外部工具服务器:{}。用 \"mcp:<server>\" 加载整台,或 \"mcp__<server>__<tool>\" 精确加载。",
            catalog.join("、")
        ));
    }
    Ok(out)
}
