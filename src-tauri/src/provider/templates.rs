//! 内置 Provider 模板。
//!
//! 对应 plan/01-phase1-mvp.md §3.3:OpenAI / DeepSeek / Kimi / 通义 / 智谱 / 自定义。
//! 模板提供 id / 展示名 / base_url / 预置模型列表;api_key 由用户在设置页填入。

use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

/// Provider 模板(元数据)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTemplate {
    pub id: String,
    pub display_name: String,
    pub base_url: String,
    pub models: Vec<String>,
    /// 全部可用模型(含被用户停用的),按用户排序;仅设置页展示/启用开关使用。
    #[serde(default)]
    pub all_models: Vec<String>,
    /// 图像生成模型候选(走 /images/generations);为空表示该 Provider 无内置图像模型
    #[serde(default)]
    pub image_models: Vec<String>,
    /// 官网/文档链接,便于用户去拿 API Key
    pub docs_url: Option<String>,
    /// 是否 OpenAI 兼容(目前全部为 true,未来支持 Anthropic / Gemini 原生协议时扩展)
    pub openai_compat: bool,
}

/// 内置模板列表。顺序即 UI 展示顺序。
pub fn builtin_templates() -> Vec<ProviderTemplate> {
    vec![
        ProviderTemplate {
            id: "openai".into(),
            display_name: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            models: vec![
                "gpt-5.2".into(),
                "gpt-5.2-pro".into(),
                "gpt-4o".into(),
                "gpt-4o-mini".into(),
            ],
            image_models: vec!["dall-e-3".into(), "dall-e-2".into(), "gpt-image-1".into()],
            docs_url: Some("https://platform.openai.com/api-keys".into()),
            openai_compat: true,
            all_models: vec![],
        },
        ProviderTemplate {
            id: "deepseek".into(),
            display_name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            models: vec!["deepseek-chat".into(), "deepseek-reasoner".into()],
            image_models: vec![],
            docs_url: Some("https://platform.deepseek.com/api_keys".into()),
            openai_compat: true,
            all_models: vec![],
        },
        ProviderTemplate {
            id: "kimi".into(),
            display_name: "Kimi (Moonshot)".into(),
            base_url: "https://api.moonshot.cn/v1".into(),
            models: vec![
                "kimi-k2.5".into(),
                "kimi-k2".into(),
                "moonshot-v1-128k".into(),
            ],
            image_models: vec![],
            docs_url: Some("https://platform.moonshot.cn/console/api-keys".into()),
            openai_compat: true,
            all_models: vec![],
        },
        ProviderTemplate {
            id: "qwen".into(),
            display_name: "通义千问 (DashScope)".into(),
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
            models: vec![
                "qwen-max".into(),
                "qwen-plus".into(),
                "qwen-turbo".into(),
                "qwen-long".into(),
            ],
            image_models: vec![],
            docs_url: Some("https://bailian.console.aliyun.com/".into()),
            openai_compat: true,
            all_models: vec![],
        },
        ProviderTemplate {
            id: "zhipu".into(),
            display_name: "智谱 GLM".into(),
            base_url: "https://open.bigmodel.cn/api/paas/v4".into(),
            models: vec!["glm-4.5".into(), "glm-4-plus".into(), "glm-4-air".into()],
            // 智谱图像模型走同一 base_url 的 /images/generations
            image_models: vec!["cogview-3-plus".into(), "cogview-3-flash".into()],
            docs_url: Some("https://open.bigmodel.cn/usercenter/apikeys".into()),
            openai_compat: true,
            all_models: vec![],
        },
        ProviderTemplate {
            id: "deepl".into(),
            display_name: "DeepL".into(),
            base_url: "https://api-free.deepl.com/v2".into(),
            models: vec![], // 专用翻译 Provider,不走对话模型
            image_models: vec![],
            docs_url: Some("https://www.deepl.com/pro-api".into()),
            openai_compat: false,
            all_models: vec![],
        },
        ProviderTemplate {
            id: "custom".into(),
            display_name: "自定义 (OpenAI 兼容)".into(),
            base_url: "".into(),
            models: vec![],
            image_models: vec![],
            docs_url: None,
            openai_compat: true,
            all_models: vec![],
        },
    ]
}

/// 模板持久化文件:`<data_dir>/provider_templates.json`(用户可增删改的全量列表)。
fn store_path() -> PathBuf {
    crate::paths::data_dir().join("provider_templates.json")
}

/// 首次种子:内置模板 + 旧版 `custom_provider.json` 合并(仅读,不落盘)。
fn seeded() -> Vec<ProviderTemplate> {
    let mut list = builtin_templates();
    if let Ok(Some(cfg)) = crate::provider::custom::load()
        && let Some(t) = list.iter_mut().find(|t| t.id == "custom")
    {
        t.display_name = cfg.display_name;
        t.base_url = cfg.base_url;
        t.models = cfg.models;
    }
    list
}

/// 读取全部模板。文件不存在时返回内置种子列表(不落盘;首次增删改时才写入)。
pub fn load_all() -> Vec<ProviderTemplate> {
    let p = store_path();
    if p.exists()
        && let Ok(text) = fs::read_to_string(&p)
        && let Ok(list) = serde_json::from_str::<Vec<ProviderTemplate>>(&text)
    {
        return list;
    }
    seeded()
}

/// 覆盖写全部模板。
pub fn save_all(list: &[ProviderTemplate]) -> Result<(), String> {
    let p = store_path();
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir data failed: {e}"))?;
    }
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    fs::write(&p, text).map_err(|e| format!("write provider_templates.json failed: {e}"))
}

/// 按 id 查模板(以用户持久化列表为准)。
pub fn find_template(id: &str) -> Option<ProviderTemplate> {
    load_all().into_iter().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_templates_have_unique_id() {
        let tpls = builtin_templates();
        let mut ids = tpls.iter().map(|t| &t.id).collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), tpls.len());
    }

    #[test]
    fn builtin_contains_openai() {
        let t = builtin_templates()
            .into_iter()
            .find(|t| t.id == "openai")
            .unwrap();
        assert_eq!(t.display_name, "OpenAI");
        assert!(t.base_url.starts_with("https://"));
    }
}
