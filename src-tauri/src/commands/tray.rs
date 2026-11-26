//! 托盘右键的自定义菜单(与主窗口同主题)+ 菜单动作命令。
//!
//! 原生菜单无法跟随应用主题,这里改为一个无边框透明 WebviewWindow:
//! 右键托盘图标时按托盘图标位置弹出,失焦自动收起。
//! 左键单击不再触发任何动作;双击打开小窗由 `lib.rs` 的托盘事件处理。

use std::sync::Mutex;

use tauri::{
    AppHandle, Manager, PhysicalPosition, Position, Rect, Size, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

/// 菜单窗口 label(前端 App.tsx 按此分支渲染托盘菜单)。
const MENU_LABEL: &str = "tray-menu";
/// 菜单初始逻辑尺寸(前端测量出真实卡片大小后会调用 `tray_menu_fit` 收缩)。
const MENU_W: f64 = 216.0;
const MENU_H: f64 = 152.0;
/// 菜单与托盘图标之间的间距(物理像素)。0 = 菜单紧贴图标上沿。
const GAP: f64 = 0.0;

/// 最近一次右键的锚点(托盘图标矩形 + 光标位置),供前端测量后重定位复用。
static LAST_ANCHOR: Mutex<Option<(PhysicalPosition<f64>, Rect)>> = Mutex::new(None);

/// 前端实测的卡片盒子(逻辑像素):卡片宽 / 高 / 卡片在窗口内的左偏移 / 上偏移。
///
/// 定位必须以**卡片的可见区域**为基准,不能以窗口为基准:
/// 窗口含透明余量与 DPI 取整误差,按窗口定位会让菜单底部空出一截(表现为「距离过远」)。
static CARD_BOX: Mutex<Option<(f64, f64, f64, f64)>> = Mutex::new(None);

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

/// 纯函数:按**卡片可见区域**计算窗口落点(物理像素)。
///
/// - card:卡片宽高;offset:卡片相对窗口左上角的偏移
/// - 默认水平居中于图标、卡片下沿紧贴图标上沿;越界夹回,上方无空间则改到图标下方
fn place_card(
    icon: (f64, f64, f64, f64),
    card: (f64, f64),
    offset: (f64, f64),
    work: Option<(f64, f64, f64, f64)>,
) -> (f64, f64) {
    let (icon_x, icon_y, icon_w, icon_h) = icon;
    let (cw, ch) = card;
    let (ox, oy) = offset;
    // 先算「卡片」该落在哪,再反推窗口位置
    let mut card_x = icon_x + icon_w / 2.0 - cw / 2.0;
    let mut card_y = icon_y - ch - GAP;
    if let Some((left, top, right, bottom)) = work {
        card_x = card_x.clamp(left, (right - cw).max(left));
        if card_y < top {
            card_y = icon_y + icon_h + GAP;
        }
        card_y = card_y.clamp(top, (bottom - ch).max(top));
    }
    ((card_x - ox).round(), (card_y - oy).round())
}

/// 托盘图标矩形 + 工作区 + 缩放系数(统一换算成物理像素)。
fn icon_frame(
    app: &AppHandle,
    anchor: PhysicalPosition<f64>,
    rect: &Rect,
) -> ((f64, f64, f64, f64), Option<(f64, f64, f64, f64)>, f64) {
    let monitor = app
        .monitor_from_point(anchor.x, anchor.y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let scale = monitor
        .as_ref()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);
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
    ((icon_x, icon_y, icon_w, icon_h), work, scale)
}

/// 以卡片盒子(逻辑像素)定位窗口。
fn menu_position_card(
    app: &AppHandle,
    anchor: PhysicalPosition<f64>,
    rect: &Rect,
    card: (f64, f64),
    offset: (f64, f64),
) -> (i32, i32) {
    let (icon, work, scale) = icon_frame(app, anchor, rect);
    let (x, y) = place_card(
        icon,
        (card.0 * scale, card.1 * scale),
        (offset.0 * scale, offset.1 * scale),
        work,
    );
    (x as i32, y as i32)
}

/// 兼容旧路径:窗口即卡片(无偏移)时按窗口尺寸定位。
fn menu_position(
    app: &AppHandle,
    anchor: PhysicalPosition<f64>,
    rect: &Rect,
    win_w: f64,
    win_h: f64,
    win_size_is_logical: bool,
) -> (i32, i32) {
    let (icon, work, scale) = icon_frame(app, anchor, rect);
    let (w, h) = if win_size_is_logical {
        (win_w * scale, win_h * scale)
    } else {
        (win_w, win_h)
    };
    let (x, y) = place_card(icon, (w, h), (0.0, 0.0), work);
    (x as i32, y as i32)
}

/// 右键托盘图标:在图标上方弹出菜单。
///
/// 优先用前端实测的**卡片盒子**定位:窗口的透明余量、DPI 取整都不再影响贴合。
pub fn show_menu(app: &AppHandle, anchor: PhysicalPosition<f64>, rect: Rect) -> Result<(), String> {
    let win = ensure_menu_window(app)?;
    if let Ok(mut slot) = LAST_ANCHOR.lock() {
        *slot = Some((anchor, rect));
    }
    let card = CARD_BOX.lock().ok().and_then(|c| *c);
    let (x, y) = match card {
        Some((cw, ch, ox, oy)) => menu_position_card(app, anchor, &rect, (cw, ch), (ox, oy)),
        None => {
            let (w, h) = win
                .outer_size()
                .map(|s| (s.width as f64, s.height as f64))
                .unwrap_or((MENU_W, MENU_H));
            menu_position(app, anchor, &rect, w, h, false)
        }
    };
    let _ = win.set_position(Position::Physical(PhysicalPosition::new(x, y)));
    let _ = win.show();
    let _ = win.set_focus();
    Ok(())
}

/// 前端测量出菜单卡片实际尺寸后回调:
/// 1. 记录卡片盒子(宽高 + 在窗口内的偏移),用于按卡片而非窗口定位;
/// 2. 把窗口收缩到「卡片 + 4px 余量」(避免高 DPI 取整裁切);
/// 3. 若已弹出过,按新盒子重新贴合托盘图标。
#[tauri::command]
pub fn tray_menu_fit(
    app: AppHandle,
    width: f64,
    height: f64,
    card_width: f64,
    card_height: f64,
    offset_x: f64,
    offset_y: f64,
) -> Result<(), String> {
    let win = app
        .get_webview_window(MENU_LABEL)
        .ok_or_else(|| "菜单窗口不存在".to_string())?;
    let cw = card_width.ceil().clamp(80.0, 480.0);
    let ch = card_height.ceil().clamp(40.0, 720.0);
    let w = width.max(cw + 4.0).clamp(80.0, 480.0);
    let h = height.max(ch + 4.0).clamp(40.0, 720.0);
    if let Ok(mut slot) = CARD_BOX.lock() {
        *slot = Some((cw, ch, offset_x.max(0.0), offset_y.max(0.0)));
    }
    let _ = win.set_size(Size::Logical(tauri::LogicalSize::new(w, h)));
    let anchor = LAST_ANCHOR.lock().ok().and_then(|s| *s);
    if let Some((pos, rect)) = anchor {
        let (x, y) = menu_position_card(&app, pos, &rect, (cw, ch), (offset_x, offset_y));
        let _ = win.set_position(Position::Physical(PhysicalPosition::new(x, y)));
    }
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
    use super::place_card;

    const WORK: (f64, f64, f64, f64) = (0.0, 0.0, 1920.0, 1080.0);
    /// 卡片铺满窗口(无偏移)时的等价写法
    fn place_menu(icon: (f64, f64, f64, f64), size: (f64, f64), work: Option<(f64, f64, f64, f64)>) -> (f64, f64) {
        place_card(icon, size, (0.0, 0.0), work)
    }

    /// 底部任务栏图标:菜单紧贴图标上沿并水平居中(GAP=0)。
    #[test]
    fn menu_sits_above_taskbar_icon() {
        let (x, y) = place_menu((1800.0, 1040.0, 24.0, 24.0), (216.0, 152.0), Some(WORK));
        assert_eq!((x, y), (1704.0, 888.0));
    }

    /// 图标贴近右边缘:菜单水平方向被夹回工作区内。
    #[test]
    fn menu_clamped_to_work_area() {
        let (x, _) = place_menu((1900.0, 1040.0, 20.0, 20.0), (216.0, 152.0), Some(WORK));
        assert_eq!(x, 1704.0);
    }

    /// 任务栏在屏幕顶部:上方无空间时紧贴图标下沿。
    #[test]
    fn menu_flips_below_when_no_room_above() {
        let (_, y) = place_menu((100.0, 4.0, 24.0, 24.0), (216.0, 152.0), Some(WORK));
        assert_eq!(y, 28.0);
    }

    /// 拿不到屏幕信息时仍紧贴图标上沿定位,不 panic。
    #[test]
    fn menu_without_work_area() {
        let (x, y) = place_menu((1800.0, 1040.0, 24.0, 24.0), (216.0, 152.0), None);
        assert_eq!((x, y), (1704.0, 888.0));
    }

    /// 窗口比卡片大(含透明余量):窗口位置要扣掉卡片偏移,
    /// 保证**卡片下沿**贴住图标上沿,而不是窗口下沿。
    #[test]
    fn card_offset_keeps_card_glued_to_icon() {
        // 卡片 120×118,窗口内偏移 (1,1) → 窗口位置 = 卡片目标位置 - (1,1)
        let (x, y) = place_card(
            (1800.0, 1040.0, 24.0, 24.0),
            (120.0, 118.0),
            (1.0, 1.0),
            Some(WORK),
        );
        // 卡片水平居中:1800 + 12 - 60 = 1752;卡片底 = 1040 → 卡片顶 = 922
        assert_eq!((x, y), (1751.0, 921.0));
    }
}
