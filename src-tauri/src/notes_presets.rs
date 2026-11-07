//! 预置知识库笔记(翻译术语表 + 表格/数据处理速查)。
//!
//! 模式与 agents_prompts 一致:内容 = `notes_presets/<id>.md`,编译期 `include_str!` 载入。
//! 首次启动由 `seed_if_needed` 落盘为普通笔记文件(`data/notes/{id}.md`),
//! 之后就是用户自己的笔记:可自由编辑/删除;`preset_seeded.json` 记录已种过的 id,
//! 用户删除后不会复活,用户已存在的同名文件也不会被覆盖。

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::commands::notes as note_store;

mod merge;

/// 预置笔记条目(编译期静态)。
pub struct PresetNote {
    pub id: &'static str,
    pub title: &'static str,
    pub tags: &'static [&'static str],
    pub body: &'static str,
}

/// 种子标记版本:仅当 marker 结构变化时递增;新增预置条目无需递增(按 id 判重)。
const SEED_VERSION: u32 = 1;

/// 种子标记:记录已落盘的预置 id,防止用户删除后被复活。
#[derive(Debug, Default, Serialize, Deserialize)]
struct SeedMarker {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    seeded: Vec<String>,
}

/// 全部预置条目(11 条:4 类翻译术语表 + 4 篇表格/数据处理速查 + 3 篇正则/排版/图表速查)。
pub fn presets() -> Vec<PresetNote> {
    vec![
        PresetNote {
            id: "preset-terms-it",
            title: "IT/编程/技术 常用翻译术语表",
            tags: &["预置", "翻译", "术语表", "技术"],
            body: include_str!("notes_presets/preset-terms-it.md"),
        },
        PresetNote {
            id: "preset-terms-academic",
            title: "学术/科研 常用翻译术语表",
            tags: &["预置", "翻译", "术语表", "学术"],
            body: include_str!("notes_presets/preset-terms-academic.md"),
        },
        PresetNote {
            id: "preset-terms-legal",
            title: "法律/商务/金融 常用翻译术语表",
            tags: &["预置", "翻译", "术语表", "法律", "金融"],
            body: include_str!("notes_presets/preset-terms-legal.md"),
        },
        PresetNote {
            id: "preset-terms-office",
            title: "通用办公/营销/数据 常用翻译术语表",
            tags: &["预置", "翻译", "术语表", "办公", "营销", "数据"],
            body: include_str!("notes_presets/preset-terms-office.md"),
        },
        PresetNote {
            id: "preset-table-markdown",
            title: "Markdown 表格语法速查(GFM)",
            tags: &["预置", "表格", "Markdown", "速查"],
            body: include_str!("notes_presets/preset-table-markdown.md"),
        },
        PresetNote {
            id: "preset-table-csv",
            title: "CSV/TSV 约定与常见坑",
            tags: &["预置", "表格", "CSV", "速查"],
            body: include_str!("notes_presets/preset-table-csv.md"),
        },
        PresetNote {
            id: "preset-data-cleaning",
            title: "数据清洗常见规则",
            tags: &["预置", "数据", "清洗", "速查"],
            body: include_str!("notes_presets/preset-data-cleaning.md"),
        },
        PresetNote {
            id: "preset-excel-formulas",
            title: "常用公式与数据透视思路",
            tags: &["预置", "表格", "Excel", "公式", "速查"],
            body: include_str!("notes_presets/preset-excel-formulas.md"),
        },
        PresetNote {
            id: "preset-regex-cheatsheet",
            title: "正则表达式速查",
            tags: &["预置", "数据", "正则", "速查"],
            body: include_str!("notes_presets/preset-regex-cheatsheet.md"),
        },
        PresetNote {
            id: "preset-cn-typography",
            title: "中文排版与标点规范",
            tags: &["预置", "写作", "排版", "翻译"],
            body: include_str!("notes_presets/preset-cn-typography.md"),
        },
        PresetNote {
            id: "preset-chart-selection",
            title: "数据可视化图表选型速查",
            tags: &["预置", "数据", "图表", "速查"],
            body: include_str!("notes_presets/preset-chart-selection.md"),
        },
    ]
}

