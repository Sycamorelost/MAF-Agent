//! 内置工具按需接入。
//!
//! 目标:每轮只下发「核心工具 + 本会话已加载工具 + 当前消息相关分组」,
//! 而不是把 30+ 个工具定义全量塞给模型(约占输入 token 的 80%)。
//!
//! 加载路径有三条:
//! 1. 核心集:读写文件、shell、时间、任务清单、交付卡,几乎每轮都会用到;
//! 2. 关键词预取:按最近一条用户消息与附件命中分组,省掉一次额外工具轮;
//! 3. 显式加载:模型调用 `tool_load` 按组名/工具名加载,写入 `session_tools`
//!    后本轮立即生效,并在本会话后续对话中保持。

use std::collections::HashSet;

use serde_json::Value;

use crate::provider::ToolDef;

/// 每轮常驻的核心工具。
const CORE: &[&str] = &[
    "get_now",
    "present",
    "todo_write",
    "todo_read",
    "fs_list",
    "fs_read",
    "fs_grep",
    "fs_write",
    "fs_edit",
    "shell_exec",
];

/// 按需分组:组名 → 工具名。`tool_load` 的参数可直接用组名。
const GROUPS: &[(&str, &[&str])] = &[
    (
        "doc",
        &["doc_search", "doc_read", "doc_list", "file_extract"],
    ),
    ("docx", &["doc_generate", "fs_export_docx"]),
    ("sheet", &["fs_export_xlsx"]),
    ("slides", &["slides_export"]),
    (
        "media",
        &["image_generate", "image_compose", "diagram_export", "ocr_extract"],
    ),
    (
        "batch",
        &["fs_write_begin", "fs_write_chunk", "fs_write_finish"],
    ),
    ("files", &["fs_delete", "fs_restore", "zip_list", "zip_extract"]),
    (
        "sys",
        &["sys_overview", "sys_processes", "sys_disks", "sys_networks"],
    ),
    ("web", &["web_fetch"]),
    ("agent", &["delegate"]),
];

/// 关键词 → 分组。只做「预取」,漏判时模型仍可通过 `tool_load` 补齐。
const TRIGGERS: &[(&str, &[&str])] = &[
    (
        "doc",
        &["附件", "上传的文档", "pdf", "这个文件", "文档里", "资料"],
    ),
    (
        "docx",
        &[
            "报告", "论文", "总结", "方案", "合同", "说明书", "白皮书", "手册", "纪要", "简历",
            "写作", "word", "docx", "文档",
        ],
    ),
    (
        "sheet",
        &["表格", "excel", "xlsx", "csv", "报表", "数据表", "统计表"],
    ),
    ("slides", &["ppt", "演示", "幻灯片", "汇报", "路演", "答辩"]),
    (
        "media",
        &[
            "图片", "配图", "海报", "封面", "流程图", "架构图", "思维导图", "ocr", "识别文字",
            "截图",
        ],
    ),
    ("sys", &["进程", "内存", "磁盘", "系统信息", "cpu", "网络接口"]),
    ("files", &["压缩", "解压", "zip", "删除文件", "恢复文件"]),
    ("web", &["网址", "链接", "网页", "http://", "https://"]),
    ("agent", &["委派", "子智能体"]),
    ("batch", &["大文件", "几十万", "分批写"]),
];

/// 全部内置工具定义(含 `tool_load` 自身)。
pub(crate) fn all_builtin_defs() -> Vec<ToolDef> {
    let mut defs = crate::mcp::tool_spec::readonly_tool_defs();
    defs.extend(crate::mcp::tool_spec::write_tool_defs());
    defs
}

/// 某会话首轮应下发的内置工具。
///
/// `allow_write = false`(写入/执行开关关闭)时退回只读工具全集,与旧行为一致;
/// 否则核心集 + 已加载 + 历史用过 + 关键词预取按需下发。
/// 智能体工具白名单存在时直接按白名单下发(白名单本身就是精选集,不再二次裁剪)。
pub fn collect_session_defs(session_id: Option<&str>, allow_write: bool) -> Vec<ToolDef> {
    if !allow_write {
        return crate::mcp::tool_spec::readonly_tool_defs()
            .into_iter()
            .filter(|d| d.function.name != "tool_load")
            .collect();
    }
    let all = all_builtin_defs();
    if let Some(policy) = session_id.and_then(agent_policy) {
        return all
            .into_iter()
            .filter(|d| policy.contains(&d.function.name))
            .collect();
    }
    let mut names: HashSet<String> = CORE.iter().map(|s| s.to_string()).collect();
    names.insert("tool_load".to_string());
    if let Some(sid) = session_id.filter(|s| !s.trim().is_empty()) {
        names.extend(crate::db::session_tools::list(sid));
        // 本会话实际调用过的工具保持可用,避免追问时工具突然消失
        names.extend(used_tool_names(sid));
        names.extend(heuristic_names(&last_user_text(sid)));
        if web_enabled(sid) {
            names.insert("web_fetch".to_string());
        }
    }
    all.into_iter()
        .filter(|d| names.contains(&d.function.name))
        .collect()
}

