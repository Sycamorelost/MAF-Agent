//! 能力令牌的 Tauri commands —— 签发 / 查看 / 撤销。
//!
//! 令牌只存内存(重启失效),因此这里不做持久化,也不接受前端「导入既有令牌」。

use crate::mcp::capability::{Capability, CapabilityStore, Scope};
use crate::paths::data_dir as data_dir_fn;
use tauri::State;

/// 签发一个令牌。ttl_secs 省略时默认 30 分钟,后端会收敛到 [1min, 8h]。
/// 能力令牌已并入「完全信任」总开关:非完全信任模式拒绝签发。
#[tauri::command]
pub fn mcp_issue_capability(
    label: String,
    scopes: Vec<Scope>,
    ttl_secs: Option<i64>,
    max_uses: Option<u32>,
    store: State<'_, CapabilityStore>,
) -> Result<Capability, String> {
    let policy = crate::mcp::approval::load_policy(&data_dir_fn());
    if policy.mode != crate::mcp::approval::ApprovalMode::Full {
        return Err("能力令牌已并入完全信任模式:请将审批模式切为「完全信任」后签发".into());
    }
    let cap = store.issue(&label, scopes, ttl_secs, max_uses)?;
    let scope_text: Vec<&str> = cap.scopes.iter().map(|s| s.label()).collect();
    crate::mcp::audit(
        &data_dir_fn(),
        "capability_issue",
        "-",
        "-",
        &format!(
            "cap={} label={} scopes={} expires_at={} max_uses={:?}",
            cap.id,
            cap.label,
            scope_text.join(","),
            cap.expires_at,
            cap.max_uses
        ),
    );
    Ok(cap)
}

/// 列出仍有效的令牌(过期项会自动剔除)。
#[tauri::command]
pub fn mcp_list_capabilities(store: State<'_, CapabilityStore>) -> Vec<Capability> {
    store.list_active()
}

/// 撤销令牌 —— 立即让后续调用回到人工审批。
#[tauri::command]
pub fn mcp_revoke_capability(
    id: String,
    store: State<'_, CapabilityStore>,
) -> Result<bool, String> {
    let ok = store.revoke(&id);
    if ok {
        crate::mcp::audit(
            &data_dir_fn(),
            "capability_revoke",
            "-",
            "-",
            &format!("cap={id}"),
        );
    }
    Ok(ok)
}

/// 清空全部令牌(切换审批模式 / 关闭 GUI 自动化时由前端调用)。
#[tauri::command]
pub fn mcp_clear_capabilities(store: State<'_, CapabilityStore>) {
    store.clear();
}
