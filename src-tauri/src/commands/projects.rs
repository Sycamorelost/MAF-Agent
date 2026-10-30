//! 项目 CRUD Tauri commands(P8.1 项目化升级)。
//!
//! 参考 Cursor Projects 设计:
//! - 项目 = 用户选本地磁盘路径 + 名字 + 可选模型覆盖 + 可选共享上下文
//! - 一个项目下可以挂多个任务会话
//! - 前端先调 fs_pick_workspace_dir(系统原生目录选择器)拿到路径,再调 create

#![allow(dead_code)]

use crate::db::{projects, sessions};

#[tauri::command]
pub fn project_list() -> Result<Vec<projects::ProjectRow>, String> {
    projects::list()
}

/// 创建项目 —— path 必须是用户选定的绝对路径且目录存在。
/// name 为空时后端自动从 path 推断目录名。
#[tauri::command]
pub fn project_create(name: String, path: String) -> Result<projects::ProjectRow, String> {
    projects::create(&name, &path)
}

#[tauri::command]
pub fn project_rename(id: String, name: String) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("项目名不能为空".into());
    }
    if name.chars().count() > 64 {
        return Err("项目名不能超过 64 字".into());
    }
    projects::rename(&id, &name)
}

#[tauri::command]
pub fn project_update_path(id: String, path: String) -> Result<(), String> {
    projects::update_path(&id, &path)
}

#[tauri::command]
pub fn project_update_model(id: String, model: Option<String>) -> Result<(), String> {
    projects::update_model_override(&id, model.as_deref())
}

#[tauri::command]
pub fn project_update_context(id: String, ctx: Option<String>) -> Result<(), String> {
    projects::update_shared_context(&id, ctx.as_deref())
}

#[tauri::command]
pub fn project_delete(id: String) -> Result<(), String> {
    projects::delete(&id)
}

#[tauri::command]
pub fn project_update_sort(id: String, sort: i64) -> Result<(), String> {
    projects::update_sort(&id, sort)
}

#[tauri::command]
pub fn session_set_project(session_id: String, project_id: Option<String>) -> Result<(), String> {
    let pid: Option<&str> = project_id.as_deref();
    sessions::set_project(&session_id, pid)
}
