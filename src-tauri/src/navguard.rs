//! M-15:顶级导航白名单守卫。
//!
//! 通过 Tauri 插件的 `on_navigation` 钩子覆盖**全部** webview
//! (包括 tauri.conf.json 静态创建的 main 窗口——它没有 Rust builder,
//! 无法挂逐窗口回调)。仅允许应用自身来源的顶级导航,阻止被注入/篡改的页面
//! 把窗口整体导航到任意外站(钓鱼页、针对旧 WebView2 内核的投放页等)。
//!
//! 说明:该回调只对应顶级框架导航,不含子资源与第三方 iframe;
//! 后端发起的外站请求另由 M-7(重定向收敛)/M-8(SSRF 地址校验)限制。

use tauri::plugin::{Builder as PluginBuilder, TauriPlugin};
use tauri::{Runtime, Url};

/// 判断目标 URL 是否属于应用自身来源。
fn is_app_origin(url: &Url) -> bool {
    match url.scheme() {
        // macOS / Linux 生产环境:tauri://localhost
        "tauri" => url.host_str() == Some("localhost"),
        // Windows 生产环境:页面 https://tauri.localhost,IPC 通道 http://ipc.localhost
        "http" | "https" => match url.host_str() {
            Some("tauri.localhost") | Some("ipc.localhost") => true,
            // 开发环境:Vite dev server(http://localhost:1420),仅调试构建放行
            Some("localhost") | Some("127.0.0.1") => cfg!(debug_assertions),
            _ => false,
        },
        _ => false,
    }
}

/// 构建导航守卫插件,需在 Builder 阶段(早于静态 main 窗口创建)注册。
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    PluginBuilder::<R>::new("nav-guard")
        .on_navigation(move |_webview, url| {
            let ok = is_app_origin(url);
            if !ok {
                log::warn!("on_navigation 拒绝外站顶级导航: {url}");
            }
            ok
        })
        .build()
}
