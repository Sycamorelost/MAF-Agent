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

/// 读取指定目录下的术语表(文件缺失/损坏返回空表)。
fn load_glossary_in(dir: &std::path::Path) -> Vec<GlossaryEntry> {
    let p = dir.join("glossary.json");
    if !p.exists() {
        return Vec::new();
    }
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 读取术语表(文件缺失/损坏返回空表)。
pub(crate) fn load_glossary() -> Vec<GlossaryEntry> {
    load_glossary_in(&crate::paths::data_dir())
}

/// 注入 prompt 的词条上限,避免大术语表撑爆上下文。
const GLOSSARY_INJECT_MAX: usize = 50;

/// 找出原文命中的词条并格式化为 prompt 片段;无命中返回 None。
/// 匹配规则:CJK 直接子串;拉丁词忽略大小写子串(轻量实现,够用)。
///
/// 导入预置后术语表可能上千条,命中数常超过注入上限;此时**优先保留更长的词**
/// (越长越具体,短词往往是长词的组成部分),避免关键术语被前面的大路词挤掉。
pub(crate) fn glossary_hints(text: &str) -> Option<String> {
    let entries = load_glossary();
    if entries.is_empty() {
        return None;
    }
    let lower = text.to_lowercase();
    let mut hits: Vec<(usize, String)> = Vec::new();
    for e in &entries {
        let src = e.src.trim();
        if src.is_empty() {
            continue;
        }
        let hit = text.contains(src) || lower.contains(&src.to_lowercase());
        if hit {
            hits.push((src.chars().count(), format!("- {} → {}", src, e.dst.trim())));
        }
    }
    if hits.is_empty() {
        return None;
    }
    hits.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    hits.truncate(GLOSSARY_INJECT_MAX);
    Some(format!(
        "\n\nTerminology glossary (use these translations consistently):\n{}",
        hits.into_iter().map(|(_, line)| line).collect::<Vec<_>>().join("\n")
    ))
}

#[tauri::command]
pub async fn glossary_list() -> Vec<GlossaryEntry> {
    load_glossary()
}

/// 落盘术语表到指定目录(glossary_save / glossary_import_presets 共用)。
fn write_glossary_in(dir: &std::path::Path, entries: &[GlossaryEntry]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("mkdir data failed: {e}"))?;
    let text = serde_json::to_string_pretty(entries).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("glossary.json"), text)
        .map_err(|e| format!("write glossary.json failed: {e}"))
}

/// 落盘术语表到当前数据目录。
fn write_glossary(entries: &[GlossaryEntry]) -> Result<(), String> {
    write_glossary_in(&crate::paths::data_dir(), entries)
}

/// 全量覆盖保存术语表(设置页增删改后整体提交)。
#[tauri::command]
pub async fn glossary_save(entries: Vec<GlossaryEntry>) -> Result<(), String> {
    let cleaned: Vec<GlossaryEntry> = entries
        .into_iter()
        .filter(|e| !e.src.trim().is_empty() && !e.dst.trim().is_empty())
        .collect();
    write_glossary(&cleaned)
}

// ---------- 预置术语导入(内置权威词表 → glossary,合并不覆盖) ----------

/// 预置术语(双向展开、跨表去重),数据源见 [`crate::glossary_presets`]。
fn preset_term_pairs() -> Vec<crate::glossary_presets::PresetTermPair> {
    crate::glossary_presets::pairs()
}

/// 合并预置词条:现有词条(含本次已并进的)按 src(trim 小写)判重,
/// 冲突一律保留用户译法,不覆盖;返回 (合并后全量, 新增数, 保留数)。
pub(crate) fn merge_preset_terms(
    existing: &[GlossaryEntry],
    presets: &[crate::glossary_presets::PresetTermPair],
) -> (Vec<GlossaryEntry>, usize, usize) {
    let mut merged: Vec<GlossaryEntry> = existing.to_vec();
    let mut keys: std::collections::HashSet<String> = existing
        .iter()
        .map(|e| e.src.trim().to_lowercase())
        .collect();
    let mut added = 0usize;
    let mut kept = 0usize;
    for p in presets {
        let key = p.src.trim().to_lowercase();
        if key.is_empty() {
            continue;
        }
        if keys.contains(&key) {
            kept += 1;
            continue;
        }
        keys.insert(key);
        let note = match p.hint.as_deref() {
            Some(h) => format!("预置·{}（{}）", p.cat, h),
            None => format!("预置·{}", p.cat),
        };
        merged.push(GlossaryEntry {
            src: p.src.clone(),
            dst: p.dst.clone(),
            note: Some(note),
        });
        added += 1;
    }
    (merged, added, kept)
}

