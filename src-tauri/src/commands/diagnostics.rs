//! 诊断信息 IPC(P2)。
//!
//! 只读暴露「静默失败」计数(事件发送 / 会话事件落库 / MCP 审计写入),
//! 供设置页「关于」在计数非 0 时提示用户;不提供任何修改入口。

use crate::diagnostics::FailureCounts;

#[tauri::command]
pub async fn diagnostics_failure_counts() -> Result<FailureCounts, String> {
    Ok(crate::diagnostics::failure_counts())
}