/// 工具轮内被 `tool_load` 新加载的工具:追加到本轮请求,下一轮即可调用。
pub fn sync_tools(tools: &mut Option<Vec<ToolDef>>, session_id: Option<&str>) {
    let Some(sid) = session_id.filter(|s| !s.trim().is_empty()) else {
        return;
    };
    // 白名单模式首轮已完整下发,无需增量
    if agent_policy(sid).is_some() {
        return;
    }
    let activated: HashSet<String> = crate::db::session_tools::list(sid).into_iter().collect();
    if activated.is_empty() {
        return;
    }
    let defs = tools.get_or_insert_with(Vec::new);
    let have: HashSet<String> = defs.iter().map(|d| d.function.name.clone()).collect();
    // 写入/执行开关关闭时,请求里不会有 fs_write / shell_exec;此时只补充只读工具
    let write_allowed = have.contains("fs_write") || have.contains("shell_exec");
    let readonly: HashSet<String> = crate::mcp::tool_spec::readonly_tool_defs()
        .into_iter()
        .map(|d| d.function.name)
        .collect();
    for d in all_builtin_defs() {
        if !activated.contains(&d.function.name) || have.contains(&d.function.name) {
            continue;
        }
        if !write_allowed && !readonly.contains(&d.function.name) {
            continue;
        }
        defs.push(d);
    }
}

/// `tool_load` 执行入口:解析组名/工具名 → 策略过滤 → 持久化。
pub fn exec_load(args: &Value, session_id: Option<&str>) -> (String, bool) {
    let Some(sid) = session_id.map(str::trim).filter(|s| !s.is_empty()) else {
        return ("加载工具失败:缺少会话上下文。".into(), false);
    };
    let Some(items) = args.get("tools").and_then(|v| v.as_array()) else {
        return ("加载工具失败:tools 必须是字符串数组。".into(), false);
    };
    let known: HashSet<String> = all_builtin_defs()
        .into_iter()
        .map(|d| d.function.name)
        .collect();
    // P1-4:外部 MCP server 也走同一按需加载通道(整台 `mcp:<server>` 或单工具命名空间名)
    let mcp_servers = enabled_mcp_servers();
    let mcp_prefixes: Vec<String> = mcp_servers
        .iter()
        .map(|s| crate::mcp::namespace::server_prefix(s))
        .collect();
    let mut wanted: Vec<String> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();
    for item in items {
        let Some(raw) = item.as_str().map(str::trim).filter(|s| !s.is_empty()) else {
            continue;
        };
        if let Some((_, names)) = GROUPS.iter().find(|(g, _)| *g == raw) {
            wanted.extend(names.iter().map(|s| s.to_string()));
        } else if let Some(server) = raw.strip_prefix("mcp:") {
            if mcp_servers.iter().any(|s| s == server.trim()) {
                wanted.push(format!("mcp:{}", server.trim()));
            } else {
                unknown.push(raw.to_string());
            }
        } else if raw.starts_with(crate::mcp::namespace::PREFIX) {
            if mcp_prefixes.iter().any(|p| raw.starts_with(p.as_str())) {
                wanted.push(raw.to_string());
            } else {
                unknown.push(raw.to_string());
            }
        } else if known.contains(raw) && raw != "tool_load" {
            wanted.push(raw.to_string());
        } else {
            unknown.push(raw.to_string());
        }
    }
    wanted.sort();
    wanted.dedup();
    if let Some(policy) = agent_policy(sid) {
        wanted.retain(|n| policy.contains(n));
    }
    if wanted.is_empty() {
        return ("没有可加载的工具:请用组名(如 docx/sheet/media/slides)或已存在的工具名。".into(), false);
    }
    crate::db::session_tools::add_many(sid, &wanted);
    let mut msg = format!(
        "已加载 {} 个工具:{},本轮后续可直接调用。",
        wanted.len(),
        wanted.join(", ")
    );
    if !unknown.is_empty() {
        msg.push_str(&format!("\n未识别(已忽略):{}", unknown.join(", ")));
    }
    (msg, true)
}

/// 当前配置里已启用的 MCP server 名(供 tool_load 校验 `mcp:<server>` / 命名空间名)。
fn enabled_mcp_servers() -> Vec<String> {
    let mut names: Vec<String> = crate::mcp::load_or_init(&crate::paths::data_dir())
        .servers
        .into_iter()
        .filter(|s| s.enabled)
        .map(|s| s.name)
        .collect();
    names.sort();
    names.dedup();
    names
}