/// 预置导入报告(前端提示汇总)。
#[derive(Debug, Clone, Serialize)]
pub struct GlossaryImportReport {
    /// 本次新增词条数
    pub added: usize,
    /// 与现有词条重复、保留用户译法的条数
    pub kept: usize,
    /// 合并后术语表总条数
    pub total: usize,
    /// 预置来源清单(分类 + 权威来源),供前端展示溯源
    pub sources: Vec<String>,
}

/// 一键(重新)导入预置术语:合并不覆盖,可反复点击。
/// 用户删掉的预置词条会被补回,用户改过译法的同名词条保持不动。
#[tauri::command]
pub async fn glossary_import_presets() -> Result<GlossaryImportReport, String> {
    import_presets_into(&crate::paths::data_dir())
}

/// 导入实现(可注入目录,便于测试真实落盘行为)。
pub(crate) fn import_presets_into(dir: &std::path::Path) -> Result<GlossaryImportReport, String> {
    let pairs = preset_term_pairs();
    if pairs.is_empty() {
        return Err("未找到预置术语内容".into());
    }
    let existing = load_glossary_in(dir);
    let (merged, added, kept) = merge_preset_terms(&existing, &pairs);
    write_glossary_in(dir, &merged)?;
    Ok(GlossaryImportReport {
        added,
        kept,
        total: merged.len(),
        sources: crate::glossary_presets::categories()
            .into_iter()
            .map(|(c, s)| format!("{c}:{s}"))
            .collect(),
    })
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

    // ---------- 预置术语导入 ----------

    fn pair(src: &str, dst: &str) -> crate::glossary_presets::PresetTermPair {
        crate::glossary_presets::PresetTermPair {
            src: src.into(),
            dst: dst.into(),
            cat: "测试".into(),
            hint: None,
        }
    }

    #[test]
    fn preset_pairs_available_and_bidirectional() {
        let pairs = preset_term_pairs();
        assert!(pairs.len() > 300, "pairs={}", pairs.len());
        assert!(pairs.iter().any(|p| p.src == "idempotent" && p.dst == "幂等"));
        assert!(pairs.iter().any(|p| p.src == "幂等" && p.dst == "idempotent"));
        // 覆盖 8 个专业领域
        let cats: std::collections::HashSet<&str> =
            pairs.iter().map(|p| p.cat.as_str()).collect();
        assert_eq!(cats.len(), 8, "cats={cats:?}");
    }

    #[test]
    fn merge_presets_empty_existing_all_added() {
        let presets = vec![pair("idempotent", "幂等"), pair("幂等", "idempotent")];
        let (merged, added, kept) = merge_preset_terms(&[], &presets);
        assert_eq!((added, kept, merged.len()), (2, 0, 2));
        assert_eq!(merged[0].note.as_deref(), Some("预置·测试"));
    }

    #[test]
    fn merge_presets_conflict_keeps_user_wording() {
        let existing = vec![GlossaryEntry {
            src: "Idempotent".into(), // 大小写不同也算同键
            dst: "用户译法".into(),
            note: None,
        }];
        let (merged, added, kept) = merge_preset_terms(&existing, &[pair("idempotent", "幂等")]);
        assert_eq!((added, kept, merged.len()), (0, 1, 1));
        assert_eq!(merged[0].dst, "用户译法"); // 用户译法零改动
    }

    #[test]
    fn merge_presets_idempotent_second_run_zero_added() {
        let presets = vec![pair("a", "甲"), pair("b", "乙")];
        let (m1, a1, _) = merge_preset_terms(&[], &presets);
        assert_eq!(a1, 2);
        let (m2, a2, k2) = merge_preset_terms(&m1, &presets);
        assert_eq!((a2, k2, m2.len()), (0, 2, 2));
    }

    #[test]
    fn merge_presets_restores_deleted_entry() {
        // 用户删掉了预置词条 → 再点「重新导入」应补回,其余用户词条不受影响
        let user = vec![GlossaryEntry {
            src: "自定义".into(),
            dst: "custom".into(),
            note: None,
        }];
        let (merged, added, kept) = merge_preset_terms(&user, &[pair("idempotent", "幂等")]);
        assert_eq!((added, kept, merged.len()), (1, 0, 2));
        assert!(merged.iter().any(|e| e.src == "自定义" && e.dst == "custom"));
        assert!(merged.iter().any(|e| e.src == "idempotent"));
    }

    /// 端到端:真实预置词表 → 空术语表全量导入 → 再次导入 0 新增(一键重新导入幂等)。
    #[test]
    fn preset_full_import_then_reimport_is_noop() {
        let pairs = preset_term_pairs();
        let (m1, a1, k1) = merge_preset_terms(&[], &pairs);
        assert_eq!(a1, pairs.len());
        assert_eq!(k1, 0);
        assert_eq!(m1.len(), pairs.len());
        // 全部带「预置·」备注,便于用户在 UI 区分
        assert!(m1.iter().all(|e| e.note.as_deref().unwrap_or("").starts_with("预置·")));
        // 第二次点击:全部命中已有,零新增零改动
        let (m2, a2, k2) = merge_preset_terms(&m1, &pairs);
        assert_eq!((a2, k2, m2.len()), (0, pairs.len(), pairs.len()));
    }

    /// 文件级端到端:真实读写 `glossary.json`,验证落盘、二次导入幂等、用户译法保留、删除后补回。
    #[test]
    fn import_presets_into_temp_dir_writes_and_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("maf-glossary-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // 首次导入:空表 → 全量新增并真实落盘
        let r1 = import_presets_into(&dir).unwrap();
        let pairs = preset_term_pairs();
        assert_eq!(r1.added, pairs.len());
        assert_eq!(r1.kept, 0);
        assert_eq!(r1.total, pairs.len());
        assert_eq!(r1.sources.len(), 8, "应带回 8 个来源说明");
        assert!(dir.join("glossary.json").exists(), "glossary.json 未落盘");
        assert_eq!(load_glossary_in(&dir).len(), pairs.len());

        // 二次导入:全部已存在 → 零新增,总数不变(一键重新导入幂等)
        let r2 = import_presets_into(&dir).unwrap();
        assert_eq!((r2.added, r2.kept, r2.total), (0, pairs.len(), pairs.len()));

        // 用户改译法 → 重新导入不得覆盖
        let mut list = load_glossary_in(&dir);
        let idx = list.iter().position(|e| e.src == "idempotent").unwrap();
        list[idx].dst = "用户译法".into();
        write_glossary_in(&dir, &list).unwrap();
        let r3 = import_presets_into(&dir).unwrap();
        assert_eq!(r3.added, 0);
        assert_eq!(load_glossary_in(&dir)[idx].dst, "用户译法");

        // 用户删掉预置词条 → 重新导入补回(恰好补 1 条,总数回到原值)
        let mut list2 = load_glossary_in(&dir);
        let n = list2.len();
        let deleted = list2.remove(0);
        write_glossary_in(&dir, &list2).unwrap();
        assert_eq!(load_glossary_in(&dir).len(), n - 1);
        let r4 = import_presets_into(&dir).unwrap();
        assert_eq!((r4.added, r4.total), (1, n));
        assert!(load_glossary_in(&dir).iter().any(|e| e.src == deleted.src));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 注入上限:命中过多时优先保留长词(更具体),不被短词挤占。
    #[test]
    fn hint_priority_prefers_longer_terms() {
        let entries: Vec<GlossaryEntry> = (1..=60)
            .map(|i| GlossaryEntry {
                src: format!("term{}", "x".repeat(i)),
                dst: format!("词{i}"),
                note: None,
            })
            .collect();
        let text = entries.iter().map(|e| e.src.as_str()).collect::<Vec<_>>().join(" ");
        let mut hits: Vec<(usize, String)> = entries
            .iter()
            .filter(|e| text.contains(e.src.as_str()))
            .map(|e| (e.src.chars().count(), format!("- {} → {}", e.src, e.dst)))
            .collect();
        hits.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
        hits.truncate(GLOSSARY_INJECT_MAX);
        assert_eq!(hits.len(), GLOSSARY_INJECT_MAX);
        // 最长的词必在其中,最短的被截掉
        assert!(hits[0].1.contains(&format!("term{}", "x".repeat(60))));
        assert!(!hits.iter().any(|(_, h)| h.contains("- termx →")));
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
