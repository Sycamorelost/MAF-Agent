//! AI 指令库相关 Tauri commands(迭代 B3 / F1)。

use crate::text_commands::{self, TextCommand};

/// 列出全部指令(内置 + 自定义,按 sort 排序)。
#[tauri::command]
pub fn list_text_commands() -> Vec<TextCommand> {
    text_commands::list_all()
}

/// 保存自定义指令(upsert;内置 id 拒绝覆盖)。
#[tauri::command]
pub fn save_text_command(cmd: TextCommand) -> Result<(), String> {
    text_commands::upsert_custom(cmd)
}

/// 删除自定义指令(内置指令拒绝)。
#[tauri::command]
pub fn delete_text_command(id: String) -> Result<(), String> {
    text_commands::remove_custom(&id)
}

/// 渲染指令模板:{selection} 由调用方传入(划词文本;对话场景传 None),
/// {clipboard} 由后端 FFI 读取剪贴板。
#[tauri::command]
pub fn render_text_command(id: String, selection: Option<String>) -> Result<String, String> {
    text_commands::render(&id, selection.as_deref())
}