fn marker_path(dir: &Path) -> PathBuf {
    dir.join("preset_seeded.json")
}

fn load_marker(dir: &Path) -> SeedMarker {
    let Ok(text) = fs::read_to_string(marker_path(dir)) else {
        return SeedMarker::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_marker(dir: &Path, marker: &SeedMarker) -> Result<(), String> {
    let text = serde_json::to_string_pretty(marker).map_err(|e| e.to_string())?;
    let path = marker_path(dir);
    fs::write(&path, text).map_err(|e| format!("write {} failed: {e}", path.display()))
}

/// 本次新写入的笔记(返回给上层补 FTS 索引)。
#[derive(Debug)]
pub struct SeededNote {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub path: PathBuf,
    pub created: String,
    pub updated: String,
    pub body: String,
}

/// 纯文件层种子:把未种过的预置笔记写入 dir,返回新写入列表。
/// - marker 已记录的 id 一律跳过(用户删除不复活);
/// - 文件已存在但 marker 未记录的(用户同名巧合),只补记录、不覆盖用户内容。
pub fn seed_into(dir: &Path) -> Result<Vec<SeededNote>, String> {
    fs::create_dir_all(dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    let mut marker = load_marker(dir);
    let mut out = Vec::new();
    let now = chrono::Local::now().to_rfc3339();

    for p in presets() {
        if marker.seeded.iter().any(|s| s == p.id) {
            continue;
        }
        let path = dir.join(format!("{}.md", p.id));
        if !path.exists() {
            let tags: Vec<String> = p.tags.iter().map(|s| s.to_string()).collect();
            let fm = note_store::NoteFrontmatter {
                id: p.id.to_string(),
                title: p.title.to_string(),
                tags: tags.clone(),
                created: now.clone(),
                updated: now.clone(),
            };
            let text = note_store::render_note(&fm, p.body);
            fs::write(&path, text).map_err(|e| format!("write {} failed: {e}", path.display()))?;
            out.push(SeededNote {
                id: p.id.to_string(),
                title: p.title.to_string(),
                tags,
                path,
                created: now.clone(),
                updated: now.clone(),
                body: p.body.to_string(),
            });
        }
        marker.seeded.push(p.id.to_string());
    }

    marker.version = SEED_VERSION;
    save_marker(dir, &marker)?;
    Ok(out)
}

/// 应用启动种子:先落盘,再把新写入的笔记补进 DB/FTS(全局搜索立即可达)。
/// 失败只由调用方记日志,不阻断启动。
pub fn seed_if_needed() -> Result<(), String> {
    let dir = note_store::notes_dir();
    let seeded = seed_into(&dir)?;
    for s in &seeded {
        note_store::index_note(
            &s.id,
            &s.title,
            &note_store::tags_to_db(&s.tags),
            &s.path.to_string_lossy(),
            &s.created,
            &s.updated,
            &s.body,
        )?;
    }
    if !seeded.is_empty() {
        log::info!("预置知识库已落盘 {} 条(术语表/表格速查)", seeded.len());
    }
    Ok(())
}

// ===== 一键重新导入(恢复缺失 + 合并不覆盖,弹窗报告) =====

/// 单条处理结果(弹窗逐条展示)。
#[derive(Debug, Clone, Serialize)]
pub struct ReimportItem {
    pub id: String,
    pub title: String,
    /// 处理说明:合并细节 / 恢复原因 / 跳过原因
    pub detail: String,
}

/// 重新导入报告:三组分类,用户可滚动翻阅核对。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ReimportReport {
    /// 缺失已恢复
    pub added: Vec<ReimportItem>,
    /// 部分重复:预置缺漏内容已并入,用户内容零改动
    pub merged: Vec<ReimportItem>,
    /// 完全重复:未做任何改动
    pub skipped: Vec<ReimportItem>,
}

/// 一键重新导入:恢复被删的预置笔记;已存在的按章节/表格合并(用户数据零丢失);
/// 完全重复的跳过。返回 (报告, 本次写入磁盘的笔记列表,供上层补 FTS)。
/// 与 seed_into 的区别:本函数面向「用户主动点击」,被删的预置会被恢复。
pub fn reimport_into(dir: &Path) -> Result<(ReimportReport, Vec<SeededNote>), String> {
    fs::create_dir_all(dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    let mut marker = load_marker(dir);
    let mut report = ReimportReport::default();
    let mut changed: Vec<SeededNote> = Vec::new();
    let now = chrono::Local::now().to_rfc3339();

    for p in presets() {
        let path = dir.join(format!("{}.md", p.id));
        if !path.exists() {
            // 缺失 → 按预置原样恢复
            let tags: Vec<String> = p.tags.iter().map(|s| s.to_string()).collect();
            let fm = note_store::NoteFrontmatter {
                id: p.id.to_string(),
                title: p.title.to_string(),
                tags: tags.clone(),
                created: now.clone(),
                updated: now.clone(),
            };
            let text = note_store::render_note(&fm, p.body);
            fs::write(&path, text).map_err(|e| format!("write {} failed: {e}", path.display()))?;
            report.added.push(ReimportItem {
                id: p.id.to_string(),
                title: p.title.to_string(),
                detail: "笔记缺失,已按预置内容恢复".into(),
            });
            changed.push(SeededNote {
                id: p.id.to_string(),
                title: p.title.to_string(),
                tags,
                path,
                created: now.clone(),
                updated: now.clone(),
                body: p.body.to_string(),
            });
        } else {
            let raw = fs::read_to_string(&path)
                .map_err(|e| format!("read {} failed: {e}", path.display()))?;
            let (fm, body) = note_store::parse_note(&raw);
            let (merged_body, details) = merge::merge_body(&body, p.body);
            if details.is_empty() {
                report.skipped.push(ReimportItem {
                    id: p.id.to_string(),
                    title: if fm.title.trim().is_empty() {
                        p.title.to_string()
                    } else {
                        fm.title.clone()
                    },
                    detail: "内容完全重复,未改动".into(),
                });
            } else {
                // 合并写回:front-matter 以用户为准,空字段回落预置;created 保留,updated 刷新
                let title = if fm.title.trim().is_empty() {
                    p.title.to_string()
                } else {
                    fm.title.clone()
                };
                let tags: Vec<String> = if fm.tags.is_empty() {
                    p.tags.iter().map(|s| s.to_string()).collect()
                } else {
                    fm.tags.clone()
                };
                let created = if fm.created.trim().is_empty() {
                    now.clone()
                } else {
                    fm.created.clone()
                };
                let nfm = note_store::NoteFrontmatter {
                    id: p.id.to_string(),
                    title: title.clone(),
                    tags: tags.clone(),
                    created: created.clone(),
                    updated: now.clone(),
                };
                let text = note_store::render_note(&nfm, &merged_body);
                fs::write(&path, text)
                    .map_err(|e| format!("write {} failed: {e}", path.display()))?;
                report.merged.push(ReimportItem {
                    id: p.id.to_string(),
                    title: title.clone(),
                    detail: details.join(";"),
                });
                changed.push(SeededNote {
                    id: p.id.to_string(),
                    title,
                    tags,
                    path,
                    created,
                    updated: now.clone(),
                    body: merged_body,
                });
            }
        }
        if !marker.seeded.iter().any(|s| s == p.id) {
            marker.seeded.push(p.id.to_string());
        }
    }

    marker.version = SEED_VERSION;
    save_marker(dir, &marker)?;
    Ok((report, changed))
}

/// 命令层入口:重新导入 + 新写入的笔记补进 DB/FTS(全局搜索立即可达)。
pub fn reimport_and_index() -> Result<ReimportReport, String> {
    let dir = note_store::notes_dir();
    let (report, changed) = reimport_into(&dir)?;
    for s in &changed {
        note_store::index_note(
            &s.id,
            &s.title,
            &note_store::tags_to_db(&s.tags),
            &s.path.to_string_lossy(),
            &s.created,
            &s.updated,
            &s.body,
        )?;
    }
    log::info!(
        "预置知识库重新导入:恢复 {} · 合并 {} · 重复 {}",
        report.added.len(),
        report.merged.len(),
        report.skipped.len()
    );
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 隔离临时目录(进程 id + 时间戳保证唯一;结束尽力清理)。
    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "myagent-notes-presets-{tag}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn presets_registry_well_formed() {
        let list = presets();
        assert_eq!(list.len(), 11);
        let mut ids: Vec<&str> = list.iter().map(|p| p.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 11, "预置 id 必须唯一");
        for p in &list {
            crate::memory::validate_id(p.id).unwrap_or_else(|e| panic!("{} id 非法: {e}", p.id));
            assert!(p.id.starts_with("preset-"), "{} 须以 preset- 开头", p.id);
            assert!(!p.title.trim().is_empty(), "{} 缺标题", p.id);
            assert!(p.tags.contains(&"预置"), "{} 缺「预置」标签", p.id);
            assert!(p.body.len() > 200, "{} 正文过短", p.id);
            // 术语表必须含表格;速查篇至少含一个标题行
            assert!(
                p.body.contains('|') || p.body.contains("# "),
                "{} 内容形态异常",
                p.id
            );
        }
    }

    #[test]
    fn seed_writes_files_and_is_idempotent() {
        let dir = temp_dir("seed");
        let first = seed_into(&dir).expect("首次种子");
        assert_eq!(first.len(), 11);
        // 落盘文件可被 notes 解析层完整读回(front-matter 往返)
        for s in &first {
            let raw = fs::read_to_string(&s.path).unwrap();
            let (fm, body) = note_store::parse_note(&raw);
            assert_eq!(fm.id, s.id);
            assert_eq!(fm.title, s.title);
            assert_eq!(fm.tags, s.tags);
            assert_eq!(body.trim(), s.body.trim());
        }
        // 幂等:第二次种子 0 条
        let second = seed_into(&dir).expect("二次种子");
        assert!(second.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn deleted_preset_never_resurrects() {
        let dir = temp_dir("nores");
        seed_into(&dir).expect("首次种子");
        let victim = dir.join("preset-terms-it.md");
        fs::remove_file(&victim).unwrap();
        let again = seed_into(&dir).expect("删除后再种子");
        assert!(again.is_empty(), "用户删除的预置笔记不得复活");
        assert!(!victim.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn user_file_same_name_not_overwritten() {
        let dir = temp_dir("keep");
        fs::create_dir_all(&dir).unwrap();
        let mine = dir.join("preset-terms-it.md");
        fs::write(&mine, "用户自己的同名笔记").unwrap();
        let seeded = seed_into(&dir).expect("种子");
        // 同名文件被保留、不覆盖;其余 10 条正常落盘
        assert_eq!(seeded.len(), 10);
        assert_eq!(fs::read_to_string(&mine).unwrap(), "用户自己的同名笔记");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_marker_recovers_to_full_seed() {
        let dir = temp_dir("corrupt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(marker_path(&dir), "{不是合法 json").unwrap();
        let seeded = seed_into(&dir).expect("损坏 marker 应按未种子处理");
        assert_eq!(seeded.len(), 11);
        // marker 被重写为合法 JSON
        let m = load_marker(&dir);
        assert_eq!(m.version, SEED_VERSION);
        assert_eq!(m.seeded.len(), 11);
        let _ = fs::remove_dir_all(&dir);
    }

    // ===== 一键重新导入(恢复 + 合并 + 极端情况) =====

    #[test]
    fn reimport_empty_dir_adds_all_then_all_skipped() {
        let dir = temp_dir("ri-empty");
        let (r1, c1) = reimport_into(&dir).expect("首次导入");
        assert_eq!(r1.added.len(), 11);
        assert!(r1.merged.is_empty() && r1.skipped.is_empty());
        assert_eq!(c1.len(), 11, "11 条全部需补索引");
        // 第二次:完全重复全部跳过,零写入
        let (r2, c2) = reimport_into(&dir).expect("二次导入");
        assert_eq!(r2.skipped.len(), 11);
        assert!(r2.added.is_empty() && r2.merged.is_empty() && c2.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reimport_restores_deleted_and_updates_marker() {
        let dir = temp_dir("ri-restore");
        seed_into(&dir).unwrap();
        fs::remove_file(dir.join("preset-terms-it.md")).unwrap();
        fs::remove_file(dir.join("preset-table-csv.md")).unwrap();
        // seed 不复活,但 reimport 主动恢复
        let (report, changed) = reimport_into(&dir).expect("导入");
        assert_eq!(report.added.len(), 2);
        assert_eq!(report.skipped.len(), 9);
        assert_eq!(changed.len(), 2);
        assert!(dir.join("preset-terms-it.md").exists());
        // marker 同步:之后再 seed 也不会重复
        let m = load_marker(&dir);
        assert_eq!(m.seeded.len(), 11);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reimport_merges_user_edits_zero_loss() {
        let dir = temp_dir("ri-merge");
        seed_into(&dir).unwrap();
        // 用户改写 IT 术语表:换标题、删章节、删行、加私货
        let user_body = "# 我的技术术语\n\n## 一、基础概念\n\n| 英文 | 中文 | 备注 |\n| --- | --- | --- |\n| function | 函式 | |\n| my-own | 私货 | |\n";
        let fm = note_store::NoteFrontmatter {
            id: "preset-terms-it".into(),
            title: "我的技术术语(改名)".into(),
            tags: vec!["预置".into(), "私有".into()],
            created: "2026-01-01T00:00:00+08:00".into(),
            updated: "2026-01-01T00:00:00+08:00".into(),
        };
        fs::write(
            dir.join("preset-terms-it.md"),
            note_store::render_note(&fm, user_body),
        )
        .unwrap();

        let (report, _) = reimport_into(&dir).expect("导入");
        assert_eq!(report.merged.len(), 1, "仅 1 条被合并");
        assert_eq!(report.skipped.len(), 10);
        let item = &report.merged[0];
        assert_eq!(item.title, "我的技术术语(改名)", "用户标题不被覆盖");
        assert!(
            item.detail.contains("恢复章节"),
            "缺章节应恢复: {}",
            item.detail
        );
        assert!(
            item.detail.contains("表格「一、基础概念」补充"),
            "被删的预置行应按首列键补回: {}",
            item.detail
        );
        assert!(
            !item.detail.contains("补充 21"),
            "function 行已存在不得重复补: {}",
            item.detail
        );

        // 落盘内容核对:用户行保留 + 预置缺漏补回 + front-matter 用户字段保留
        let raw = fs::read_to_string(dir.join("preset-terms-it.md")).unwrap();
        let (fm2, body2) = note_store::parse_note(&raw);
        assert_eq!(fm2.title, "我的技术术语(改名)");
        assert_eq!(fm2.created, "2026-01-01T00:00:00+08:00", "created 保留");
        assert!(fm2.tags.contains(&"私有".to_string()), "用户标签保留");
        assert!(body2.contains("| function | 函式 | |"), "用户译法保留");
        assert!(body2.contains("| my-own | 私货 | |"), "用户自增行保留");
        assert!(body2.contains("| variable | 变量 | |"), "被删行补回");
        assert!(body2.contains("## 四、安全与身份"), "被删章节恢复");

        // 幂等:再导入全部跳过
        let (r2, _) = reimport_into(&dir).expect("二次导入");
        assert_eq!(r2.skipped.len(), 11);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reimport_foreign_note_gets_sections_and_preset_fm() {
        let dir = temp_dir("ri-foreign");
        seed_into(&dir).unwrap();
        // 同名文件被用户彻底换成无 front-matter 的私人文本
        fs::write(dir.join("preset-table-csv.md"), "买菜清单\n\n- 白菜\n").unwrap();
        let (report, _) = reimport_into(&dir).expect("导入");
        assert_eq!(report.merged.len(), 1);
        let raw = fs::read_to_string(dir.join("preset-table-csv.md")).unwrap();
        let (fm, body) = note_store::parse_note(&raw);
        assert_eq!(fm.title, "CSV/TSV 约定与常见坑", "空标题回落预置");
        assert!(fm.tags.contains(&"预置".to_string()));
        assert!(body.starts_with("买菜清单"), "用户原文保留在开头");
        assert!(body.contains("RFC 4180 核心规则"), "预置章节并入");
        let _ = fs::remove_dir_all(&dir);
    }
}
