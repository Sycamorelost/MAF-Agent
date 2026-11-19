//! 主循环共享的数据结构。

use super::super::super::pricing::PricingCtx;

/// 用量归因上下文(与 Key 选择无关,仅影响落库内容)。
pub(crate) struct Attribution {
    pub(crate) session_id: Option<String>,
    pub(crate) agent_id: Option<String>,
    pub(crate) agent_name: Option<String>,
    pub(crate) pricing: Option<PricingCtx>,
}
