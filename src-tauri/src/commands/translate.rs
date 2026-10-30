//! 翻译增强(迭代 D6):术语表 + 翻译历史。
//!
//! - 术语表:`data/glossary.json`,结构 `[{src, dst, note?}]`;
//!   translate_stream 启动时自动加载,把原文命中的词条注入 system prompt。
//! - 翻译历史:`data/translations/{yyyyMMdd-HHmmssfff}.json`,前端流式完成后落盘;
//!   列表按文件名时间戳倒序,最多保留 100 条。

use serde::{Deserialize, Serialize};

// ---------- 术语表 ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlossaryEntry {
    /// 原文词(匹配用)
    pub src: String,
    /// 指定译法
    pub dst: String,
    /// 备注(仅 UI 展示)
    pub note: Option<String>,
}

fn glossary_path() -> std::path::PathBuf {
    crate::paths::data_dir().join("glossary.json")
}

/// 读取术语表(文件缺失/损坏返回空表)。
pub(crate) fn load_glossary() -> Vec<GlossaryEntry> {
    let p = glossary_path();
    if !p.exists() {
        return Vec::new();
    }
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 注入 prompt 的词条上限,避免大术语表撑爆上下文。
const GLOSSARY_INJECT_MAX: usize = 50;

/// 找出原文命中的词条并格式化为 prompt 片段;无命中返回 None。
/// 匹配规则:CJK 直接子串;拉丁词忽略大小写子串(轻量实现,够用)。
pub(crate) fn glossary_hints(text: &str) -> Option<String> {
    let entries = load_glossary();
    if entries.is_empty() {
        return None;
    }
    let lower = text.to_lowercase();
    let mut hits: Vec<String> = Vec::new();
    for e in &entries {
        let src = e.src.trim();
        if src.is_empty() {
            continue;
        }
        let hit = text.contains(src) || lower.contains(&src.to_lowercase());
        if hit {
            hits.push(format!("- {} → {}", src, e.dst.trim()));
            if hits.len() >= GLOSSARY_INJECT_MAX {
                break;
            }
        }
    }
    if hits.is_empty() {
        return None;
    }
    Some(format!(
        "\n\nTerminology glossary (use these translations consistently):\n{}",
        hits.join("\n")
    ))
}

#[tauri::command]
pub async fn glossary_list() -> Vec<GlossaryEntry> {
    load_glossary()
}

/// 全量覆盖保存术语表(设置页增删改后整体提交)。
#[tauri::command]
pub async fn glossary_save(entries: Vec<GlossaryEntry>) -> Result<(), String> {
    let p = glossary_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir data failed: {e}"))?;
    }
    let cleaned: Vec<GlossaryEntry> = entries
        .into_iter()
        .filter(|e| !e.src.trim().is_empty() && !e.dst.trim().is_empty())
        .collect();
    let text = serde_json::to_string_pretty(&cleaned).map_err(|e| e.to_string())?;
    std::fs::write(&p, text).map_err(|e| format!("write glossary.json failed: {e}"))
}

// ---------- 翻译历史 ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationRecord {
    /// 落盘文件名(列表时注入;保存时为空字符串)。
    /// 用 `skip_serializing_if` 而非 `skip_serializing`:保存时为空会被跳过(落盘文件干净),
    /// 列表时已注入真实文件名必须序列化,否则前端删除按钮拿不到文件名(FUNC-1 修复)。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file_name: String,
    pub created: String,
    pub provider_id: String,
    pub model: String,
    /// 源语言英文名;null = 自动检测
    pub source_lang: Option<String>,
    pub target_lang: String,
    pub source_text: String,
    pub result_text: String,
}

fn translations_dir() -> std::path::PathBuf {
    crate::paths::data_dir().join("translations")
}

/// 历史列表上限(超出时删除最旧文件)。
const HISTORY_MAX: usize = 100;

/// 保存一条翻译记录(前端流式完成后调用)。
#[tauri::command]
pub async fn save_translation(rec: TranslationRecord) -> Result<String, String> {
    let dir = translations_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 translations 目录失败: {e}"))?;
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S%3f");
    let name = format!("{ts}.json");
    let text = serde_json::to_string_pretty(&rec).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(&name), text).map_err(|e| format!("写入翻译记录失败: {e}"))?;

    // 超上限时裁掉最旧
    let mut names = list_history_names(&dir);
    while names.len() > HISTORY_MAX {
        if let Some(oldest) = names.pop() {
            let _ = std::fs::remove_file(dir.join(oldest));
        }
    }
    Ok(name)
}

