//! 托盘右键的自定义菜单(与主窗口同主题)+ 菜单动作命令。
//!
//! 原生菜单无法跟随应用主题,这里改为一个无边框透明 WebviewWindow:
//! 右键托盘图标时按托盘图标位置弹出,失焦自动收起。
//! 左键单击不再触发任何动作;双击打开小窗由 `lib.rs` 的托盘事件处理。

use tauri::{
    AppHandle, Manager, PhysicalPosition, Position, Rect, Size, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

/// 菜单窗口 label(前端 App.tsx 按此分支渲染托盘菜单)。
const MENU_LABEL: &str = "tray-menu";
/// 菜单逻辑尺寸(与前端卡片高度保持一致,多余区域保持透明)。
const MENU_W: f64 = 216.0;
const MENU_H: f64 = 152.0;
/// 菜单与托盘图标之间的间距(物理像素)。
const GAP: f64 = 8.0;

/// 创建(或复用)隐藏的菜单窗口。启动时预建,右键时无需等待 Webview 初始化。
fn ensure_menu_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(w) = app.get_webview_window(MENU_LABEL) {
        return Ok(w);
    }
    let win = WebviewWindowBuilder::new(app, MENU_LABEL, WebviewUrl::default())
        .title("tray-menu")
        .inner_size(MENU_W, MENU_H)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|e| format!("创建托盘菜单窗口失败:{e}"))?;
    // 点击菜单外部 / 切换窗口 → 失焦收起
    let handle = win.clone();
    win.on_window_event(move |event| {
        if matches!(event, WindowEvent::Focused(false)) {
            let _ = handle.hide();
        }
    });
    Ok(win)
}

/// 启动时预建菜单窗口(隐藏),避免首次右键时出现加载空窗。
pub fn preload(app: &AppHandle) {
    if let Err(e) = ensure_menu_window(app) {
        log::warn!("托盘菜单窗口预建失败: {e}");
    }
}

/// 收起菜单窗口(动作执行前调用)。
pub fn hide_menu(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MENU_LABEL) {
        let _ = w.hide();
    }
}

/// 纯函数:菜单落点(物理像素)。图标矩形 + 菜单尺寸 + 工作区 → 菜单左上角。
/// 默认水平居中于图标、垂直贴在图标上方;超出工作区时夹回,
/// 上方空间不足(如任务栏在屏幕顶部)则改为图标下方。
fn place_menu(
    icon: (f64, f64, f64, f64),
    size: (f64, f64),
    work: Option<(f64, f64, f64, f64)>,
) -> (f64, f64) {
    let (icon_x, icon_y, icon_w, icon_h) = icon;
    let (w, h) = size;
    let mut x = icon_x + icon_w / 2.0 - w / 2.0;
    let mut y = icon_y - h - GAP;
    if let Some((left, top, right, bottom)) = work {
        x = x.clamp(left, (right - w).max(left));
        if y < top {
            y = icon_y + icon_h + GAP;
        }
        y = y.clamp(top, (bottom - h).max(top));
    }
    (x.round(), y.round())
}

/// 计算菜单左上角的物理坐标:水平居中于托盘图标,垂直贴在图标上方;
/// 超出工作区时夹回,上方空间不足则改到图标下方。
fn menu_position(app: &AppHandle, anchor: PhysicalPosition<f64>, rect: &Rect) -> (i32, i32) {
    let monitor = app
        .monitor_from_point(anchor.x, anchor.y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let scale = monitor
        .as_ref()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);
    // 托盘图标矩形统一换算到物理像素(Windows 为 Physical;Logical 时按 scale 放大)
    let (icon_x, icon_y) = match rect.position {
        Position::Physical(p) => (p.x as f64, p.y as f64),
        Position::Logical(p) => (p.x * scale, p.y * scale),
    };
    let (icon_w, icon_h) = match rect.size {
        Size::Physical(s) => (s.width as f64, s.height as f64),
        Size::Logical(s) => (s.width * scale, s.height * scale),
    };
    let work = monitor.map(|m| {
        let w = m.work_area();
        (
            w.position.x as f64,
            w.position.y as f64,
            w.position.x as f64 + w.size.width as f64,
            w.position.y as f64 + w.size.height as f64,
        )
    });
    let (x, y) = place_menu(
        (icon_x, icon_y, icon_w, icon_h),
        (MENU_W * scale, MENU_H * scale),
        work,
    );
    (x as i32, y as i32)
}

/// 右键托盘图标:在图标上方弹出菜单(已显示时先收起再重新定位)。
pub fn show_menu(app: &AppHandle, anchor: PhysicalPosition<f64>, rect: Rect) -> Result<(), String> {
    let win = ensure_menu_window(app)?;
    let (x, y) = menu_position(app, anchor, &rect);
    let _ = win.set_position(Position::Physical(PhysicalPosition::new(x, y)));
    let _ = win.show();
    let _ = win.set_focus();
    Ok(())
}

/// 托盘菜单按钮动作(菜单窗口前端 invoke)。
#[tauri::command]
pub fn tray_menu_action(app: AppHandle, action: String) -> Result<(), String> {
    hide_menu(&app);
    match action.as_str() {
        // 显示并聚焦主窗口
        "show" => crate::show_main_window(&app),
        // 切换小窗(与托盘双击同语义)
        "popout" => crate::commands::popout::toggle_popout_from_tray(&app),
        // 仅收起菜单(Esc / 失焦)
        "close" => {}
        "quit" => app.exit(0),
        other => return Err(format!("未知托盘菜单动作:{other}")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::place_menu;

    const WORK: (f64, f64, f64, f64) = (0.0, 0.0, 1920.0, 1080.0);

    /// 底部任务栏图标:菜单落在图标正上方并水平居中。
    #[test]
    fn menu_sits_above_taskbar_icon() {
        let (x, y) = place_menu((1800.0, 1040.0, 24.0, 24.0), (216.0, 152.0), Some(WORK));
        assert_eq!((x, y), (1704.0, 880.0));
    }

    /// 图标贴近右边缘:菜单水平方向被夹回工作区内。
    #[test]
    fn menu_clamped_to_work_area() {
        let (x, _) = place_menu((1900.0, 1040.0, 20.0, 20.0), (216.0, 152.0), Some(WORK));
        assert_eq!(x, 1704.0);
    }

    /// 任务栏在屏幕顶部:上方无空间时改为图标下方。
    #[test]
    fn menu_flips_below_when_no_room_above() {
        let (_, y) = place_menu((100.0, 4.0, 24.0, 24.0), (216.0, 152.0), Some(WORK));
        assert_eq!(y, 36.0);
    }

    /// 拿不到屏幕信息时仍按图标上方定位,不 panic。
    #[test]
    fn menu_without_work_area() {
        let (x, y) = place_menu((1800.0, 1040.0, 24.0, 24.0), (216.0, 152.0), None);
        assert_eq!((x, y), (1704.0, 880.0));
    }
}
