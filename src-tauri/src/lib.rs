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

use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

/// 关闭主窗口时隐藏到托盘:先移除任务栏按钮,再 hide。
fn hide_to_tray(window: &tauri::Window) {
    let _ = window.set_skip_taskbar(true);
    let _ = window.hide();
}

/// 从托盘恢复主窗口:恢复任务栏按钮 → 还原最小化 → show → focus。
pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_skip_taskbar(false);
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// 创建系统托盘:
/// - 左键双击 → 打开小窗(显示并聚焦)
/// - 左键单击 → 不做任何动作
/// - 右键 → 在托盘图标上方弹出同主题自定义菜单(显示主窗 / 小窗 / 退出)
fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // 读取用户自定义应用名称(空 = "myAgent")
    let app_name = crate::settings::load()
        .custom_app_name
        .unwrap_or_else(|| "myAgent".to_string());
    let mut builder = TrayIconBuilder::new()
        .tooltip(&app_name)
        .on_tray_icon_event(|tray, event| match event {
            // 双击:打开小窗(唯一保留的单击类手势)
            TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => {
                crate::commands::popout::show_popout_from_tray(tray.app_handle());
            }
            // 右键抬起:弹出同主题菜单(原生菜单无法跟随主题,改为自绘窗口)
            TrayIconEvent::Click {
                button: MouseButton::Right,
                button_state: MouseButtonState::Up,
                position,
                rect,
                ..
            } => {
                if let Err(e) = crate::commands::tray::show_menu(tray.app_handle(), position, rect)
                {
                    log::warn!("托盘菜单打开失败: {e}");
                }
            }
            // 左键单击:取消动作(避免与双击争抢)
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    // 预建隐藏的菜单窗口:首次右键即时弹出
    crate::commands::tray::preload(app);
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
        .manage(commands::provider::chat::run::ChatRunRegistry::default())
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

            // 开箱即用但不过度预置(P1-6):仅当用户打开 MCP 工具总开关后才拉起已启用 server;
            // 未打开时完全懒启动(首次 mcp_collect_tools 再拉起),避免首启 4 个 npx 串行下载。
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let data = paths::data_dir();
                    let cfg = mcp::load_or_init(&data);
                    if !cfg.tools_enabled {
                        return;
                    }
                    let servers: Vec<mcp::McpServerConfig> =
                        cfg.servers.into_iter().filter(|s| s.enabled).collect();
                    if servers.is_empty() {
                        return;
                    }
                    mcp::set_bootstrap_state(1);
                    let rt = handle.state::<mcp::McpRuntime>();
                    // 并行拉起 + 总超时:串行时 4 个 npx 首启会逐个累加等待;
                    // 单个 server 的启动超时仍由 start_from_config 内部控制。
                    let started = tokio::time::timeout(
                        std::time::Duration::from_secs(20),
                        futures::future::join_all(
                            servers
                                .iter()
                                .map(|s| rt.start_from_config(s, &data, false)),
                        ),
                    )
                    .await;
                    match started {
                        Ok(results) => {
                            for (s, r) in servers.iter().zip(results) {
                                match r {
                                    Ok(()) => log::info!("MCP server `{}` 已随应用启动", s.name),
                                    Err(e) => {
                                        log::warn!("MCP server `{}` 自动启动失败: {e}", s.name)
                                    }
                                }
                            }
                        }
                        Err(_) => log::warn!(
                            "MCP server 并行启动总超时(20s),其余保持未启动,首次调用时再懒启动"
                        ),
                    }
                    // 不再启动期预热工具列表:预热会让没用到外部工具的会话也付 list_tools RPC
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

            // 小窗预建(隐藏):WebView2 在托盘事件回调里创建会初始化不完整(白屏卡死),
            // 与 tray-menu 一样放到 setup 阶段建,托盘双击时只 show。
            if let Err(e) = crate::commands::popout::preload(app.handle()) {
                log::warn!("小窗预建失败: {e}");
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
