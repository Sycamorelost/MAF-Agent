//! API 流转 · 用量统计(usage_record)读写。
//!
//! 拆分(§10.2.1 A 阶段):`record` 负责写入与掩码,`query` 负责聚合与明细;
//! 本文件仅做声明与再导出,调用方路径保持不变(`crate::db::usage::*`)。

mod query;
mod record;

pub use query::{
    UsageEntry, UsageGroup, UsageHour, UsageSummary, entries, hourly, key_health, summary,
    token_mix,
};
#[cfg(test)]
pub use record::insert_on;
pub use record::{UsageRecord, mask_key, record};
