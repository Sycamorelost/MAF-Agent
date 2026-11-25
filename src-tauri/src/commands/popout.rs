//! 小窗 Popout 命令:创建/切换/关闭独立小窗。
//!
//! 小窗用途:
//! - 系统托盘双击唤起的悬浮对话入口
//! - 与主窗口共用一套后端(SQLite / MCP / provider / stream 全复用)
//! - 小窗创建的新会话自动同步到主窗口记录(同一 DB)
//!
//! 窗口在创建时就由 Rust 端算好目标显示器工作区右下角坐标,首帧即落在正确位置,
//! 不存在「先居中、再由前端调整」的跳位。

use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder};

const POPOUT_LABEL: &str = "popout";
/// 小窗默认逻辑尺寸(与 inner_size 保持一致)
const POPOUT_W: f64 = 340.0;
const POPOUT_H: f64 = 420.0;
/// 距工作区右/下边缘的留白
const POPOUT_PAD: f64 = 12.0;

/// 纯函数:把物理工作区换算成「窗口右下角落位」的逻辑坐标。
/// work_area 为物理像素且已排除任务栏,统一除以显示器 scale_factor 转为逻辑像素。
fn bottom_right_in(
    work_x: i32,
    work_y: i32,
    work_w: u32,
    work_h: u32,
    scale: f64,
    win_w: f64,
    win_h: f64,
    pad: f64,
) -> (f64, f64) {
    let right = (work_x as f64 + work_w as f64) / scale;
    let bottom = (work_y as f64 + work_h as f64) / scale;
    (right - win_w - pad, bottom - win_h - pad)
}

/// 首次创建用的锚点:优先光标所在显示器,拿不到再回退主屏。
fn anchor_bottom_right(app: &AppHandle, win_w: f64, win_h: f64) -> Option<(f64, f64)> {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let work = monitor.work_area();
    Some(bottom_right_in(
        work.position.x,
        work.position.y,
        work.size.width,
        work.size.height,
        monitor.scale_factor(),
        win_w,
        win_h,
        POPOUT_PAD,
    ))
}

/// 确保 popout 窗口存在并 visible。已存在则 show+focus,不存在则创建。
/// 创建时直接定位到光标所在显示器工作区右下角,避免先居中再跳位。
pub fn ensure_popout(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(w) = app.get_webview_window(POPOUT_LABEL) {
        let _ = w.set_skip_taskbar(true);
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return Ok(w);
    }
    // 创建:无装饰 + 始终置顶 + 不在任务栏
    let mut builder = WebviewWindowBuilder::new(app, POPOUT_LABEL, tauri::WebviewUrl::default())
        .title("myAgent")
        // 默认最窄尺寸（右下角贴任务栏出现时看起来像个迷你气泡）
        .inner_size(POPOUT_W, POPOUT_H)
        .min_inner_size(320.0, 360.0)
        .max_inner_size(520.0, 720.0)
        .resizable(true)
        .decorations(false)
        .transparent(false)
        .always_on_top(true)
        .skip_taskbar(true);
    // 构建前就给出右下角坐标:窗口首次显示即在目标位置,没有居中帧。
    // 仅在完全拿不到屏幕信息时才退回居中。
    builder = match anchor_bottom_right(app, POPOUT_W, POPOUT_H) {
        Some((x, y)) => builder.position(x, y),
        None => builder.center(),
    };
    let w = builder
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
    let scale = monitor.scale_factor();
    let win = w.inner_size().unwrap_or(tauri::PhysicalSize {
        width: (POPOUT_W * scale) as u32,
        height: (POPOUT_H * scale) as u32,
    });
    let (x, y) = bottom_right_in(
        work.position.x,
        work.position.y,
        work.size.width,
        work.size.height,
        scale,
        win.width as f64 / scale,
        win.height as f64 / scale,
        POPOUT_PAD,
    );

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

#[cfg(test)]
mod tests {
    use super::bottom_right_in;

    /// 主屏 1920×1080、任务栏 40px、150% 缩放:窗口应贴在工作区右下角。
    #[test]
    fn anchor_matches_scaled_work_area() {
        let (x, y) = bottom_right_in(0, 0, 1920, 1040, 1.5, 340.0, 420.0, 12.0);
        assert!((x - 928.0).abs() < 0.01, "x = {x}");
        assert!((y - 261.333).abs() < 0.01, "y = {y}");
    }

    /// 副屏工作区原点是 (1920, 0)、100% 缩放:位置必须叠加显示器原点。
    #[test]
    fn anchor_respects_secondary_monitor_origin() {
        let (x, y) = bottom_right_in(1920, 0, 1920, 1040, 1.0, 340.0, 420.0, 12.0);
        assert!((x - 3488.0).abs() < 0.01, "x = {x}");
        assert!((y - 608.0).abs() < 0.01, "y = {y}");
    }

    /// 主屏左侧的显示器(负原点):换算后仍落在该屏右下角。
    #[test]
    fn anchor_handles_negative_origin() {
        let (x, y) = bottom_right_in(-1920, 0, 1920, 1040, 1.0, 340.0, 420.0, 12.0);
        assert!((x + 352.0).abs() < 0.01, "x = {x}");
        assert!((y - 608.0).abs() < 0.01, "y = {y}");
    }
}
