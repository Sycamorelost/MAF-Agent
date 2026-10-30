//! 应用设置 commands(迭代 D3.7):提炼专用小模型等,落盘 data/settings.json。

use crate::settings::{self, AppSettings};

/// 读取应用设置(缺失时返回默认值)。
#[tauri::command]
pub async fn get_settings() -> AppSettings {
    settings::load()
}

/// 保存应用设置(覆盖写)。
#[tauri::command]
pub async fn save_settings(settings: AppSettings) -> Result<(), String> {
    settings::save(&settings)
}
