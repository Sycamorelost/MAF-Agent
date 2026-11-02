//! 文档驻留层(R3 C1/C2,val.md §2.3 / §8 C1)。
//!
//! 附件在保存时提取的正文不再只塞进消息 JSON:同时按块(目标 1000 字符)入库
//! `doc_chunks`,保留页码与章节路径。会话内可通过 `doc_search` / `doc_read`
//! 内置工具按需回看任意章节 —— 长文档不再只靠一次性注入,也不会因上下文裁剪而「失忆」。
//!
//! 生命周期:块随会话级联删除(表上 `REFERENCES sessions(id) ON DELETE CASCADE`),
//! 不入任何远端;本模块只用本地 SQLite。
//!
//! 编排入口在 [`service`](service):提取入库 / 摘要 / 删除 / 目录迁移都走那里。
//! 文件职责:
//! - `chunk.rs`:分块、摘要卡、内容指纹等纯算法;
//! - `store.rs`:块索引的存取(建索引 / 列出文档 / 记忆骨架);
//! - `search.rs`:token 切分、FTS 与覆盖率打分、RRF 融合;
//! - `read.rs`:按块读取原文。

pub mod service;

#[cfg(test)]
mod bench;
#[cfg(test)]
mod tests;

mod chunk;
mod read;
mod search;
mod store;

#[allow(unused_imports)]
pub use chunk::{CHUNK_TARGET, DocChunk, chunk_text, doc_id_of, summary};
#[allow(unused_imports)]
pub use read::{READ_MAX_CHARS, read_range};
#[allow(unused_imports)]
pub use search::{SearchHit, SearchOutcome, query_tokens, search, search_with_policy};
#[allow(unused_imports)]
pub use store::{DocInfo, index_document, list_docs, outline};

// 基准测试(bench)直接调用打分路,保持 `crate::doc::coverage_candidates` 可用。
#[allow(unused_imports)]
pub(crate) use search::coverage_candidates;
