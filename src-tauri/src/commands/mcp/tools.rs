//! 工具定义下发:按会话历史与完全信任模式选择内置 / MCP 工具。

use super::McpState;
use crate::mcp::{self, McpRuntime, McpServerConfig};
use crate::paths::data_dir as data_dir_fn;
use tauri::State;

/// 查询某会话历史里已调用过的工具名集合(tool_call_log)。
/// 用于「按需激活」:没调过的工具不下发,省 token。
fn history_tool_names(session_id: Option<&str>) -> std::collections::HashSet<String> {
    let Some(sid) = session_id else { return std::collections::HashSet::new() };
    let Ok(conn) = crate::db::conn().lock() else { return std::collections::HashSet::new() };
    let mut stmt = match conn.prepare(
        "SELECT DISTINCT tool_name FROM tool_call_log WHERE session_id = ?1",
    ) {
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
/// P5/D18:按会话生效预设的 tools.include/exclude 裁剪(内置与 MCP 工具共用同一名单);
/// session_id 为空或会话不存在时不过滤(保持既有调用方行为)。
///
/// P7/token 解耦:工具按「高频/必用」与「低频/按需」分层,前者每轮都下发,
/// 后者仅当 session 历史里有过调用记录时才下发,避免简单问答场景白白消耗
/// 7000+ tokens 在没用上的 tool spec JSON 上。
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

    // 1) 只读内置工具(get_now / todo / sys / fs 读 / file_extract / doc_search / doc_read …)
    //    默认可用:它们不改变磁盘与系统状态,且是长文档分析的前置能力。
    if builtin_enabled {
        out.extend(crate::mcp::tool_spec::readonly_tool_defs());
    }

    // 2) 完全信任模式:GUI 自动化工具(ui_*)也下发
    if full_trust {
        out.extend(crate::gui::tools::tool_defs());
    }

    // 3) 写入/执行类内置工具 —— 高频始终下发,低频按需激活
    //    高频(始终):shell_exec(79×) / fs_write(46×) / fs_edit(19×) / 分块三件套
    //    docx 属核心交付能力且新会话无历史,必须始终下发;xlsx 仍按需
    //    低频(按需):fs_delete / fs_export_xlsx / fs_restore / zip_extract
    if builtin_enabled {
        let write_all = crate::mcp::tool_spec::write_tool_defs();
        let always = ["shell_exec", "fs_write", "fs_edit", "fs_write_begin", "fs_write_chunk", "fs_write_finish", "fs_export_docx", "image_generate", "slides_export"];
        out.extend(write_all.iter().filter(|t| always.contains(&t.function.name.as_str())).cloned());
        // 低频:session 历史里调过任意低频 write 才下发全部低频
        let optional = ["fs_delete", "fs_export_xlsx", "fs_restore", "zip_extract"];
        if !history_names.is_empty() && optional.iter().any(|n| history_names.contains(*n)) {
            out.extend(write_all.iter().filter(|t| optional.contains(&t.function.name.as_str())).cloned());
        }
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
    // P7/token 解耦:按 server 粒度过滤 —— 历史用过该 server 任意工具 → 全部下发;
    // 否则只下发核心只读子集(启发式:read/list/search/get/find/think 关键词匹配),
    // 避免 write/delete/export 等低频工具白白消耗 tokens。
    let server_core_keywords = ["read", "list", "search", "get", "find", "think", "fetch", "extract"];
    for (server_cfg, tools) in servers.iter().zip(per_server) {
        if tools.is_empty() {
            continue;
        }
        let tool_names: std::collections::HashSet<&str> =
            tools.iter().map(|t| t.name.as_str()).collect();
        let server_used = !history_names.is_empty()
            && tool_names.iter().any(|n| history_names.contains(*n));

        let selected: Vec<&mcp::mcp_call::McpToolInfo> = if server_used {
            // 历史已调过 → 全部下发(用户已经在用,不需要再切)
            tools.iter().collect()
        } else if tools.len() <= 3 {
            // 小 server(≤3 工具)没必要再切
            tools.iter().collect()
        } else {
            // 大 server 首次启用:只下发核心只读子集
            let core: Vec<&mcp::mcp_call::McpToolInfo> = tools
                .iter()
                .filter(|t| {
                    let lower = t.name.to_lowercase();
                    server_core_keywords.iter().any(|kw| lower.contains(kw))
                })
                .collect();
            if core.is_empty() {
                // 启发式没命中 → 兜底取前 3 个(通常是最常用的)
                tools.iter().take(3).collect()
            } else {
                core
            }
        };

        log::debug!(
            "[MCP] server=`{}` total={} selected={} used_by_history={}",
            server_cfg.name,
            tools.len(),
            selected.len(),
            server_used
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
                    name: t.name.clone(),
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
    // P5 / D18:按会话生效预设裁剪(钉住 > Agent 模板 > 全局默认)。
    if let Some(sid) = session_id.as_deref()
        && let Ok(Some(s)) = crate::db::sessions::get(sid)
    {
        let info = super::super::presets::resolve_for(&s);
        out = crate::presets::resolve::filter_defs(&info.preset.tools, out, |t| {
            t.function.name.as_str()
        });
    }
    Ok(out)
}
