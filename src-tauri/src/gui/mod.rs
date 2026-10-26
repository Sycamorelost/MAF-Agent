//! GUI 自动化能力(Tauri host 侧):窗口枚举 + UI Automation + 模拟输入。
//!
//! 安全模型(逐层):
//! 1. 默认关闭(`data/gui-policy.json` 的 `enabled=false`),关闭时所有 ui_* 工具直接报错;
//! 2. 仅允许白名单进程(进程名精确匹配,大小写不敏感);
//! 3. 动作类工具(点击/写值/输入/聚焦)必须经过 HITL 审批或能力令牌;
//! 4. 元素句柄带 TTL 与数量上限,过期需重新查找,防止拿着陈旧句柄乱点。

#[cfg(windows)]
pub mod input;
pub mod policy;
pub mod registry;
pub mod tools;
#[cfg(windows)]
pub mod uia;

// 非 Windows:UIA / SendInput 不可用,提供同签名占位实现(调用直接报错),保证跨平台可编译
#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
pub use unsupported::{input, uia};

use serde::Serialize;

pub use policy::GuiPolicy;

/// 顶层窗口信息(ui_list_windows 返回值)。
#[derive(Debug, Clone, Serialize)]
pub struct WindowInfo {
    /// 窗口句柄(十进制,作为后续工具的 hwnd 参数)
    pub hwnd: i64,
    pub pid: u32,
    /// 进程名,如 notepad.exe —— 白名单按它匹配
    pub process: String,
    pub title: String,
}

/// UI 元素信息(ui_find_elements 返回值)。
#[derive(Debug, Clone, Serialize)]
pub struct ElementInfo {
    /// 短句柄,后续 ui_invoke / ui_set_value 用它定位元素
    pub element_id: String,
    pub hwnd: i64,
    pub process: String,
    pub name: String,
    pub automation_id: String,
    /// 人类可读控件类型,如 button / edit
    pub control_type: String,
    pub class_name: String,
    pub enabled: bool,
    /// 屏幕坐标 [left, top, right, bottom]
    pub rect: [i32; 4],
}
