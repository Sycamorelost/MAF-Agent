//! 系统只读快照(sys_*)与网页正文抓取(web_fetch)。
//!
//! 两者都只读、免审批:系统快照隔离到阻塞线程池(CPU 采样约 200ms);
//! 网页抓取的安全边界(SSRF / robots / 限长)在 `websearch::page` 内实现。

use serde_json::Value;

/// 本模块负责的工具名。
pub(super) fn handles(name: &str) -> bool {
    name.starts_with("sys_") || name == "web_fetch"
}

pub(super) async fn exec(name: &str, args: &Value, session_id: Option<&str>) -> (String, bool) {
    match name {
        "web_fetch" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("").trim();
            if url.is_empty() {
                return ("缺少 url 参数(需完整的 http/https 链接)".into(), false);
            }
            let max = args
                .get("max_chars")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            match crate::websearch::page::fetch_page(url, max).await {
                Ok(page) => {
                    // D7:抓取事件落库(地址/标题/正文字数),便于排查「模型看了哪个页面」
                    crate::db::events::append(
                        session_id,
                        "web_fetch",
                        &serde_json::json!({
                            "url": page.url,
                            "title": page.title,
                            "chars": page.total_chars,
                            "truncated": page.truncated,
                        })
                        .to_string(),
                    );
                    (crate::websearch::page::render_for_model(&page), true)
                }
                Err(e) => {
                    crate::db::events::append(
                        session_id,
                        "web_fetch",
                        &serde_json::json!({ "url": url, "error": e }).to_string(),
                    );
                    (format!("抓取失败:{e}"), false)
                }
            }
        }
        // G5 系统状态快照:只读免审批;CPU 采样含约 200ms 间隔,隔离到阻塞线程池
        "sys_overview" | "sys_processes" | "sys_disks" | "sys_networks" => {
            let tool = name.to_string();
            let limit = args
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            let r = tokio::task::spawn_blocking(move || {
                let v = match tool.as_str() {
                    "sys_overview" => crate::sys::overview(),
                    "sys_processes" => crate::sys::processes(limit),
                    "sys_disks" => crate::sys::disks(),
                    _ => crate::sys::networks(),
                };
                crate::sys::audit(&tool);
                v
            })
            .await;
            match r {
                Ok(v) => (serde_json::to_string(&v).unwrap_or_default(), true),
                Err(e) => (format!("系统快照线程异常:{e}"), false),
            }
        }
        _ => ("未知内置工具".into(), false),
    }
}
