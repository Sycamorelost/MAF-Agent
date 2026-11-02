//! AI 检测引擎主入口(R3 B1)。
//!
//! 职责拆分:
//! - `types.rs`:引擎版本、免责声明、报告与维度数据结构;
//! - `analyze.rs`:主入口 `analyze()` + 总分加权 + 句子热力标注;
//! - `stats.rs`:基础统计函数 + V3 动态统计 proxy(纯函数,方便单测)
//!
//! 全部计算在本地完成,不调用任何模型;只回答「哪里的写作特征像 AI」,
//! 不作「人写 / AI 写」的二值判定(伦理要求见 `DISCLAIMER`)。

mod analyze;
mod stats;
mod types;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use analyze::analyze;
#[allow(unused_imports)]
pub use stats::*;
#[allow(unused_imports)]
pub use types::*;
