//! 研究过程事件协议与事件出口。

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

use super::store;

/// 一条已分配编号的引用来源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceItem {
    pub id: u32,
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub engine: String,
    pub published: Option<String>,
}

/// 研究过程事件(推前端 + 落 trail.jsonl)。
///
/// 专项研究(步骤 2):轮次级事件新增 `stage` 字段(serde default 0,兼容旧 trail 回放);
/// 新增阶段级事件 StageStart / StageDone / StageSkipped / AngleGap / Checkpoint。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResearchEvent {
    RoundStart {
        round: u32,
        max_rounds: u32,
        #[serde(default)]
        stage: u32,
    },
    Thought {
        round: u32,
        content: String,
        #[serde(default)]
        stage: u32,
    },
    Queries {
        round: u32,
        queries: Vec<String>,
        #[serde(default)]
        stage: u32,
    },
    Searching {
        round: u32,
        query: String,
        #[serde(default)]
        stage: u32,
    },
    Sources {
        round: u32,
        sources: Vec<SourceItem>,
        #[serde(default)]
        stage: u32,
    },
    Observation {
        round: u32,
        content: String,
        #[serde(default)]
        stage: u32,
    },
    Paused {
        round: u32,
        #[serde(default)]
        stage: u32,
    },
    Resumed {
        round: u32,
        #[serde(default)]
        stage: u32,
    },
    /// 阶段开始(专项模式)
    StageStart {
        stage: u32,
        name: String,
        kind: String,
    },
    /// 阶段完成,附阶段摘要与新增资料数;partial=true 表示因失败策略提前中止(M2,default 兼容旧 trail)
    StageDone {
        stage: u32,
        summary: String,
        sources_count: u32,
        #[serde(default)]
        partial: bool,
    },
    /// 阶段跳过(停用 / 全局约束耗尽)
    StageSkipped {
        stage: u32,
        reason: String,
    },
    /// 必覆盖角度缺口提醒(校验为提醒,不阻断)
    AngleGap {
        stage: u32,
        missing: Vec<String>,
    },
    /// 检查点:阶段结束暂停等待确认(步骤 3 启用暂停逻辑,本步仅协议)
    Checkpoint {
        stage: u32,
        next: String,
    },
    ReportDelta {
        content: String,
    },
    Done {
        report: String,
    },
    Cancelled,
    Error {
        message: String,
    },
}

/// 事件出口:同时落盘 + 推前端。发送失败不中断研究(便于事后查 trail)。
pub(crate) struct EventSink {
    pub(crate) id: String,
    pub(crate) channel: Channel<ResearchEvent>,
}

impl EventSink {
    pub(crate) fn emit(&self, ev: ResearchEvent) {
        let _ = store::append_trail(&self.id, &ev);
        let _ = self.channel.send(ev);
    }
}
