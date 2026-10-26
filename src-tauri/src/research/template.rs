//! 研究提示词模板(专项研究步骤 1):全局决策提示 / 终稿报告模板的自定义与落盘。
//!
//! - 内置模板即引擎常量(REACT_SYSTEM / REPORT_SYSTEM / REPORT_SYSTEM_EN),代码内保护不可删;
//! - 用户自定义覆盖落盘 `data/research/templates.json`,同 key 覆盖式更新;
//! - 生效优先级:ResearchParams 逐次覆盖 > templates.json 自定义 > 内置;
//! - 未知占位符原样保留(编辑器即时校验提示,不阻断保存)。

use std::fs;
use std::path::PathBuf;

use chrono::Local;
use serde::{Deserialize, Serialize};

use super::{REACT_SYSTEM, REPORT_SYSTEM, REPORT_SYSTEM_EN};

/// 合法模板 key。
pub const TEMPLATE_KEYS: [&str; 2] = ["decide", "report"];

/// 一条用户自定义覆盖。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateOverride {
    pub key: String,
    pub content: String,
    pub updated: String,
}

/// 前端模板编辑器 DTO。
#[derive(Debug, Clone, Serialize)]
pub struct TemplateDto {
    pub key: String,
    pub name: String,
    /// true = 当前生效为内置模板(无自定义覆盖)
    pub builtin: bool,
    /// 生效内容(自定义优先,否则内置)
    pub content: String,
    /// 支持的占位符(编辑器校验/速查侧栏用)
    pub placeholders: Vec<String>,
}

/// 模板展示名。
fn display_name(key: &str) -> &'static str {
    match key {
        "decide" => "全局决策提示",
        "report" => "终稿报告模板",
        _ => "未知模板",
    }
}

/// 各模板支持的占位符集合(编辑器校验白名单)。
pub fn placeholders_of(key: &str) -> Vec<String> {
    match key {
        "decide" => vec!["topic", "round", "max_rounds", "queries_per_round"],
        "report" => vec!["topic", "references", "output_lang"],
        _ => vec![],
    }
    .into_iter()
    .map(String::from)
    .collect()
}

/// 内置模板内容(report 按语言区分;编辑器展示用 zh)。
pub fn builtin_content(key: &str, lang: &str) -> Option<&'static str> {
    match key {
        "decide" => Some(REACT_SYSTEM),
        "report" => Some(if lang == "en" {
            REPORT_SYSTEM_EN
        } else {
            REPORT_SYSTEM
        }),
        _ => None,
    }
}

fn path() -> PathBuf {
    crate::paths::data_dir()
        .join("research")
        .join("templates.json")
}

/// 读取全部自定义覆盖(文件缺失/损坏返回空表,本地优先不报错)。
pub fn load_overrides() -> Vec<TemplateOverride> {
    let p = path();
    let Ok(text) = fs::read_to_string(&p) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_overrides(list: &[TemplateOverride]) -> Result<(), String> {
    let p = path();
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    fs::write(&p, text).map_err(|e| format!("write templates.json failed: {e}"))
}

/// 查某 key 的自定义覆盖。
pub fn find_override(key: &str) -> Option<TemplateOverride> {
    load_overrides().into_iter().find(|t| t.key == key)
}

/// 生效模板:自定义覆盖 > 内置。
pub fn effective(key: &str, lang: &str) -> String {
    if let Some(o) = find_override(key)
        && !o.content.trim().is_empty()
    {
        return o.content;
    }
    builtin_content(key, lang).unwrap_or_default().to_string()
}

/// 模板列表(编辑器初始数据;report 展示 zh 内置)。
pub fn list() -> Vec<TemplateDto> {
    TEMPLATE_KEYS
        .iter()
        .map(|key| {
            let o = find_override(key);
            TemplateDto {
                key: key.to_string(),
                name: display_name(key).to_string(),
                builtin: o.is_none(),
                content: o
                    .map(|x| x.content)
                    .unwrap_or_else(|| builtin_content(key, "zh").unwrap_or_default().to_string()),
                placeholders: placeholders_of(key),
            }
        })
        .collect()
}

/// 保存自定义覆盖,返回最新列表。
pub fn save(key: &str, content: &str) -> Result<Vec<TemplateDto>, String> {
    if !TEMPLATE_KEYS.contains(&key) {
        return Err(format!(
            "非法模板 key:{key}(可选:{})",
            TEMPLATE_KEYS.join("/")
        ));
    }
    if content.trim().is_empty() {
        return Err("模板内容不能为空;如需回到内置请使用「恢复默认」".into());
    }
    let mut list = load_overrides();
    let now = Local::now().to_rfc3339();
    if let Some(x) = list.iter_mut().find(|x| x.key == key) {
        x.content = content.to_string();
        x.updated = now;
    } else {
        list.push(TemplateOverride {
            key: key.to_string(),
            content: content.to_string(),
            updated: now,
        });
    }
    save_overrides(&list)?;
    Ok(self::list())
}

/// 恢复默认(删除自定义覆盖),返回最新列表。
pub fn reset(key: &str) -> Result<Vec<TemplateDto>, String> {
    if !TEMPLATE_KEYS.contains(&key) {
        return Err(format!("非法模板 key:{key}"));
    }
    let list: Vec<_> = load_overrides()
        .into_iter()
        .filter(|t| t.key != key)
        .collect();
    save_overrides(&list)?;
    Ok(self::list())
}

/// 渲染模板:替换已知占位符,未知的原样保留。
pub fn render(template: &str, vars: &[(&str, String)]) -> String {
    let mut out = template.to_string();
    for (k, v) in vars {
        let ph = format!("{{{k}}}");
        if out.contains(&ph) {
            out = out.replace(&ph, v);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_content_available_for_all_keys() {
        for k in TEMPLATE_KEYS {
            assert!(!builtin_content(k, "zh").unwrap_or_default().is_empty());
            assert!(!builtin_content(k, "en").unwrap_or_default().is_empty());
        }
        assert_ne!(
            builtin_content("report", "zh"),
            builtin_content("report", "en")
        );
        assert_eq!(
            builtin_content("decide", "zh"),
            builtin_content("decide", "en")
        );
    }

    #[test]
    fn save_rejects_bad_key_and_empty_content() {
        assert!(save("evil", "x").unwrap_err().contains("非法模板 key"));
        assert!(save("decide", "   ").unwrap_err().contains("不能为空"));
    }

    #[test]
    fn render_replaces_known_and_keeps_unknown() {
        let out = render(
            "主题 {topic},第 {round} 轮,{unknown} 保留",
            &[("topic", "AI".into()), ("round", "2".into())],
        );
        assert_eq!(out, "主题 AI,第 2 轮,{unknown} 保留");
    }

    #[test]
    fn placeholders_cover_keys() {
        assert!(placeholders_of("report").contains(&"references".to_string()));
        assert!(placeholders_of("decide").contains(&"round".to_string()));
        assert!(placeholders_of("other").is_empty());
    }
}
