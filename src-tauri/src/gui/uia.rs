//! UI Automation 封装:窗口枚举 / 元素搜索 / 元素动作(Invoke、Value、点击兜底)。
//!
//! COM 约定:
//! - 每次操作在调用线程自行 `CoInitializeEx(MTA)`,结束再 `CoUninitialize`;
//! - 元素按「描述」重新定位,不跨线程保存接口指针(见 registry.rs)。

use std::ffi::c_void;
use std::sync::OnceLock;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern,
    IUIAutomationSelectionItemPattern, IUIAutomationTogglePattern, IUIAutomationTreeWalker,
    IUIAutomationValuePattern, UIA_InvokePatternId, UIA_SelectionItemPatternId,
    UIA_TogglePatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};
use windows::core::{BOOL, BSTR, PWSTR};

use super::{ElementInfo, WindowInfo};

/// 单次遍历的最大节点数(防止浏览器等超大 UI 树拖死)。
const MAX_VISITED: usize = 6000;
/// 遍历最大深度。
const MAX_DEPTH: usize = 40;
/// 允许的矩形误差(像素)。
const RECT_TOLERANCE: i32 = 8;

/// 元素搜索条件 —— 全部为空表示「列出前 N 个元素」。
#[derive(Debug, Default, Clone)]
pub struct ElementQuery {
    pub name: Option<String>,
    pub automation_id: Option<String>,
    pub control_type: Option<String>,
}

/// 列出可见顶层窗口(按标题非空过滤,并排除本进程自身)。
pub fn list_windows() -> Vec<WindowInfo> {
    let mut out: Vec<WindowInfo> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(enum_window), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // 安全:lparam 由 list_windows 传入,始终指向栈上的 Vec,回调期间有效
    let out = unsafe { &mut *(lparam.0 as *mut Vec<WindowInfo>) };
    if out.len() >= 200 {
        return BOOL(0);
    }
    if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return BOOL(1);
    }
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return BOOL(1);
    }
    let mut buf = vec![0u16; len as usize + 1];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if n <= 0 {
        return BOOL(1);
    }
    let title = String::from_utf16_lossy(&buf[..n as usize])
        .trim()
        .to_string();
    if title.is_empty() {
        return BOOL(1);
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    let process = process_name(pid);
    if process.is_empty() || process.eq_ignore_ascii_case(own_exe()) {
        return BOOL(1);
    }
    out.push(WindowInfo {
        hwnd: hwnd.0 as i64,
        pid,
        process,
        title,
    });
    BOOL(1)
}

/// 按 hwnd 取窗口信息(用于动作前复核进程白名单)。
pub fn find_window(hwnd: i64) -> Option<WindowInfo> {
    list_windows().into_iter().find(|w| w.hwnd == hwnd)
}

/// 窗口内搜索元素并登记短句柄。
pub fn find_elements(
    hwnd: i64,
    query: &ElementQuery,
    limit: usize,
) -> Result<Vec<ElementInfo>, String> {
    let process = find_window(hwnd)
        .map(|w| w.process)
        .unwrap_or_else(|| "unknown".into());
    with_uia(|uia| {
        let root = unsafe { uia.ElementFromHandle(HWND(hwnd as *mut c_void)) }
            .map_err(|e| format!("获取窗口 UI 树失败(窗口可能已关闭或权限不足):{e}"))?;
        let walker =
            unsafe { uia.RawViewWalker() }.map_err(|e| format!("创建 UI 树遍历器失败:{e}"))?;
        let mut out: Vec<ElementInfo> = Vec::new();
        walk(&walker, &root, 0, &mut 0, &mut |el| {
            if out.len() >= limit {
                return true;
            }
            if element_matches(el, query) {
                out.push(super::registry::REGISTRY.add(describe(el, hwnd, &process)));
            }
            false
        });
        Ok(out)
    })
}

/// 触发元素:优先 Invoke → Toggle → SelectionItem,最后按矩形中心模拟点击。
pub fn invoke_element(target: &ElementInfo) -> Result<String, String> {
    with_uia(|uia| {
        let el = locate_element(uia, target)?;
        unsafe {
            if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
                && p.Invoke().is_ok()
            {
                return Ok("已通过 Invoke 模式触发".into());
            }
            if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId)
                && p.Toggle().is_ok()
            {
                return Ok("已通过 Toggle 模式切换".into());
            }
            if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                UIA_SelectionItemPatternId,
            ) && p.Select().is_ok()
            {
                return Ok("已通过 SelectionItem 模式选中".into());
            }
        }
        // 兜底:元素不支持任何动作模式时,按可见矩形中心点击
        let [l, t, r, b] = target.rect;
        if r <= l || b <= t {
            return Err("元素矩形无效(可能已隐藏),无法点击".into());
        }
        crate::gui::input::click_point(l + (r - l) / 2, t + (b - t) / 2)?;
        Ok("已通过模拟鼠标点击触发".into())
    })
}

