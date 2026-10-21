mod agents;
mod attachment;
mod commands;
mod db;
mod memory;
mod navguard;
mod paths;
mod provider;
mod research;
mod selection;
mod settings;
mod study;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

/// 从托盘恢复主窗口:show + unminimize + focus。
fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// 创建系统托盘:左键点击 / 菜单「显示」恢复主窗口,菜单「退出」结束应用。
fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "tray_show", "显示 MAF Agent", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray_quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("MAF Agent")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray_show" => show_main_window(app),
            "tray_quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, .. } = event {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}


pub fn run() {
  // 进程级 DPI 感知:保证划词等全局坐标按物理像素计算,不偏移。
  selection::ensure_dpi_aware();

  tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    // M-15:顶级导航白名单(必须在静态 main 窗口创建前的 Builder 阶段注册)
    .plugin(navguard::init())
    .manage(memory::watcher::WatcherState::default())
    .manage(commands::research::ResearchRegistry::default())
    .manage(commands::study::StudyRegistry::default())
    .manage(provider::rotation::RotationState::default())
    .manage(commands::provider::StreamCancelState::default())
    .manage(commands::research::ExportPathTokens::default())
    .invoke_handler(commands::all_commands!())
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      // 数据目录运行时解析 + 执行「待迁移」(必须早于 db::init)
      paths::init(app.handle()).map_err(|e| {
        log::error!("paths init failed: {e}");
        std::io::Error::new(std::io::ErrorKind::Other, e)
      })?;

      // 科研增强:解析内置技能资源目录(未命中打包资源时回退源码树,注入层静默降级)
      research::skills::init_resource(app.handle());

      // Phase 1 §3.1:SQLite 初始化(建目录 + 打开 + 迁移)
      db::init().map_err(|e| {
        log::error!("db init failed: {e}");
        std::io::Error::new(std::io::ErrorKind::Other, e)
      })?;

      // L1:研究任务是进程内任务,重启后 running/paused 无法恢复,统一标记为中断
      match db::conn().lock() {
        Ok(conn) => {
          if let Err(e) = research::store::mark_interrupted(&conn) {
            log::error!("mark interrupted research failed: {e}");
          }
          // 学习规划同款:generating 中的计划重启后无法恢复
          if let Err(e) = study::store::mark_interrupted(&conn) {
            log::error!("mark interrupted plans failed: {e}");
          }
        }
        Err(e) => log::error!("db lock failed: {e}"),
      }

      // 迭代 D9:启动全局输入监听(划词提问)
      selection::spawn_listener(app.handle().clone());

      // 系统托盘:最小化进托盘后从这里恢复主窗口
      if let Err(e) = setup_tray(app.handle()) {
        log::error!("tray setup failed: {e}");
      }

      Ok(())
    })
    .on_window_event(|window, event| {
      // 主窗口最小化时按设置隐藏到托盘(覆盖任务栏右键等一切最小化路径)
      if window.label() != "main" {
        return;
      }
      match event {
        // 点 × 的行为跟随「最小化到托盘」开关:
        //   开关开 → 不销毁窗口,隐藏到托盘(托盘「退出」才真正结束进程);
        //   开关关 → 直接结束应用
        tauri::WindowEvent::CloseRequested { api, .. } => {
          if settings::load().minimize_to_tray {
            api.prevent_close();
            let _ = window.hide();
          } else {
            window.app_handle().exit(0);
          }
        }
        tauri::WindowEvent::Resized(_) => {
          if settings::load().minimize_to_tray && window.is_minimized().unwrap_or(false) {
            let _ = window.hide();
          }
        }
        _ => {}
      }
    })
    .run(tauri::generate_context!())
    .expect("error while building tauri application");
}
