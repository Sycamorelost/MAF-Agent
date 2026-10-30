//! GUI 自动化相关 Tauri commands —— 策略读写 + 设置页自检(窗口 / 元素预览)。
//!
//! 真正的工具执行在 `crate::gui::tools`,由 dispatcher 走审批后调用。

use crate::gui::{ElementInfo, GuiPolicy, WindowInfo, policy, uia};
use crate::paths::data_dir as data_dir_fn;

/// 读取 GUI 自动化策略。
#[tauri::command]
pub async fn gui_get_policy() -> Result<GuiPolicy, String> {
    Ok(policy::load(&data_dir_fn()))
}

/// 保存策略。关闭开关时同步清空元素句柄,避免旧句柄残留。
#[tauri::command]
pub async fn gui_save_policy(policy: GuiPolicy) -> Result<(), String> {
    policy::save(&data_dir_fn(), &policy)?;
    if !policy.enabled {
        crate::gui::registry::REGISTRY.clear();
    }
    Ok(())
}

/// 列出可见窗口(设置页自检,只读)。
#[tauri::command]
pub async fn gui_list_windows() -> Result<Vec<WindowInfo>, String> {
    tokio::task::spawn_blocking(uia::list_windows)
        .await
        .map_err(|e| format!("窗口枚举失败:{e}"))
}

/// 在指定窗口内预览元素(设置页自检,只读)。
#[tauri::command]
pub async fn gui_find_elements(
    hwnd: i64,
    name: Option<String>,
    automation_id: Option<String>,
    control_type: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<ElementInfo>, String> {
    let cfg = policy::load(&data_dir_fn());
    // 设置页自检与工具执行同一总开关:仅完全信任模式可用
    let approval = crate::mcp::approval::load_policy(&data_dir_fn());
    if approval.mode != crate::mcp::approval::ApprovalMode::Full {
        return Err("GUI 自动化已并入完全信任模式:请将审批模式切为「完全信任」后自检".into());
    }
    let window = uia::find_window(hwnd).ok_or_else(|| format!("窗口 {hwnd} 不存在或不可见"))?;
    cfg.ensure_app(&window.process)?;
    let query = uia::ElementQuery {
        name: name.filter(|s| !s.trim().is_empty()),
        automation_id: automation_id.filter(|s| !s.trim().is_empty()),
        control_type: control_type.filter(|s| !s.trim().is_empty()),
    };
    let max = limit
        .map(|n| (n as usize).clamp(1, 200))
        .unwrap_or(cfg.max_elements());
    tokio::task::spawn_blocking(move || uia::find_elements(hwnd, &query, max))
        .await
        .map_err(|e| format!("元素查找失败:{e}"))?
}
