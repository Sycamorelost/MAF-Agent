//! myAgent 后端入口。
//!
//! 第三方来源声明 / Third-party attribution(详见仓库根目录 `LICENSE` 与 `NOTICE`):
//! - Deep-tutor           <https://github.com/hershate/Deep-tutor>        Apache-2.0
//!   衍生:study/**、study/template.rs(Rust 重写)
//! - Cherry Studio        <https://github.com/CherryHQ/cherry-studio>     AGPL-3.0
//!   仅架构/交互理念参考,未复制源码、样式或资源
//!
//! 本项目自身以 Apache License 2.0 发布。

mod agents;
mod agents_prompts;
mod aiflavor;
mod attachment;
mod commands;
mod db;
mod doc;
mod export;
mod fs;
mod gui;
mod mcp;
mod memory;
mod navguard;
mod paths;
mod presets;
mod provider;
mod retrieval;
mod selection;
mod settings;
mod study;
mod sys;
mod tasks;
mod websearch;

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
    // 读取用户自定义应用名称(空 = "myAgent")
    let app_name = crate::settings::load()
        .custom_app_name
        .unwrap_or_else(|| "myAgent".to_string());
    let show_label = format!("显示 {}", app_name);
    let show = MenuItem::with_id(app, "tray_show", &show_label, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray_quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip(&app_name)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray_show" => show_main_window(app),
            "tray_quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = event
            {
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
        .manage(commands::study::StudyRegistry::default())
        .manage(provider::rotation::RotationState::default())
        .manage(commands::provider::StreamCancelState::default())
        .manage(export::path::ExportPathTokens::default())
        .manage(commands::mcp::McpState(std::sync::Mutex::new(
            mcp::McpConfig::default(),
        )))
        .manage(mcp::McpRuntime::default())
        .manage(mcp::ApprovalStore::default())
        .manage(mcp::CapabilityStore::default())
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
                std::io::Error::other(e)
            })?;

            // MCP 配置预加载:McpState 默认是空配置,若不在这里灌一次磁盘配置,
            // 用户没打开过「设置 → MCP 工具」时聊天将拿不到任何工具(mcp_collect_tools 读到空列表)。
            if let Some(st) = app.try_state::<commands::mcp::McpState>() {
                let cfg = mcp::load_or_init(&paths::data_dir());
                if let Ok(mut g) = st.0.lock() {
                    *g = cfg;
                }
            } else {
                log::warn!("McpState 未注册,聊天可能拿不到 MCP 工具");
            }

            // 开箱即用:应用启动后自动拉起所有已启用的 MCP server(后台顺序启动,失败仅记日志,
            // 用户仍可在设置页手动重启);首次启动 npx 需下载包,国内镜像已默认开启。
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let data = paths::data_dir();
                    let servers: Vec<mcp::McpServerConfig> = mcp::load_or_init(&data)
                        .servers
                        .into_iter()
                        .filter(|s| s.enabled)
                        .collect();
                    if servers.is_empty() {
                        return;
                    }
                    let rt = handle.state::<mcp::McpRuntime>();
                    for s in servers {
                        let name = s.name.clone();
                        if let Err(e) = rt.start_from_config(&s, &data).await {
                            log::warn!("MCP server `{name}` 自动启动失败: {e}");
                        } else {
                            log::info!("MCP server `{name}` 已随应用启动");
                        }
                    }
                });
            }

            // 解析内置 AI 味资源目录(未命中打包资源时回退源码树,注入层静默降级)
            aiflavor::init_resource(app.handle());

            // Phase 1 §3.1:SQLite 初始化(建目录 + 打开 + 迁移)
            db::init().map_err(|e| {
                log::error!("db init failed: {e}");
                std::io::Error::other(e)
            })?;

            // 学习规划:generating 中的计划属进程内任务,重启后无法恢复,统一标记为中断
            match db::conn().lock() {
                Ok(conn) => {
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

            // DEF-01 首帧白屏自愈:dev 模式下窗口创建与 Vite 首次编译存在竞态,
            // WebView 可能停在空白导航(UIA 树为空、F5 后才恢复)。
            // 延迟 2s 由页面自评:body 无可见文本则自动 reload;正常页面无副作用。
            // sessionStorage 计数守卫,最多 reload 2 次防死循环。
            if let Some(w) = app.get_webview_window("main") {
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(2000));
                    let _ = w.eval(
                        r#"(function(){try{
              var k='__maf_reload_guard';
              var n=parseInt(sessionStorage.getItem(k)||'0',10);
              var b=document.body;
              if((!b||b.innerText.trim().length===0)&&n<2){
                sessionStorage.setItem(k,String(n+1));
                location.reload();
              }
            }catch(e){}})();"#,
                    );
                });
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
                tauri::WindowEvent::Resized(_)
                    if settings::load().minimize_to_tray
                        && window.is_minimized().unwrap_or(false) =>
                {
                    let _ = window.hide();
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
