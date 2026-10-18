//! 研究方案 Blueprint(专项研究步骤 4):参数 + 阶段编排 + 提示词 + 约束一体保存复用。
//!
//! - 落盘:`data/research/blueprints.json`(仅用户方案;内置方案代码内生成,list 时合并返回);
//! - 旧 `presets.json` 首次加载自动迁移为 legacy 模式用户方案,原文件改名 `.migrated` 备份;
//! - 内置方案同名保护:不可覆盖 / 删除(对齐指令库 builtin 交互模式),可「另存为」副本。

use std::fs;
use std::path::PathBuf;

use chrono::Local;
use serde::{Deserialize, Serialize};

use super::stage::StageConfig;
use super::{preset, ResearchParams};

/// 一条研究方案。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blueprint {
    pub name: String,
    pub desc: String,
    pub builtin: bool,
    pub params: ResearchParams,
    pub created: String,
}

// ---------------- 内置方案 ----------------

/// 内置「开发策略研究」终稿模板:策略四章节结构,{references} 锚点控制引用位置。
const STRATEGY_REPORT_TEMPLATE: &str = r#"你是研究报告撰写器。基于给定的阶段摘要与资料列表,输出 Markdown 专项研究与开发策略报告(语言:{output_lang})。

结构要求(严格按此章节顺序):
# 《{topic}》专项研究与开发策略报告
## TL;DR
(3-5 句核心结论)
## 一、背景与现状
## 二、方案对比分析
## 三、关键技术结论
## 四、开发策略
### 4.1 推荐方案与理由
### 4.2 实施路线
### 4.3 风险与缓解
### 4.4 验收标准
## 五、局限与不确定性
{references}

规则:
1. 正文中的论断用 [n] 引用编号标注来源,编号对应资料列表中的 [n]。
2. 不得虚构资料列表之外的事实与引用编号。
3. 只输出 Markdown 正文,不要用代码块包裹,不要输出解释性前言。

【阶段摘要】
{stages_digest}"#;

fn research_stage(id: &str, name: &str, goal: &str, max_rounds: Option<u32>) -> StageConfig {
    StageConfig {
        id: id.into(),
        name: name.into(),
        goal: goal.into(),
        kind: "research".into(),
        enabled: true,
        max_rounds,
        ..StageConfig::default()
    }
}

/// 内置:开发策略研究(四阶段 + 检查点 + 自定义终稿模板)。
fn builtin_strategy() -> Blueprint {
    let s1 = research_stage(
        "bp-st-1",
        "背景与现状摸底",
        "收集主题相关的基础定义、发展脉络与现状数据,建立背景认知",
        Some(4),
    );
    let s2 = research_stage(
        "bp-st-2",
        "方案与竞品对比",
        "收集主流方案与竞品资料,提取各自优劣、适用场景与可对比维度",
        Some(6),
    );
    let s3 = research_stage(
        "bp-st-3",
        "关键技术深挖",
        "针对前两阶段发现的关键技术点深入检索,获取实现细节与数据支撑",
        None,
    );
    let s4 = StageConfig {
        id: "bp-st-4".into(),
        name: "开发策略报告".into(),
        goal: "基于全部资料与阶段摘要,产出含推荐方案、实施路线、风险与验收标准的开发策略".into(),
        kind: "synthesis".into(),
        enabled: true,
        ..StageConfig::default()
    };
    Blueprint {
        name: "开发策略研究".into(),
        desc: "四阶段:背景摸底 → 方案对比 → 技术深挖 → 开发策略报告。产出含推荐方案、实施路线、风险与验收标准。".into(),
        builtin: true,
        params: ResearchParams {
            mode: "staged".into(),
            stages: vec![s1, s2, s3, s4],
            checkpoint: true,
            report_prompt: Some(STRATEGY_REPORT_TEMPLATE.into()),
            ..ResearchParams::default()
        },
        created: "builtin".into(),
    }
}

/// 内置:学术论文深挖(两阶段,学术优先源 + 全文阅读)。
fn builtin_academic() -> Blueprint {
    let mut s1 = research_stage(
        "bp-ac-1",
        "文献广撒",
        "围绕主题广泛收集学术论文与研究报告,提取研究问题、方法、数据集与核心结论",
        Some(6),
    );
    s1.source_mode = Some("academic".into());
    s1.read_length = Some("full".into());
    let s2 = StageConfig {
        id: "bp-ac-2".into(),
        name: "方法论精读".into(),
        goal: "基于已收集文献,提炼研究方法、实验设计与结论可靠性的对比分析".into(),
        kind: "synthesis".into(),
        enabled: true,
        ..StageConfig::default()
    };
    Blueprint {
        name: "学术论文深挖".into(),
        desc: "两阶段:文献广撒(学术优先源)→ 方法论精读。全文阅读长度,适合论文与报告类主题。".into(),
        builtin: true,
        params: ResearchParams {
            mode: "staged".into(),
            stages: vec![s1, s2],
            source_mode: "academic".into(),
            read_length: "full".into(),
            ..ResearchParams::default()
        },
        created: "builtin".into(),
    }
}

