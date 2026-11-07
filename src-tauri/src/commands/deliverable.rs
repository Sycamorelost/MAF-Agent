//! 产物(交付物)只读查询命令(M2)。
//!
//! 写入只发生在 `present` 工具执行期与消息落库绑定期;前端只读。
//! 会话删除 / 消息删除时的清理见 `db::sessions` 与 `db::messages::crud`。

use crate::deliverables::{self, Deliverable};

/// 列出某会话的全部产物(按登记顺序),前端按 message_id 归入各条消息。
#[tauri::command]
pub async fn list_deliverables(session_id: String) -> Result<Vec<Deliverable>, String> {
    Ok(deliverables::list_by_session(&session_id))
}
