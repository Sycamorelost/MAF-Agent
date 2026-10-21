//! 学习规划引擎(P0):确定性流水线 ——
//!
//! 来源声明 / Attribution:
//!   Pipeline and prompt contracts derived from Deep-tutor
//!   https://github.com/hershate/Deep-tutor — Copyright (c) hershate and contributors
//!   Licensed under the Apache License, Version 2.0 (see LICENSE / NOTICE).
//!   Rewritten in Rust with deterministic validation and scheduling; see NOTICE.
//!
//! ```text
//! intent(意图合并/兜底) → map(知识地图) → stages(阶段设计,校验重试≤3+降级 repaired)
//!   → [resources ∥ schedule](资料推荐 ∥ 确定性排期) → validate → export(落库)
//! ```
//!
//! - LLM 调用/事件流/暂停终止复用研究引擎同款设施(Provider + Channel + AtomicBool);
//! - 结构校验(阶段数/难度阶梯/知识点覆盖/前置引用)在 Rust 端确定性完成,不依赖模型自检;
//! - 排期为确定性计算(零 LLM 调用,零幻觉);资料推荐为单次 LLM 调用,失败静默跳过。

pub mod learn;
pub mod pipeline;
pub mod schedule;
pub mod store;
pub mod template;
pub mod validate;

use serde::{Deserialize, Serialize};

pub use pipeline::{preflight, run, StudyParams};
#[allow(unused_imports)]
pub use schedule::{calc_schedule, export_markdown, store_payload_nodes};
#[allow(unused_imports)]
pub use validate::{extract_json, fallback_repaired, validate_stages};

// ---------------- 契约类型 ----------------

/// null → 类型默认值:模型常输出 `"field": null`,serde(default) 只兜「字段缺失」不兜显式 null。
fn null_to_default<'de, T, D>(d: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + Default,
    D: serde::Deserializer<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// 学习意图(preflight 产出 / 流水线合并结果)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Intent {
    #[serde(default, deserialize_with = "null_to_default")]
    pub topic: String,
    /// zero / beginner / intermediate / advanced / 空
    #[serde(default, deserialize_with = "null_to_default")]
    pub level: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub goal: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub daily_minutes: u32,
    pub deadline: Option<String>,
    #[serde(default, deserialize_with = "null_to_default")]
    pub lang: String,
    /// 缺失的可选字段名(preflight 用)
    #[serde(default, deserialize_with = "null_to_default")]
    pub missing: Vec<String>,
    /// 追问(preflight 用)
    #[serde(default, deserialize_with = "null_to_default")]
    pub questions: Vec<String>,
}

impl Default for Intent {
    fn default() -> Self {
        Self {
            topic: String::new(),
            level: String::new(),
            goal: String::new(),
            daily_minutes: 30,
            deadline: None,
            lang: "zh".into(),
            missing: Vec::new(),
            questions: Vec::new(),
        }
    }
}

/// 知识点(知识地图节点)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSpec {
    pub name: String,
    /// 1-5
    pub difficulty: u8,
    #[serde(default)]
    pub why: String,
    #[serde(default)]
    pub must_learn: bool,
    #[serde(default)]
    pub prereqs: Vec<String>,
}

/// 学习阶段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageSpec {
    pub index: u32,
    pub name: String,
    #[serde(default)]
    pub goal: String,
    pub difficulty: u8,
    #[serde(default = "default_weeks")]
    pub estimated_weeks: u32,
    pub topics: Vec<String>,
    #[serde(default)]
    pub milestone: String,
}

fn default_weeks() -> u32 {
    2
}

/// 周粒度排期行(确定性计算)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleWeek {
    pub week: u32,
    pub stage: u32,
    pub focus: String,
    pub minutes_per_day: u32,
}

/// 阶段资料推荐。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResource {
    pub stage: u32,
    pub items: Vec<String>,
}

/// 计划载荷(stages_json / plan.json / Done 事件的完整内容)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanPayload {
    pub topic: String,
    pub level: String,
    pub goal: String,
    pub daily_minutes: u32,
    pub deadline: Option<String>,
    pub lang: String,
    pub nodes: Vec<NodeSpec>,
    pub stages: Vec<StageSpec>,
    pub schedule: Vec<ScheduleWeek>,
    pub resources: Vec<StageResource>,
    /// true = 校验失败降级生成的默认结构
    pub repaired: bool,
}

impl Default for PlanPayload {
    fn default() -> Self {
        Self {
            topic: String::new(),
            level: String::new(),
            goal: String::new(),
            daily_minutes: 30,
            deadline: None,
            lang: "zh".into(),
            nodes: Vec::new(),
            stages: Vec::new(),
            schedule: Vec::new(),
            resources: Vec::new(),
            repaired: false,
        }
    }
}

/// 过程事件(推前端 + 落 trail.jsonl)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StudyEvent {
    /// 流水线步骤开始(step: intent/map/stages/resources/schedule/validate/export)
    StepStart { step: String, label: String },
    /// 步骤结果摘要行
    StepNote { step: String, content: String },
    /// 校验失败带反馈重试
    Retry { step: String, attempt: u32, reason: String },
    Paused,
    Resumed,
    Done { plan: Option<PlanPayload> },
    Cancelled,
    Error { message: String },
}

/// 面板已确认的意图字段(preflight 前用户已手工填写,不再追问)。
#[derive(Debug, Default, Clone)]
pub struct IntentHints {
    pub level: Option<String>,
    pub daily_minutes: Option<u32>,
    pub deadline: Option<String>,
}
