//! myAgent 后端入口。
//!
//! 第三方来源声明 / Third-party attribution(详见仓库根目录 `LICENSE` 与 `NOTICE`):
//! - Cherry Studio        <https://github.com/CherryHQ/cherry-studio>     AGPL-3.0
//!   仅架构/交互理念参考,未复制源码、样式或资源
//! - MCP 参考服务器(MIT)与 agent-search-mcp(Apache-2.0)
//!   运行时经 npx 按需启动,不随安装包分发
//!
//! 本项目自身以 Apache License 2.0 发布。

mod agents;
mod agents_prompts;
mod attachment;
mod attachment_guard;
mod commands;
mod db;
mod deliverables;
mod diagnostics;
mod doc;
mod export;
mod fs;
mod gui;
mod mcp;
mod memory;
mod navguard;
mod notes_presets;
mod paths;
mod presets;
mod project_rules;
mod provider;
mod retrieval;
mod settings;
mod sys;
mod tasks;
mod websearch;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use std::sync::atomic::{AtomicU64, Ordering};

/// 当前毫秒时间戳(用于手势节流判断)。
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 托盘手势代次:每次有效单击/双击自增。延迟执行的单击先记住代次,
/// 到点时若代次已变(期间发生了第二次单击或双击)则放弃执行 —— 严格保证
/// 「双击只开小窗、单击只开主窗」,互不串台。
static CLICK_GEN: AtomicU64 = AtomicU64::new(0);
/// 双击后的抑制窗口(ms 时间戳):Windows 双击会附带额外的 Click 事件,
/// 在其后短时间内直接忽略 Click,避免双击结束后又触发一次单击。
static SUPPRESS_UNTIL_MS: AtomicU64 = AtomicU64::new(0);

/// 关闭主窗口时隐藏到托盘:先移除任务栏按钮,再 hide。
fn hide_to_tray(window: &tauri::Window) {
    let _ = window.set_skip_taskbar(true);
    let _ = window.hide();
}

/// 从托盘恢复主窗口:恢复任务栏按钮 → 还原最小化 → show → focus。
fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_skip_taskbar(false);
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// 创建系统托盘:
/// - 左键双击 → 打开 popout 小窗(显示并聚焦)
/// - 左键单击 → 打开主窗口(隐藏小窗后显示主窗)
/// - 菜单「显示」→ 恢复主窗口
/// - 菜单「退出」→ 结束应用
fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // 读取用户自定义应用名称(空 = "myAgent")
    let app_name = crate::settings::load()
        .custom_app_name
        .unwrap_or_else(|| "myAgent".to_string());
    let show_label = format!("显示 {}", app_name);
    let popout_label = "小窗对话";
    let quit = MenuItem::with_id(app, "tray_quit", "退出", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "tray_show", &show_label, true, None::<&str>)?;
    let popout_item = MenuItem::with_id(app, "tray_popout", popout_label, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &popout_item, &quit])?;
    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip(&app_name)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray_show" => show_main_window(app),
            "tray_popout" => crate::commands::popout::toggle_popout_from_tray(app),
            "tray_quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| match event {
            // 双击:只打开小窗,不作废延迟单击之外不做任何主窗动作
            TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => {
                CLICK_GEN.fetch_add(1, Ordering::SeqCst);
                SUPPRESS_UNTIL_MS.store(now_ms() + 500, Ordering::SeqCst);
                crate::commands::popout::show_popout_from_tray(tray.app_handle());
            }
            // 单击(仅在抬起时处理,避免按下/抬起各触发一次)
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => {
                if now_ms() < SUPPRESS_UNTIL_MS.load(Ordering::SeqCst) {
                    return;
                }
                let click_gen = CLICK_GEN.fetch_add(1, Ordering::SeqCst) + 1;
                let handle = tray.app_handle().clone();
                // 等一个双击判定窗口:期间若有第二次点击或双击,代次变化 → 放弃单击动作
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(320));
                    if CLICK_GEN.load(Ordering::SeqCst) != click_gen
                        || now_ms() < SUPPRESS_UNTIL_MS.load(Ordering::SeqCst)
                    {
                        return;
                    }
                    crate::commands::popout::hide_popout(&handle);
                    show_main_window(&handle);
                });
            }
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // M-15:顶级导航白名单(必须在静态 main 窗口创建前的 Builder 阶段注册)
        .plugin(navguard::init())
        .manage(memory::watcher::WatcherState::default())
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
                    // 初始化开始:前端轮询到 running 后显示「正在初始化组件…」(首启 npx 拉包阶段的卡死感来源)
                    mcp::set_bootstrap_state(1);
                    let rt = handle.state::<mcp::McpRuntime>();
                    for s in servers {
                        let name = s.name.clone();
                        if let Err(e) = rt.start_from_config(&s, &data, false).await {
                            log::warn!("MCP server `{name}` 自动启动失败: {e}");
                        } else {
                            log::info!("MCP server `{name}` 已随应用启动");
                            // P1-5:启动即预热工具列表缓存 —— 首轮对话的 mcp_collect_tools
                            // 直接命中,不再为每个 server 付一次 list_tools RPC(最坏 10s×N)
                            if let Err(e) = rt.list_tools_cached(&name).await {
                                log::warn!("MCP server `{name}` 工具列表预热失败: {e}");
                            }
                        }
                    }
                    mcp::set_bootstrap_state(2);
                    diagnostics::safe_emit(&handle, "bootstrap_done", ());
                });
            }

            // Phase 1 §3.1:SQLite 初始化(建目录 + 打开 + 迁移)
            db::init().map_err(|e| {
                log::error!("db init failed: {e}");
                std::io::Error::other(e)
            })?;

            // 预置知识库:首次启动落盘术语表/表格速查(用户可编辑/删除,删除不复活);
            // 失败仅记日志,不阻断启动。
            if let Err(e) = notes_presets::seed_if_needed() {
                log::warn!("预置知识库种子失败: {e}");
            }

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
            // P0:popout 小窗关闭 → 隐藏而非销毁(保留状态,下次 show 直接恢复)
            if window.label() == "popout" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
                return;
            }
            // 仅处理主窗口的关闭请求;最小化是普通最小化,不进托盘
            if window.label() != "main" {
                return;
            }
            // 点 × 的行为跟随「关闭时最小化到托盘」开关:
            //   开关开 → 不销毁窗口,隐藏到托盘(托盘「退出」才真正结束进程);
            //   开关关 → 直接结束应用
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if settings::load().minimize_to_tray {
                    api.prevent_close();
                    hide_to_tray(window);
                } else {
                    window.app_handle().exit(0);
                }
            }
            // 最小化始终是普通最小化,窗口留在任务栏,不进托盘
        })
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