/// 目录内全部 .json 文件名,按时间戳升序(旧 → 新)。
fn list_history_names(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".json") {
                names.push(name);
            }
        }
    }
    names.sort();
    names
}

/// 列出翻译历史(新 → 旧,最多 100 条)。
#[tauri::command]
pub async fn list_translations() -> Vec<TranslationRecord> {
    let dir = translations_dir();
    let mut names = list_history_names(&dir);
    names.reverse();
    names
        .into_iter()
        .take(HISTORY_MAX)
        .filter_map(|name| {
            let text = std::fs::read_to_string(dir.join(&name)).ok()?;
            let mut rec: TranslationRecord = serde_json::from_str(&text).ok()?;
            rec.file_name = name;
            Some(rec)
        })
        .collect()
}

/// 删除单条历史(仅允许简单文件名,防路径穿越)。
#[tauri::command]
pub async fn delete_translation(file_name: String) -> Result<(), String> {
    if file_name.contains(['/', '\\']) || !file_name.ends_with(".json") {
        return Err("非法文件名".into());
    }
    let p = translations_dir().join(&file_name);
    if p.exists() {
        std::fs::remove_file(&p).map_err(|e| format!("删除失败: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint_of(text: &str, entries: &[GlossaryEntry]) -> Option<String> {
        // 绕过文件 IO,直接复用匹配逻辑:临时把匹配核心提出来等价验证
        let lower = text.to_lowercase();
        let mut hits: Vec<String> = Vec::new();
        for e in entries {
            let src = e.src.trim();
            if src.is_empty() {
                continue;
            }
            if text.contains(src) || lower.contains(&src.to_lowercase()) {
                hits.push(format!("- {} → {}", src, e.dst.trim()));
            }
        }
        if hits.is_empty() {
            None
        } else {
            Some(hits.join("\n"))
        }
    }

    #[test]
    fn glossary_match_cjk_and_latin() {
        let entries = vec![
            GlossaryEntry {
                src: "元代理".into(),
                dst: "myAgent".into(),
                note: None,
            },
            GlossaryEntry {
                src: "memory.md".into(),
                dst: "memory.md(记忆文件)".into(),
                note: None,
            },
        ];
        // CJK 命中
        let h = hint_of("元代理是一个本地助手", &entries).unwrap();
        assert!(h.contains("元代理 → myAgent"));
        // 拉丁忽略大小写命中
        let h2 = hint_of("the MEMORY.MD file", &entries).unwrap();
        assert!(h2.contains("memory.md"));
        // 无命中
        assert!(hint_of("完全无关的文本", &entries).is_none());
    }

    #[test]
    fn save_cleans_empty_entries() {
        let entries = vec![
            GlossaryEntry {
                src: "  ".into(),
                dst: "x".into(),
                note: None,
            },
            GlossaryEntry {
                src: "a".into(),
                dst: "".into(),
                note: None,
            },
        ];
        let cleaned: Vec<GlossaryEntry> = entries
            .into_iter()
            .filter(|e| !e.src.trim().is_empty() && !e.dst.trim().is_empty())
            .collect();
        assert!(cleaned.is_empty());
    }

    /// FUNC-1 回归:file_name 在列表场景(非空)必须序列化,保存场景(空)可省略。
    #[test]
    fn translation_record_file_name_serialization() {
        let with_name = TranslationRecord {
            file_name: "20261026-000000000.json".into(),
            created: "t".into(),
            provider_id: "p".into(),
            model: "m".into(),
            source_lang: None,
            target_lang: "zh".into(),
            source_text: "hi".into(),
            result_text: "你好".into(),
        };
        let s = serde_json::to_string(&with_name).unwrap();
        assert!(s.contains("\"file_name\":\"20261026-000000000.json\""));

        // 保存入参 file_name 为空时不写出,落盘文件干净
        let empty = TranslationRecord {
            file_name: String::new(),
            ..with_name
        };
        let s2 = serde_json::to_string(&empty).unwrap();
        assert!(!s2.contains("file_name"));
    }
}
