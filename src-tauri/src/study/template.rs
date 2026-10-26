//! 学习规划提示词模板(移植 Deep-tutor 技能提示词包)。
//!
//! 来源声明 / Attribution:
//!   Deep-tutor — https://github.com/hershate/Deep-tutor
//!   Copyright (c) hershate and contributors
//!   Licensed under the Apache License, Version 2.0 (see LICENSE / NOTICE).
//!   This file is a derivative work: the prompt contracts were rewritten for
//!   MAF Agent's Rust engine and adapted to its JSON schema; see NOTICE.
//!
//! - 内置模板即引擎常量,代码内保护不可删;
//! - 用户自定义覆盖落盘 `data/study/templates.json`,同 key 覆盖式更新;
//! - 生效优先级:templates.json 自定义 > 内置;
//! - 所有模板只允许模型输出约定 Schema 的 JSON(校验在 mod.rs 确定性完成)。

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 合法模板 key(P0:意图澄清 / 知识地图 / 阶段设计 / 资料推荐;P1:自学材料 / 测验出题;
/// 排期为确定性计算,无模板)。
#[allow(dead_code)] // P1 模板编辑 UI 启用
pub const TEMPLATE_KEYS: [&str; 6] = [
    "study_intent",
    "study_map",
    "study_stages",
    "study_resources",
    "study_guide",
    "study_quiz",
];

/// 一条用户自定义覆盖(与研究模板同构)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateOverride {
    pub key: String,
    pub content: String,
    pub updated: String,
}

// ---------------- 内置模板 ----------------

pub const STUDY_INTENT: &str = r#"你是学习规划师。从用户输入中抽取学习意图,只输出一个 JSON 对象,不包代码围栏,不输出任何其他文本。

Schema:
{"topic":"学习主题","level":"zero|beginner|intermediate|advanced","goal":"学习目标(一句话)","daily_minutes":每天可投入分钟数(整数),"deadline":"目标完成日期 YYYY-MM-DD 或 null","lang":"zh|en","missing":["缺失的可选字段名"],"questions":["针对缺失字段的追问"]}

规则:
1. 只抽取用户明确表达的信息,禁止编造;无法确定时放入 missing 并给出 questions;
2. 可选字段为 level / daily_minutes / deadline;topic 与 goal 必有;
3. topic 若用户输入即主题则原样保留,不得改写;
4. lang 与用户输入语言一致;
5. questions 最多 2 条,每条一句话,直接问用户要缺失信息。"#;

pub const STUDY_MAP: &str = r#"你是课程设计专家。为主题生成知识地图,只输出一个 JSON 对象,不包代码围栏,不输出任何其他文本。

Schema:
{"nodes":[{"name":"知识点名","difficulty":1,"why":"为什么重要(≤30字)","must_learn":true,"prereqs":["前置知识点名"]}]}

规则:
1. 生成 8-20 个知识点,覆盖主题全貌,由浅入深;
2. name 必须唯一、简短(≤12 字);difficulty 为 1(入门)~5(专家)整数;
3. prereqs 只能引用本列表内已有的知识点名,可为空数组;
4. 约 60% 的知识点 must_learn=true(核心必学),其余为可选拓展;
5. 只输出 JSON。"#;

pub const STUDY_STAGES: &str = r#"你是课程设计专家。把给定的知识地图组织为学习阶段,只输出一个 JSON 对象,不包代码围栏,不输出任何其他文本。

Schema:
{"stages":[{"index":0,"name":"阶段名","goal":"阶段目标(一句话)","difficulty":1,"estimated_weeks":2,"topics":["知识点名"],"milestone":"阶段里程碑"}]}

硬规则:
1. 阶段 3-6 个,按学习顺序排列,index 从 0 递增;
2. 每阶段 topics 3-6 个,必须来自给定的知识点列表,不得编造;每个知识点只允许出现一次;
3. 相邻阶段 difficulty 差 ≤2,整体由易到难;
4. 所有 must_learn 知识点必须被覆盖;
5. estimated_weeks 为整数 ≥1;
6. milestone 只用当前及之前阶段的知识可达成,表述为可检验的成果;
7. 只输出 JSON。"#;

