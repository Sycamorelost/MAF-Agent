//! 深度研究引擎(Phase 2):多轮搜索 + ReAct 循环 + 引用列表。
//!
//! 流程对齐 idea/02-features.md §3:
//!   1. ReAct 循环:Thought → Action(search / finish)→ Queries → 搜索 → Observation
//!   2. 达到 finish 或 max_rounds 后,基于收集的资料流式生成带 [n] 引用的研究报告
//!   3. 过程事件实时推给前端(Channel),同时落盘 trail.jsonl;报告落盘 report.md

pub mod blueprint;
pub mod preset;
pub mod search;
pub mod skills;
pub mod stage;
pub mod store;
pub mod template;

mod events;
mod export;
mod figures;
mod llm;
mod params;
mod pipeline;
mod prompts;
mod references;
mod validate;

pub use events::{ResearchEvent, SourceItem};
pub use params::ResearchParams;
#[allow(unused_imports)]
pub use references::{
    REF_FILE_BUDGET_CHARS, REF_TOTAL_BUDGET_CHARS, references_cache_dir, references_dir,
};

// 内置提示词常量供子模块(template.rs)经 `super::` 访问。
pub(crate) use prompts::{REACT_SYSTEM, REPORT_SYSTEM, REPORT_SYSTEM_EN};

use std::sync::{Arc, atomic::AtomicBool};

use tauri::ipc::Channel;

use crate::provider::Provider;
use search::SearchEngine;

use events::EventSink;
use figures::attach_image_draft;
use pipeline::{RunError, run_concise, run_legacy, run_staged};

// ---------------- 引擎运行参数 ----------------

/// 引擎运行参数。
pub struct EngineParams {
    pub id: String,
    pub topic: String,
    pub provider: Box<dyn Provider>,
    pub model: String,
    pub engine: Box<dyn SearchEngine>,
    pub params: ResearchParams,
    pub cancel: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
    pub channel: Channel<ResearchEvent>,
    /// H3:stream_options 被中转站拒绝时,无该字段重建 Provider 所需的入参
    pub rebuild: Option<ProviderRebuild>,
    /// H3:回退重建后的 Provider(重建一次,后续 LLM 调用复用)
    pub fallback: std::sync::Mutex<Option<Arc<dyn Provider>>>,
    /// 用户导入参考资料的一次性注入块(启动时装配,避免每轮重复读盘/解析)
    pub references: Option<String>,
}

/// H3:重建 Provider(关闭 stream_options)所需的启动入参快照。
pub struct ProviderRebuild {
    pub provider_id: String,
    pub model: String,
    pub custom_base_url: Option<String>,
    pub custom_display_name: Option<String>,
}

// ---------------- 主入口 ----------------

/// 运行一次研究(在后台任务中被 spawn)。负责终态状态落库。
pub async fn run(p: EngineParams) {
    let sink = EventSink {
        id: p.id.clone(),
        channel: p.channel.clone(),
    };
    match run_inner(&p, &sink).await {
        Ok(report) => {
            // 科研增强:AI 概念示意草稿(默认关;失败静默降级,不影响报告交付)
            let report = attach_image_draft(&p, report).await;
            let _ = store::save_report(&p.id, &report);
            let _ = store::update_status(&p.id, "done", None);
            sink.emit(ResearchEvent::Done { report });
        }
        Err(RunError::Cancelled) => {
            let _ = store::update_status(&p.id, "cancelled", None);
            sink.emit(ResearchEvent::Cancelled);
        }
        Err(RunError::Fatal(msg)) => {
            let _ = store::update_status(&p.id, "error", Some(&msg));
            sink.emit(ResearchEvent::Error { message: msg });
        }
    }
}

/// 模式分发:concise → 单对话迭代;staged 且阶段非空 → 阶段流水线;否则原单层 ReAct(legacy)。
async fn run_inner(p: &EngineParams, sink: &EventSink) -> Result<String, RunError> {
    if p.params.mode == "concise" {
        run_concise(p, sink).await
    } else if p.params.mode == "staged" && !p.params.stages.is_empty() {
        run_staged(p, sink).await
    } else {
        run_legacy(p, sink).await
    }
}
