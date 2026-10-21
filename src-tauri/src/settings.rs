//! 应用级设置(迭代 D3.7,迭代 D9 扩展划词提问),落盘 `data/settings.json`。

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

/// 应用设置。所有字段缺省 = 走默认行为。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// 提炼专用 provider_id(空 = 用当前会话模型)
    #[serde(default)]
    pub summarize_provider_id: Option<String>,
    /// 提炼专用模型(空 = 用当前会话模型)
    #[serde(default)]
    pub summarize_model: Option<String>,
    /// 划词提问:任意应用中连按两次 Ctrl+C 唤起主窗口
    #[serde(default = "default_true")]
    pub selection_ask_enabled: bool,
    /// 主窗口最小化时隐藏到托盘(默认关:最小化保留任务栏图标,点 × 才进托盘)
    #[serde(default)]
    pub minimize_to_tray: bool,
    /// 导航菜单中隐藏的功能项(对话/设置始终显示;空 = 全部显示)
    #[serde(default)]
    pub hidden_nav_items: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            summarize_provider_id: None,
            summarize_model: None,
            selection_ask_enabled: true,
            minimize_to_tray: false,
            hidden_nav_items: Vec::new(),
        }
    }
}

fn settings_path() -> PathBuf {
    crate::paths::data_dir().join("settings.json")
}

/// 读取设置;文件缺失/损坏时返回默认值(不报错,保证本地优先可用)。
pub fn load() -> AppSettings {
    let p = settings_path();
    let Ok(text) = fs::read_to_string(&p) else {
        return AppSettings::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// 保存设置(覆盖写)。
pub fn save(settings: &AppSettings) -> Result<(), String> {
    let p = settings_path();
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&p, text).map_err(|e| format!("write {} failed: {e}", p.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_json_roundtrip() {
        let s = AppSettings {
            summarize_provider_id: Some("deepseek".into()),
            summarize_model: Some("deepseek-chat".into()),
            ..Default::default()
        };
        let text = serde_json::to_string(&s).unwrap();
        let back: AppSettings = serde_json::from_str(&text).unwrap();
        assert_eq!(back.summarize_provider_id.as_deref(), Some("deepseek"));
        assert_eq!(back.summarize_model.as_deref(), Some("deepseek-chat"));
        assert!(back.selection_ask_enabled);
        assert!(!back.minimize_to_tray);
        assert!(back.hidden_nav_items.is_empty());
    }

    #[test]
    fn settings_default_when_fields_missing() {
        // 旧版 settings.json(无字段)或空对象都能解析为默认
        let back: AppSettings = serde_json::from_str("{}").unwrap();
        assert!(back.summarize_provider_id.is_none());
        assert!(back.summarize_model.is_none());
        assert!(back.selection_ask_enabled);
        assert!(!back.minimize_to_tray);
    }
}