/// 写值:仅支持 ValuePattern 的元素(输入框等)。
pub fn set_value(target: &ElementInfo, value: &str) -> Result<String, String> {
    with_uia(|uia| {
        let el = locate_element(uia, target)?;
        let pattern = unsafe {
            el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .map_err(|_| "该元素不支持 ValuePattern,请改用 ui_type_text".to_string())?
        };
        unsafe { pattern.SetValue(&BSTR::from(value)) }.map_err(|e| format!("写入失败:{e}"))?;
        Ok("已通过 ValuePattern 写入".into())
    })
}

/// 在 MTA 线程上下文里执行一段 UIA 操作(本函数同步阻塞,调用方需放在阻塞线程)。
fn with_uia<T>(f: impl FnOnce(&IUIAutomation) -> Result<T, String>) -> Result<T, String> {
    // 返回 S_OK / S_FALSE 都代表本线程由这次调用完成初始化,需要配对 CoUninitialize
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let own = hr.is_ok();
    let result = (|| {
        let uia: IUIAutomation = unsafe {
            CoCreateInstance(
                &CUIAutomation,
                None::<&windows::core::IUnknown>,
                CLSCTX_INPROC_SERVER,
            )
        }
        .map_err(|e| format!("初始化 UI Automation 失败:{e}"))?;
        f(&uia)
    })();
    if own {
        unsafe { CoUninitialize() };
    }
    result
}

/// 深度优先遍历子元素;回调返回 true 表示提前结束。
fn walk<F>(
    walker: &IUIAutomationTreeWalker,
    parent: &IUIAutomationElement,
    depth: usize,
    visited: &mut usize,
    hit: &mut F,
) where
    F: FnMut(&IUIAutomationElement) -> bool,
{
    if depth >= MAX_DEPTH || *visited >= MAX_VISITED {
        return;
    }
    let mut child = unsafe { walker.GetFirstChildElement(parent) }.ok();
    while let Some(el) = child {
        *visited += 1;
        if *visited >= MAX_VISITED || hit(&el) {
            return;
        }
        walk(walker, &el, depth + 1, visited, hit);
        child = unsafe { walker.GetNextSiblingElement(&el) }.ok();
    }
}

/// 按描述重新定位元素(automation_id 优先,否则名称 + 控件类型 + 矩形)。
fn locate_element(
    uia: &IUIAutomation,
    target: &ElementInfo,
) -> Result<IUIAutomationElement, String> {
    let root = unsafe { uia.ElementFromHandle(HWND(target.hwnd as *mut c_void)) }
        .map_err(|e| format!("获取窗口 UI 树失败(窗口可能已关闭):{e}"))?;
    let walker = unsafe { uia.RawViewWalker() }.map_err(|e| format!("创建 UI 树遍历器失败:{e}"))?;
    let mut found: Option<IUIAutomationElement> = None;
    walk(&walker, &root, 0, &mut 0, &mut |el| {
        if locate_matches(el, target) {
            found = Some(el.clone());
            true
        } else {
            false
        }
    });
    found.ok_or_else(|| "元素已失效,请重新执行 ui_find_elements 获取新句柄".into())
}

fn locate_matches(el: &IUIAutomationElement, target: &ElementInfo) -> bool {
    let ct = control_type_name(current_control_type(el));
    if !target.control_type.is_empty() && !ct.eq_ignore_ascii_case(&target.control_type) {
        return false;
    }
    let aid = read_text(unsafe { el.CurrentAutomationId() });
    if !target.automation_id.is_empty() {
        return aid.eq_ignore_ascii_case(&target.automation_id);
    }
    let name = read_text(unsafe { el.CurrentName() });
    name == target.name && rect_close(current_rect(el), target.rect)
}

