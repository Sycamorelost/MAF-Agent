#![allow(dead_code)]

//! 工作区文件命令 —— 供设置页 / Agent 内置工具调用。
//!
//! 路径支持绝对路径(无沙箱),相对路径按工作区解析;写/删前自动备份,可用 `fs_restore` 回滚。

use crate::fs::{backup, formats, text};

/// 工作区信息(根目录与备份目录)。
#[tauri::command]
pub async fn fs_workspace_info(session_id: Option<String>) -> Result<serde_json::Value, String> {
    let root = text::root_for_session(session_id.as_deref())?;
    Ok(serde_json::json!({
        "root": root.display().to_string(),
        "backups": backup::backup_dir().display().to_string(),
    }))
}

/// 列出工作区目录(缺省为根目录)。
#[tauri::command]
pub async fn fs_list(rel: Option<String>, session_id: Option<String>) -> Result<Vec<text::FileEntry>, String> {
    text::list(rel.as_deref().unwrap_or(""), session_id.as_deref())
}

/// 读取文本文件(默认上限 20 万字符;offset 用于分页续读)。
#[tauri::command]
pub async fn fs_read_text(
    path: String,
    offset: Option<usize>,
    max_chars: Option<usize>,
    session_id: Option<String>,
) -> Result<String, String> {
    text::read_text(&path, offset, max_chars, session_id.as_deref())
}

/// 写入文本文件(自动创建父目录;覆盖前备份)。
#[tauri::command]
pub async fn fs_write_text(
    path: String,
    content: String,
    session_id: Option<String>,
) -> Result<text::WriteResult, String> {
    text::write_text(&path, &content, session_id.as_deref())
}

/// 删除文件(先备份)。
#[tauri::command]
pub async fn fs_delete(path: String, session_id: Option<String>) -> Result<text::WriteResult, String> {
    text::delete(&path, session_id.as_deref())
}

/// 列出最近的备份。
/// `session_id` 为 Some 时只返回该会话触发的备份;None 时返回全部。
#[tauri::command]
pub async fn fs_backup_list(session_id: Option<String>) -> Vec<backup::BackupEntry> {
    backup::list(session_id.as_deref())
}

/// 恢复到指定备份(恢复前会对当前文件再做一次快照)。
#[tauri::command]
pub async fn fs_restore(id: String, session_id: Option<String>) -> Result<String, String> {
    let root = text::root_for_session(session_id.as_deref())?;
    backup::restore(&root, &id)
}

/// 导出 docx(正文支持 Markdown;可指定用途模板、目录与封面元信息)。
#[tauri::command]
pub async fn fs_export_docx(
    path: String,
    title: String,
    body: String,
    profile: Option<String>,
    toc: Option<bool>,
    meta: Option<crate::fs::docgen::DocMeta>,
    session_id: Option<String>,
) -> Result<text::WriteResult, String> {
    let req = crate::fs::docgen::DocRequest::from_parts(
        &title,
        &body,
        profile.as_deref(),
        toc,
        meta.unwrap_or_default(),
    );
    text::export_docx_ext(&path, &req, session_id.as_deref())
}

/// 导出 xlsx(多工作表,每行字符串数组)。
#[tauri::command]
pub async fn fs_export_xlsx(
    path: String,
    sheets: Vec<text::SheetInput>,
    session_id: Option<String>,
) -> Result<text::WriteResult, String> {
    text::export_xlsx(&path, &sheets, session_id.as_deref())
}

/// 全格式文本提取(docx / xlsx / pdf / pptx / epub / html 等)。
#[tauri::command]
pub async fn fs_extract_text(path: String, session_id: Option<String>) -> Result<String, String> {
    formats::extract(&path, session_id.as_deref())
}

/// 列出 zip 内容。
#[tauri::command]
pub async fn fs_zip_list(path: String, session_id: Option<String>) -> Result<Vec<formats::ZipEntry>, String> {
    formats::zip_list(&path, session_id.as_deref())
}

/// 安全解包 zip 到工作区目录,返回文件数。
#[tauri::command]
pub async fn fs_zip_extract(
    path: String,
    dest: String,
    session_id: Option<String>,
) -> Result<usize, String> {
    formats::zip_extract(&path, &dest, session_id.as_deref())
}

/// 在资源管理器中打开工作区目录。
#[tauri::command]
pub async fn fs_open_workspace(session_id: Option<String>) -> Result<(), String> {
    open_dir(&text::root_for_session(session_id.as_deref())?)
}

/// 弹出系统文件夹选择器,返回用户选中的目录(用于设置默认工作区)。
#[tauri::command]
pub async fn fs_pick_workspace_dir(app: tauri::AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    // tauri-plugin-dialog 2.x blocking_* API 需在 spawn_blocking 中调用
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_folder()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.display().to_string())
    })
    .await
    .ok()
    .flatten();
    if let Some(ref path) = picked {
        log::info!("fs_pick_workspace_dir: {path}");
    }
    picked
}

/// 在资源管理器中打开备份目录。
#[tauri::command]
pub async fn fs_open_backups() -> Result<(), String> {
    let dir = backup::backup_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建备份目录失败: {e}"))?;
    open_dir(&dir)
}

/// 读取备份内容(P6 检查器 diff 视图;仅文本,截断 20 万字符)。
#[tauri::command]
pub async fn fs_backup_read(id: String) -> Result<String, String> {
    backup::read_payload(&id)
}

/// 在系统文件管理器中定位工作区内文件(P6 产物页签「定位目录」)。
#[tauri::command]
pub async fn fs_reveal(path: String, session_id: Option<String>) -> Result<(), String> {
    let root = text::root_for_session(session_id.as_deref())?;
    let abs = crate::fs::paths::resolve(&root, &path, true)?;
    if abs.is_dir() {
        return open_dir(&abs);
    }
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", abs.display()))
            .spawn()
            .map_err(|e| format!("定位文件失败: {e}"))?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    open_dir(abs.parent().ok_or("无法定位父目录")?)
}

/// 用系统默认应用打开工作区内文件(P6 产物页签「在应用中打开」)。
#[tauri::command]
pub async fn fs_open(path: String, session_id: Option<String>) -> Result<(), String> {
    let root = text::root_for_session(session_id.as_deref())?;
    let abs = crate::fs::paths::resolve(&root, &path, true)?;
    if abs.is_dir() {
        return open_dir(&abs);
    }
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(&abs)
        .spawn()
        .map_err(|e| format!("打开文件失败: {e}"))?;
    Ok(())
}

fn open_dir(dir: &std::path::Path) -> Result<(), String> {
    if !dir.exists() {
        return Err(format!("目录不存在:{}", dir.display()));
    }
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(dir)
        .spawn()
        .map_err(|e| format!("打开目录失败: {e}"))?;
    Ok(())
}
