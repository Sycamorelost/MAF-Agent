//! 旧版「自定义 Provider」配置读取(仅用于一次性迁移到 `data/provider_templates.json`)。
//!
//! 模板现已由用户在设置页自由增删改(templates::load_all / save_all),
//! 本模块只保留读取能力,把 `data/custom_provider.json` 合并进种子列表。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 旧版自定义 Provider 配置(仅 display_name / base_url / models)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomProviderConfig {
    pub display_name: String,
    pub base_url: String,
    pub models: Vec<String>,
}

fn config_path() -> PathBuf {
    crate::paths::data_dir().join("custom_provider.json")
}

/// 读取旧版配置;不存在或解析失败返回 Ok(None)。
pub fn load() -> Result<Option<CustomProviderConfig>, String> {
    let p = config_path();
    if !p.exists() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(&p).map_err(|e| format!("read custom config failed: {e}"))?;
    let cfg: CustomProviderConfig =
        serde_json::from_str(&text).map_err(|e| format!("parse custom config failed: {e}"))?;
    Ok(Some(cfg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_legacy_config() {
        let cfg: CustomProviderConfig = serde_json::from_str(
            r#"{"display_name":"我的网关","base_url":"https://gw.example.com/v1","models":["m1","m2"]}"#,
        )
        .unwrap();
        assert_eq!(cfg.display_name, "我的网关");
        assert_eq!(cfg.models, vec!["m1", "m2"]);
    }
}