fn element_matches(el: &IUIAutomationElement, q: &ElementQuery) -> bool {
    if let Some(want) = &q.automation_id
        && !read_text(unsafe { el.CurrentAutomationId() })
            .to_ascii_lowercase()
            .contains(&want.to_ascii_lowercase())
    {
        return false;
    }
    if let Some(want) = &q.control_type
        && !control_type_name(current_control_type(el)).eq_ignore_ascii_case(want.trim())
    {
        return false;
    }
    if let Some(want) = &q.name
        && !read_text(unsafe { el.CurrentName() })
            .to_ascii_lowercase()
            .contains(&want.to_ascii_lowercase())
    {
        return false;
    }
    // 名称 / 类型 / id 都为空时,只返回有实际标识的元素,避免一堆匿名 Pane 刷屏
    !(q.name.is_none()
        && q.automation_id.is_none()
        && q.control_type.is_none()
        && read_text(unsafe { el.CurrentName() }).is_empty())
}

fn describe(el: &IUIAutomationElement, hwnd: i64, process: &str) -> ElementInfo {
    ElementInfo {
        element_id: String::new(),
        hwnd,
        process: process.to_string(),
        name: read_text(unsafe { el.CurrentName() }),
        automation_id: read_text(unsafe { el.CurrentAutomationId() }),
        control_type: control_type_name(current_control_type(el)),
        class_name: read_text(unsafe { el.CurrentClassName() }),
        enabled: unsafe { el.CurrentIsEnabled() }
            .map(|b| b.as_bool())
            .unwrap_or(false),
        rect: current_rect(el),
    }
}

fn current_control_type(el: &IUIAutomationElement) -> u32 {
    unsafe { el.CurrentControlType() }
        .map(|c| c.0.max(0) as u32)
        .unwrap_or(0)
}

fn current_rect(el: &IUIAutomationElement) -> [i32; 4] {
    unsafe { el.CurrentBoundingRectangle() }
        .map(|r| [r.left, r.top, r.right, r.bottom])
        .unwrap_or([0, 0, 0, 0])
}

fn rect_close(a: [i32; 4], b: [i32; 4]) -> bool {
    // 目标矩形缺失(全 0)时不做几何校验
    if b == [0, 0, 0, 0] {
        return true;
    }
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| (x - y).abs() <= RECT_TOLERANCE)
}

fn read_text(r: windows::core::Result<BSTR>) -> String {
    r.map(|b| b.to_string()).unwrap_or_default()
}

/// pid → 进程名(取文件名部分;失败返回空串)。
fn process_name(pid: u32) -> String {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if !ok {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..size as usize])
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or_default()
            .to_string()
    }
}

fn own_exe() -> &'static str {
    static OWN: OnceLock<String> = OnceLock::new();
    OWN.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_default()
    })
}

/// UIA 控件类型 id → 常用英文名(未收录的返回 `control_<id>`)。
fn control_type_name(id: u32) -> String {
    let name = match id {
        50000 => "button",
        50001 => "calendar",
        50002 => "checkbox",
        50003 => "combobox",
        50004 => "edit",
        50005 => "hyperlink",
        50006 => "image",
        50007 => "listitem",
        50008 => "list",
        50009 => "menu",
        50010 => "menubar",
        50011 => "menuitem",
        50012 => "progressbar",
        50013 => "radiobutton",
        50014 => "scrollbar",
        50015 => "slider",
        50016 => "spinner",
        50017 => "statusbar",
        50018 => "tab",
        50019 => "tabitem",
        50020 => "text",
        50021 => "toolbar",
        50022 => "tooltip",
        50023 => "tree",
        50024 => "treeitem",
        50025 => "custom",
        50026 => "group",
        50028 => "datagrid",
        50029 => "dataitem",
        50030 => "document",
        50031 => "splitbutton",
        50032 => "window",
        50033 => "pane",
        50034 => "header",
        50035 => "headeritem",
        50036 => "table",
        50037 => "titlebar",
        50038 => "separator",
        _ => return format!("control_{id}"),
    };
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_type_names_cover_common_controls() {
        assert_eq!(control_type_name(50000), "button");
        assert_eq!(control_type_name(50004), "edit");
        assert_eq!(control_type_name(50033), "pane");
        assert_eq!(control_type_name(59999), "control_59999");
    }

    #[test]
    fn rect_close_tolerates_small_drift() {
        assert!(rect_close([10, 10, 110, 40], [12, 9, 108, 43]));
        assert!(!rect_close([10, 10, 110, 40], [0, 0, 110, 40]));
        assert!(
            rect_close([1, 2, 3, 4], [0, 0, 0, 0]),
            "目标矩形缺失时不做校验"
        );
    }
}
