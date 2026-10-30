//! 数据目录 commands(迭代:数据目录可自定义)。
//!
//! 路径解析与迁移的实现在 `crate::paths`;本模块只负责把能力暴露给前端:
//!   get_data_dir / pick_data_dir / set_data_dir / open_data_dir / restart_app

use serde::Serialize;
use tauri_plugin_dialog::DialogExt;

use crate::paths;

/// 当前数据目录状态(供设置页与首次启动引导展示)。
#[derive(Debug, Serialize)]
pub struct DataDirInfo {
    /// 当前数据根目录(绝对路径)
    pub path: String,
    /// 引导配置文件路径(exe 旁的 config.json,不可写时回退 %APPDATA%)
    pub config_path: String,
    /// 是否已由用户显式选定过
    pub configured: bool,
    /// 是否采用了历史数据目录(老版本升级,未配置但已有数据)
    pub legacy_adopted: bool,
    /// 当前目录是否已存在数据库文件
    pub has_data: bool,
    /// 上次迁移失败的原因(有则设置页可见,便于用户处理)
    pub migration_error: Option<String>,
    /// 是否为开发构建(开发期不弹首次启动引导)
    pub is_dev: bool,
}

/// 读取当前数据目录状态。
#[tauri::command]
pub async fn get_data_dir() -> DataDirInfo {
    let dir = paths::data_dir();
    DataDirInfo {
        has_data: dir.join("metaagent.db").exists(),
        path: dir.display().to_string(),
        config_path: paths::config_path().display().to_string(),
        configured: paths::is_configured(),
        legacy_adopted: paths::legacy_adopted(),
        migration_error: paths::migration_error(),
        is_dev: cfg!(debug_assertions),
    }
}

/// 弹出原生文件夹选择器;用户取消返回 None。
/// `blocking_pick_folder` 是阻塞调用,放到 blocking 线程池执行,避免卡住运行时。
#[tauri::command]
pub async fn pick_data_dir(app: tauri::AppHandle) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_folder()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.display().to_string())
    })
    .await
    .ok()
    .flatten()
}

/// 切换数据目录:写引导配置;数据迁移在下次启动、无文件句柄时执行。
#[tauri::command]
pub async fn set_data_dir(path: String, migrate: bool) -> Result<paths::SetDirResult, String> {
    paths::set_data_dir(std::path::Path::new(&path), migrate)
}

/// 在资源管理器中打开当前数据目录。
#[tauri::command]
pub async fn open_data_dir() -> Result<(), String> {
    let dir = paths::data_dir();
    if !dir.exists() {
        return Err(format!("目录不存在:{}", dir.display()));
    }
    // explorer 用分离参数传路径,兼容含空格目录;退出码非 0 属正常,忽略
    std::process::Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map_err(|e| format!("打开目录失败:{e}"))?;
    Ok(())
}

/// 重启应用(迁移与目录切换需要新进程才能生效)。
#[tauri::command]
pub async fn restart_app(app: tauri::AppHandle) {
    app.restart();
}
