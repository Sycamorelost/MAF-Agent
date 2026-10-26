//! 模拟输入:聚焦窗口 / 输入 Unicode 文本 / 指定坐标左键点击。
//!
//! 输入走 SendInput + KEYEVENTF_UNICODE(中文等非 ASCII 也能逐 UTF-16 单元送达);
//! 点击走 SetCursorPos + mouse_event(先聚焦,失败则不动鼠标)。

use std::mem::size_of;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, SendInput, VIRTUAL_KEY,
    mouse_event,
};
use windows::Win32::UI::WindowsAndMessaging::{
    IsIconic, SW_RESTORE, SetCursorPos, SetForegroundWindow, ShowWindow,
};

/// 把窗口切到前台(最小化则先恢复)。前台切换失败直接报错,避免把内容打到别的窗口。
pub fn focus_window(hwnd: i64) -> Result<(), String> {
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if !SetForegroundWindow(hwnd).as_bool() {
            return Err("无法把目标窗口切到前台(窗口可能已关闭或被系统限制)".into());
        }
    }
    Ok(())
}

/// 发送一段文本(逐字符 down/up)。调用方负责长度上限与前台聚焦。
pub fn type_unicode(text: &str) -> Result<(), String> {
    let flags_up: KEYBD_EVENT_FLAGS = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP;
    let mut inputs: Vec<INPUT> = Vec::with_capacity(text.encode_utf16().count() * 2);
    for unit in text.encode_utf16() {
        for flags in [KEYEVENTF_UNICODE, flags_up] {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: unit,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
    }
    if inputs.is_empty() {
        return Ok(());
    }
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent == 0 {
        return Err("SendInput 被系统拒绝(常见于目标进程权限更高)".into());
    }
    Ok(())
}

/// 在屏幕坐标 (x, y) 模拟一次左键单击。
pub fn click_point(x: i32, y: i32) -> Result<(), String> {
    unsafe {
        SetCursorPos(x, y).map_err(|e| format!("移动鼠标失败:{e}"))?;
        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
        mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
    }
    Ok(())
}