fn builtins() -> Vec<Blueprint> {
    vec![builtin_strategy(), builtin_academic()]
}

fn is_builtin_name(name: &str) -> bool {
    builtins().iter().any(|b| b.name == name)
}

// ---------------- 落盘与迁移 ----------------

fn blueprints_path() -> PathBuf {
    crate::paths::data_dir()
        .join("research")
        .join("blueprints.json")
}

fn presets_path() -> PathBuf {
    crate::paths::data_dir()
        .join("research")
        .join("presets.json")
}

/// 直接读用户方案文件(不做迁移;文件缺失/损坏返回空表)。
fn read_user_file() -> Vec<Blueprint> {
    let p = blueprints_path();
    if !p.exists() {
        return Vec::new();
    }
    let Ok(text) = fs::read_to_string(&p) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn write_user_file(list: &[Blueprint]) -> Result<(), String> {
    let p = blueprints_path();
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {} failed: {e}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    fs::write(&p, text).map_err(|e| format!("write blueprints.json failed: {e}"))
}

/// 旧 presets.json 一次性迁移:转换为 legacy 模式用户方案后改名备份。
fn migrate_presets_once() {
    let src = presets_path();
    if !src.exists() {
        return;
    }
    let presets = preset::load_presets();
    if !presets.is_empty() {
        let mut list = read_user_file();
        for pr in presets {
            if list.iter().any(|b| b.name == pr.name) || is_builtin_name(&pr.name) {
                continue;
            }
            list.push(Blueprint {
                name: pr.name,
                desc: "旧版研究预设迁移(仅参数,快速模式)".into(),
                builtin: false,
                params: ResearchParams {
                    mode: "legacy".into(),
                    stages: Vec::new(),
                    ..pr.params
                },
                created: pr.created,
            });
        }
        let _ = write_user_file(&list);
    }
    // 迁移后改名备份(失败不阻断:下次启动重试)
    let _ = fs::rename(&src, src.with_extension("json.migrated"));
}

fn load_user() -> Vec<Blueprint> {
    migrate_presets_once();
    read_user_file()
}

/// 列出全部方案(内置在前;与内置同名的用户条目视为非法,过滤)。
pub fn list() -> Vec<Blueprint> {
    let b = builtins();
    let user: Vec<Blueprint> = load_user()
        .into_iter()
        .filter(|x| !b.iter().any(|bi| bi.name == x.name))
        .collect();
    b.into_iter().chain(user).collect()
}

/// 保存方案(同名覆盖;内置名保护)。desc 可空(落为空串)。
pub fn save(name: &str, desc: &str, params: ResearchParams) -> Result<Vec<Blueprint>, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("方案名称不能为空".into());
    }
    if is_builtin_name(&name) {
        return Err("内置方案不可覆盖,请另存为新名称".into());
    }
    let desc = desc.trim().to_string();
    let mut items = load_user();
    if let Some(existing) = items.iter_mut().find(|x| x.name == name) {
        existing.desc = desc;
        existing.params = params;
    } else {
        items.push(Blueprint {
            name,
            desc,
            builtin: false,
            params,
            created: Local::now().to_rfc3339(),
        });
    }
    write_user_file(&items)?;
    Ok(list())
}

/// 删除用户方案(内置名保护)。
pub fn delete(name: &str) -> Result<Vec<Blueprint>, String> {
    if is_builtin_name(name) {
        return Err("内置方案不可删除".into());
    }
    let mut items = load_user();
    items.retain(|x| x.name != name);
    write_user_file(&items)?;
    Ok(list())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_names_protected() {
        assert!(is_builtin_name("开发策略研究"));
        assert!(is_builtin_name("学术论文深挖"));
        assert!(!is_builtin_name("我的方案"));
        let r = save("开发策略研究", "", ResearchParams::default());
        assert!(r.is_err());
        assert!(delete("学术论文深挖").is_err());
    }

    #[test]
    fn builtin_strategy_params_shape() {
        let b = builtin_strategy();
        assert_eq!(b.params.mode, "staged");
        assert_eq!(b.params.stages.len(), 4);
        assert!(b.params.checkpoint);
        assert!(b.params.report_prompt.is_some());
        assert_eq!(b.params.stages[3].kind, "synthesis");
        // 阶段均为启用且 id 唯一
        let ids: std::collections::HashSet<_> = b.params.stages.iter().map(|s| &s.id).collect();
        assert_eq!(ids.len(), 4);
        assert!(b.params.stages.iter().all(|s| s.enabled));
    }

    #[test]
    fn builtin_academic_params_shape() {
        let b = builtin_academic();
        assert_eq!(b.params.source_mode, "academic");
        assert_eq!(b.params.read_length, "full");
        assert_eq!(b.params.stages.len(), 2);
        assert_eq!(b.params.stages[0].source_mode.as_deref(), Some("academic"));
    }
}