pub const STUDY_RESOURCES: &str = r#"你是学习资料顾问。为每个学习阶段推荐学习资源,只输出一个 JSON 对象,不包代码围栏,不输出任何其他文本。

Schema:
{"resources":[{"stage":0,"items":["资源建议(书/官方文档/课程/实践项目 + 一句推荐理由,≤40字)"]}]}

规则:
1. stage 为阶段序号,必须覆盖全部阶段,每阶段 2-4 条;
2. 只给资源名称与理由描述,严禁编造 URL 链接;
3. 优先经典教材、官方文档与动手实践项目;
4. 只输出 JSON。"#;

pub const STUDY_GUIDE: &str = r#"你是自学材料作者。为给定的一个学习知识点写一份可直接阅读的自学材料,输出 Markdown 正文(不要输出 JSON,不要把全文包进代码围栏)。

结构要求:
1. 以 `# <知识点名>` 一级标题开头;
2. 概念讲解:用通俗语言讲清是什么 / 为什么 / 怎么用,配 1-2 个可运行的示例(代码用围栏代码块);
3. 常见误区:列出 2-3 个初学者易错点并说明原因;
4. 小结自测:文末给 3 条自测问题,只列问题不给答案。

规则:
1. 内容自洽,不引用外部链接;按给定难度(1-5)调整深浅;
2. 篇幅约 800-1500 字;
3. 只输出 Markdown 正文。"#;

pub const STUDY_QUIZ: &str = r#"你是出题老师。针对给定的知识点出恰好 5 道测验题,只输出一个 JSON 对象,不包代码围栏,不输出任何其他文本。

Schema:
{"questions":[{"idx":0,"type":"choice","stem":"题干","options":["A. 选项一","B. 选项二","C. 选项三","D. 选项四"],"answer":"A","explanation":"一句话解析"},{"idx":1,"type":"judge","stem":"判断题题干","answer":"对","explanation":"一句话解析"},{"idx":4,"type":"short","stem":"简答题题干","answer":"参考答案要点","explanation":"一句话解析"}]}

规则:
1. 恰好 5 题:至少 2 道 choice、1 道 judge、至多 2 道 short,idx 从 0 递增;
2. choice 的 answer 必须是选项字母(A/B/C/D);judge 的 answer 只能是「对」或「错」;short 的 answer 为参考答案要点;
3. 题目覆盖该知识点的核心概念,整体由易到难;
4. 只输出 JSON。"#;

/// 内置模板内容。
pub fn builtin_content(key: &str) -> Option<&'static str> {
    match key {
        "study_intent" => Some(STUDY_INTENT),
        "study_map" => Some(STUDY_MAP),
        "study_stages" => Some(STUDY_STAGES),
        "study_resources" => Some(STUDY_RESOURCES),
        "study_guide" => Some(STUDY_GUIDE),
        "study_quiz" => Some(STUDY_QUIZ),
        _ => None,
    }
}

fn path() -> PathBuf {
    crate::paths::data_dir()
        .join("study")
        .join("templates.json")
}

/// 读取全部自定义覆盖(文件缺失/损坏返回空表)。
pub fn load_overrides() -> Vec<TemplateOverride> {
    let Ok(text) = fs::read_to_string(path()) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// 查某 key 的自定义覆盖。
pub fn find_override(key: &str) -> Option<TemplateOverride> {
    load_overrides().into_iter().find(|t| t.key == key)
}

/// 生效模板:自定义覆盖 > 内置。
pub fn effective(key: &str) -> String {
    if let Some(o) = find_override(key)
        && !o.content.trim().is_empty()
    {
        return o.content;
    }
    builtin_content(key).unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_content_available_for_all_keys() {
        for k in TEMPLATE_KEYS {
            assert!(!builtin_content(k).unwrap_or_default().is_empty());
        }
    }

    #[test]
    fn effective_falls_back_to_builtin() {
        assert_eq!(effective("study_map"), STUDY_MAP);
        assert_eq!(effective("no_such_key"), "");
    }
}
