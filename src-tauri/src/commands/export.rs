//! 通用导出命令(P0 自 commands/research 迁出,IPC 名不变):
//! 原生「另存为」对话框(pick_save_path)与打印对话框导出 PDF(export_pdf)。

use tauri::Manager;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::export::{ExportPathTokens, SavePathPick};

/// 弹出原生「另存为」对话框;用户取消返回 None。
/// 与 pick_data_dir 同款实现:阻塞调用放入 blocking 线程池,避免卡住运行时。
/// 选定路径登记为令牌(N-6),后续导出命令只认令牌。
#[tauri::command]
pub async fn pick_save_path(
    app: tauri::AppHandle,
    tokens: State<'_, ExportPathTokens>,
    title: String,
    default_name: String,
    filter_name: String,
    filter_ext: String,
) -> Result<Option<SavePathPick>, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title(title)
            .set_file_name(default_name)
            .add_filter(filter_name, &[filter_ext.as_str()])
            .blocking_save_file()
            .and_then(|p| p.into_path().ok())
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    // 此时应用尚未写入:存在即代表覆盖了用户的旧文件,系统对话框已征得确认
    let existed = picked.is_file();
    let token = tokens.issue(picked.clone());
    Ok(Some(SavePathPick {
        path: picked.display().to_string(),
        existed,
        token,
    }))
}

/// 触发主窗口的原生打印对话框(用户可选「另存为 PDF」)。
///
/// Tauri 2.12 的 WebView 未暴露 `print_to_pdf` 字节流接口,因此采用原生打印对话框:
/// 渲染层先把「待导出内容」以全屏白底覆盖层铺到主窗口,再调用本命令,
/// 系统打印对话框弹出后用户选择「Microsoft Print to PDF / 另存为 PDF」即可。
/// 这是跨平台(Windows/macOS/Linux)最稳定的 PDF 产出路径,且不依赖额外二进制依赖。
#[tauri::command]
pub async fn export_pdf(app: tauri::AppHandle) -> Result<(), String> {
    let webview = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口不存在,无法打印".to_string())?;
    webview.print().map_err(|e| format!("唤起打印失败: {e}"))
}
