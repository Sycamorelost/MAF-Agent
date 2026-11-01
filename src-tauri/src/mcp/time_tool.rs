//! 当前时间(中立实现)。
//!
//! 从 `commands::mcp::get_now` 上提:`mcp::builtin` 执行 `get_now` 工具时直接调用本模块,
//! `commands` 只做 IPC 适配,避免执行层反向依赖命令层。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowResult {
    pub iso: String,
    pub unix_ms: i64,
    pub tz: String,
    pub date: String,
    pub time: String,
    pub weekday: String,
}

/// 取当前本地时间与日期。
pub fn now() -> NowResult {
    use chrono::Local;
    let now = Local::now();
    let weekday_names = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
    let wd_idx = now.format("%u").to_string().parse::<usize>().unwrap_or(1);
    NowResult {
        iso: now.to_rfc3339(),
        unix_ms: now.timestamp_millis(),
        tz: now.format("%:z").to_string(),
        date: now.format("%Y-%m-%d").to_string(),
        time: now.format("%H:%M:%S").to_string(),
        weekday: weekday_names
            .get(wd_idx - 1)
            .copied()
            .unwrap_or("")
            .to_string(),
    }
}
