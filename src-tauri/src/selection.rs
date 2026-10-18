//! 划词提问(迭代 D9):全局输入监听 + 剪贴板读取。
//!
//! 触发方式:
//!   1. 划词提问:在任意应用中选中文本后连按两次 `Ctrl+C`(间隔 < 700ms),
//!      后端读取剪贴板文本,唤起主窗口并派发 `selection_ask` 事件(前端预填输入框)。
//!
//! 实现说明:为规避新增 crate 的网络下载与权限配置风险,这里直接使用 Win32 FFI
//! (`GetAsyncKeyState` / `GetClipboardData`)做轮询式监听,
//! 不引入 device_query / clipboard 插件。线程常驻整个进程生命周期。

use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::settings;

const CF_UNICODETEXT: u32 = 13;
const VK_CONTROL: i32 = 0x11;
const VK_C: i32 = 0x43;

/// 连按两次 Ctrl+C 判定窗口。
const DOUBLE_WINDOW: Duration = Duration::from_millis(700);
/// 轮询间隔。
const POLL: Duration = Duration::from_millis(25);
/// 设置文件重载间隔(轮询次数)。
const CONFIG_EVERY: u32 = 40;
/// 剪贴板文本长度上限(超出截断,避免把整篇文档塞进输入框)。
const MAX_TEXT: usize = 4000;

#[link(name = "user32")]
unsafe extern "system" {
    fn GetAsyncKeyState(vkey: i32) -> i16;
    fn IsClipboardFormatAvailable(format: u32) -> i32;
    fn OpenClipboard(hwnd: isize) -> i32;
    fn CloseClipboard() -> i32;
    fn GetClipboardData(format: u32) -> isize;
    fn SetProcessDPIAware() -> i32;
}

/// 进程级 DPI 感知。
pub fn ensure_dpi_aware() {
    unsafe {
        SetProcessDPIAware();
    }
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalLock(hmem: isize) -> *mut u16;
    fn GlobalUnlock(hmem: isize) -> i32;
    fn GlobalSize(hmem: isize) -> usize;
}

/// 指定虚拟键当前是否按下(取高位的当前状态)。
fn key_down(vk: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
}

/// 读取剪贴板中的 Unicode 文本;无文本或空串返回 None。
/// pub(crate):指令库(B3){clipboard} 变量渲染时复用。
pub(crate) fn read_clipboard_text() -> Option<String> {
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
            return None;
        }
        if OpenClipboard(0) == 0 {
            return None;
        }
        let handle = GetClipboardData(CF_UNICODETEXT);
        if handle == 0 {
            CloseClipboard();
            return None;
        }
        let ptr = GlobalLock(handle);
        if ptr.is_null() {
            CloseClipboard();
            return None;
        }
        let bytes = GlobalSize(handle);
        let cap = bytes / 2;
        let mut len = 0usize;
        while len < cap && *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        let text = String::from_utf16_lossy(slice);
        GlobalUnlock(handle);
        CloseClipboard();

        let t = text.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    }
}

/// 截断超长文本。
fn clip_text(mut t: String) -> String {
    if t.chars().count() > MAX_TEXT {
        t = t.chars().take(MAX_TEXT).collect();
        t.push('…');
    }
    t
}

#[derive(Clone, Serialize)]
struct SelectionPayload {
    text: String,
}

/// 启动全局监听线程(进程生命周期内常驻,重复调用会启动多个线程,故只调用一次)。
pub fn spawn_listener(app: AppHandle) {
    std::thread::spawn(move || {
        let mut cfg = settings::load();
        let mut tick: u32 = 0;

        let mut c_prev = false;
        let mut last_c: Option<Instant> = None;

        loop {
            std::thread::sleep(POLL);

            tick = tick.wrapping_add(1);
            if tick % CONFIG_EVERY == 0 {
                cfg = settings::load();
            }

            let ctrl = key_down(VK_CONTROL);

            // —— 划词提问:Ctrl 按住时 C 键连按两次 ——
            let c_now = key_down(VK_C);
            let c_edge = ctrl && c_now && !c_prev;
            c_prev = c_now;

            if c_edge {
                if !cfg.selection_ask_enabled {
                    last_c = None;
                } else {
                    let now = Instant::now();
                    match last_c {
                        Some(prev) if now.duration_since(prev) < DOUBLE_WINDOW => {
                            last_c = None;
                            if let Some(text) = read_clipboard_text() {
                                // 唤起主窗口并把划词文本交给前端(预填输入框)
                                if let Some(w) = app.get_webview_window("main") {
                                    let _ = w.show();
                                    let _ = w.unminimize();
                                    let _ = w.set_focus();
                                }
                                let payload = SelectionPayload { text: clip_text(text) };
                                let _ = app.emit_to("main", "selection_ask", payload);
                                log::info!("selection ask triggered");
                            }
                        }
                        _ => last_c = Some(now),
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_text_truncates_long_input() {
        let long = "中".repeat(MAX_TEXT + 10);
        let out = clip_text(long);
        assert_eq!(out.chars().count(), MAX_TEXT + 1); // 截断 + 省略号
        assert!(out.ends_with('…'));
    }

    #[test]
    fn clip_text_keeps_short_input() {
        let s = "短文本".to_string();
        assert_eq!(clip_text(s.clone()), s);
    }

    #[test]
    fn selection_payload_serializes_text() {
        let p = SelectionPayload { text: "hi".into() };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["text"], "hi");
    }
}