/// 关键词预取:命中分组即返回该组全部工具名。
fn heuristic_names(text: &str) -> HashSet<String> {
    let lower = text.to_lowercase();
    let mut names = HashSet::new();
    for (group, keywords) in TRIGGERS {
        if keywords.iter().any(|k| lower.contains(k)) {
            if let Some((_, tools)) = GROUPS.iter().find(|(g, _)| g == group) {
                names.extend(tools.iter().map(|s| s.to_string()));
            }
        }
    }
    names
}

/// 最近一条用户消息(含附件标记),用于关键词预取。
fn last_user_text(session_id: &str) -> String {
    if !crate::db::is_initialized() {
        return String::new();
    }
    let Ok(conn) = crate::db::conn().lock() else {
        return String::new();
    };
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT content, attachments FROM messages \
             WHERE session_id=?1 AND role='user' ORDER BY id DESC LIMIT 1",
            rusqlite::params![session_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let (mut text, attachments) = row.unwrap_or_default();
    if attachments.is_some_and(|a| !a.trim().is_empty() && a.trim() != "[]") {
        text.push_str(" 附件");
    }
    text
}

/// 本会话历史调用过的工具名(tool_call_log),用于保持续聊时的工具连续性。
fn used_tool_names(session_id: &str) -> HashSet<String> {
    if !crate::db::is_initialized() {
        return HashSet::new();
    }
    let Ok(conn) = crate::db::conn().lock() else {
        return HashSet::new();
    };
    let Ok(mut stmt) =
        conn.prepare("SELECT DISTINCT tool_name FROM tool_call_log WHERE session_id=?1")
    else {
        return HashSet::new();
    };
    match stmt.query_map(rusqlite::params![session_id], |r| r.get::<_, String>(0)) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => HashSet::new(),
    }
}

/// 会话是否开启联网搜索(开启时预取 web_fetch)。
fn web_enabled(session_id: &str) -> bool {
    crate::db::sessions::get(session_id)
        .ok()
        .flatten()
        .is_some_and(|s| s.web_search_enabled)
}

/// 会话智能体声明的工具白名单:None = 未限制;Some = 仅允许名单内工具。
fn agent_policy(session_id: &str) -> Option<HashSet<String>> {
    let s = crate::db::sessions::get(session_id).ok().flatten()?;
    let agent_id = s.agent_id.filter(|a| !a.trim().is_empty())?;
    crate::agents::find(&agent_id)?
        .tool_policy
        .map(|v| v.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_cover_every_optional_tool_once() {
        let all: HashSet<String> = all_builtin_defs()
            .into_iter()
            .map(|d| d.function.name)
            .collect();
        let mut grouped = HashSet::new();
        for (group, names) in GROUPS {
            for name in *names {
                assert!(all.contains(*name), "分组 `{group}` 含未知工具 `{name}`");
                assert!(grouped.insert(*name), "工具 `{name}` 在多个分组中重复");
            }
        }
        let expected: HashSet<&str> = all
            .iter()
            .filter(|n| !CORE.contains(&n.as_str()) && n.as_str() != "tool_load")
            .map(|n| n.as_str())
            .collect();
        assert_eq!(grouped, expected, "分组必须恰好覆盖核心集之外的工具体");
    }

    #[test]
    fn core_set_stays_small() {
        assert!(CORE.len() <= 12, "核心集过大:{}", CORE.len());
        for name in CORE {
            assert!(
                all_builtin_defs().iter().any(|d| d.function.name == *name),
                "核心工具 `{name}` 不存在"
            );
        }
    }

    #[test]
    fn core_specs_stay_within_token_budget() {
        // P2:核心常驻工具集必须足够轻,否则每轮都会挤占上下文预算。
        let core = collect_session_defs(None, true);
        assert!(!core.is_empty(), "核心工具集不应为空");
        let tok = |defs: &[ToolDef]| {
            crate::commands::provider::context_budget::tools_tokens(defs, Some("deepseek-flash"))
        };
        let core_tok = tok(&core);
        assert!(
            core_tok <= 3000,
            "核心工具 spec 共 {core_tok} tokens,超过 3000 上限"
        );

        // 按需下发必须显著小于内置工具全集(否则「按需加载」名存实亡)。
        let all_tok = tok(&all_builtin_defs());
        assert!(
            core_tok * 2 < all_tok,
            "核心集({core_tok})应远小于全集({all_tok})"
        );
    }

    #[test]
    fn heuristics_pick_expected_groups() {
        let slides = heuristic_names("帮我做一个产品发布会 PPT");
        assert!(slides.contains("slides_export"));
        let export = heuristic_names("把数据导出成 Excel 表格");
        assert!(export.contains("fs_export_xlsx"));
        let doc = heuristic_names("看一下我上传的附件");
        assert!(doc.contains("doc_search"));
        let web = heuristic_names("打开这个网页 https://example.com");
        assert!(web.contains("web_fetch"));
        assert!(heuristic_names("你好").is_empty());
    }
}
