//! 小窗 Popout 命令:创建/切换/关闭独立小窗。
//!
//! 小窗用途:
//! - 系统托盘双击唤起的悬浮对话入口
//! - 与主窗口共用一套后端(SQLite / MCP / provider / stream 全复用)
//! - 小窗创建的新会话自动同步到主窗口记录(同一 DB)
//!
//! 窗口默认位置由前端 popout 页面在 load 时自行算好(桌面右下角贴任务栏上方),
//! 然后调 move_popout_to(x, y) 让 Rust 端把窗口放过去。

use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder};

const POPOUT_LABEL: &str = "popout";

/// 确保 popout 窗口存在并 visible。已存在则 show+focus,不存在则创建。
/// 创建时初始位置居中(前端 load 后会立即算好右下角位置再调整)。
pub fn ensure_popout(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(w) = app.get_webview_window(POPOUT_LABEL) {
        let _ = w.set_skip_taskbar(true);
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return Ok(w);
    }
    // 创建:无装饰 + 始终置顶 + 不在任务栏
    let w = WebviewWindowBuilder::new(
        app,
        POPOUT_LABEL,
        tauri::WebviewUrl::default(),
    )
    .title("myAgent")
    // 默认最窄尺寸（右下角贴任务栏出现时看起来像个迷你气泡）
    .inner_size(340.0, 420.0)
    .min_inner_size(320.0, 360.0)
    .max_inner_size(520.0, 720.0)
    .resizable(true)
    .decorations(false)
    .transparent(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .center()
    .build()
    .map_err(|e| format!("创建 popout 窗口失败:{e}"))?;
    Ok(w)
}

/// 关闭(隐藏)popout,不销毁 Webview(保留状态,下次 show 直接恢复)。
pub fn hide_popout(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(POPOUT_LABEL) {
        let _ = w.hide();
    }
}

/// 状态查询:popout 当前是否 visible。
pub fn popout_visible(app: &AppHandle) -> bool {
    app.get_webview_window(POPOUT_LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

// ---------- IPC 命令 ----------

/// 托盘双击 / 主窗口按钮调用:toggle popout 显隐。
#[tauri::command]
pub fn toggle_popout(app: AppHandle) -> Result<bool, String> {
    if popout_visible(&app) {
        hide_popout(&app);
        Ok(false)
    } else {
        let w = ensure_popout(&app)?;
        Ok(w.is_visible().unwrap_or(false))
    }
}

/// 前端调整 popout 位置(load 后算好右下角再调)。
#[tauri::command]
pub fn move_popout(app: AppHandle, x: f64, y: f64) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(POPOUT_LABEL) {
        use tauri::Position;
        let _ = w.set_position(Position::Logical(tauri::LogicalPosition { x, y }));
    }
    Ok(())
}

/// 把 popout 锚定到主屏工作区右下角(紧贴任务栏上方,12px 边距)。
/// 自动读取 Monitor::work_area,排除任务栏占的空间。
#[tauri::command]
pub fn dock_popout(app: AppHandle) -> Result<(), String> {
    let Some(w) = app.get_webview_window(POPOUT_LABEL) else {
        return Err("popout 窗口不存在".into());
    };
    let monitor = w
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())
        .ok_or_else(|| "拿不到屏幕信息".to_string())?;

    // work_area: PhysicalRect { position: (x,y), size: (w,h) } —— 已排除任务栏
    let work = monitor.work_area();
    let scale = w.scale_factor().unwrap_or(1.0);
    let win = w.inner_size().unwrap_or(tauri::PhysicalSize {
        width: 340,
        height: 420,
    });

    let pad = 12.0;
    // work_area.right = x + width; work_area.bottom = y + height
    let work_right = work.position.x as f64 + work.size.width as f64;
    let work_bottom = work.position.y as f64 + work.size.height as f64;
    let x = work_right / scale - win.width as f64 / scale - pad;
    let y = work_bottom / scale - win.height as f64 / scale - pad;

    use tauri::Position;
    let _ = w.set_position(Position::Logical(tauri::LogicalPosition { x, y }));
    Ok(())
}

/// 供托盘事件直接调用(不走 IPC):左键单击主窗口图标、双击切 popout。
pub fn toggle_popout_from_tray(app: &AppHandle) {
    if popout_visible(app) {
        hide_popout(app);
    } else {
        let _ = ensure_popout(app);
    }
}

/// 托盘双击:只打开(显示并聚焦)小窗,不做隐藏切换。
pub fn show_popout_from_tray(app: &AppHandle) {
    let _ = ensure_popout(app);
}
