//! 文档检索策略。
//!
//! 运行时预设功能已移除:工具恒放行(按 tool_load 按需加载),上下文窗口由后端
//! 按预算 / 会话设置治理,检索策略恒为默认档。本模块仅保留 `RetrievalPolicy`
//! 类型与默认策略,供 doc / mcp 层消费。

/// 检索策略:lexical = trigram3;vector/rerank 为混合检索开关;top_k 为返回块数。
/// 本地无 embedding 设施,语义约定:vector=true → 启用第二路词法信号(覆盖率打分)
/// 与 FTS 路做 RRF 混合;rerank=true → 召回 top20 按覆盖率 + 章节命中软重排后截 top_k。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct RetrievalPolicy {
    pub lexical: String,
    pub vector: bool,
    pub rerank: bool,
    pub top_k: u32,
}

/// 默认检索策略(三维度各取最大:向量 + 重排 + top_k=5)。
pub fn default_retrieval() -> RetrievalPolicy {
    RetrievalPolicy {
        lexical: "trigram3".into(),
        vector: true,
        rerank: true,
        top_k: 5,
    }
}

/// 取生效检索策略:预设功能已移除,恒返回默认策略。
pub fn retrieval_for_session(_session_id: &str) -> RetrievalPolicy {
    default_retrieval()
}
