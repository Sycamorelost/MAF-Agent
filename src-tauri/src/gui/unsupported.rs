//! 非 Windows 平台的 GUI 自动化占位实现。
//!
//! UIA / SendInput 仅在 Windows 可用;这里保持与 `uia` / `input` 相同的公开签名,
//! 查询返回空、动作直接报错,让 policy / registry / tools 等上层模块跨平台可编译。

use super::{ElementInfo, WindowInfo};

const UNSUPPORTED: &str = "GUI 自动化仅支持 Windows 平台";

pub mod uia {
    use super::*;

    /// 与 Windows 版 `uia::ElementQuery` 字段保持一致。
    #[derive(Debug, Default, Clone)]
    pub struct ElementQuery {
        pub name: Option<String>,
        pub automation_id: Option<String>,
        pub control_type: Option<String>,
    }

    pub fn list_windows() -> Vec<WindowInfo> {
        Vec::new()
    }

    pub fn find_window(_hwnd: i64) -> Option<WindowInfo> {
        None
    }

    pub fn find_elements(
        _hwnd: i64,
        _query: &ElementQuery,
        _limit: usize,
    ) -> Result<Vec<ElementInfo>, String> {
        Err(UNSUPPORTED.into())
    }

    pub fn invoke_element(_target: &ElementInfo) -> Result<String, String> {
        Err(UNSUPPORTED.into())
    }

    pub fn set_value(_target: &ElementInfo, _value: &str) -> Result<String, String> {
        Err(UNSUPPORTED.into())
    }
}

pub mod input {
    use super::*;

    pub fn focus_window(_hwnd: i64) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }

    pub fn type_unicode(_text: &str) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
}
