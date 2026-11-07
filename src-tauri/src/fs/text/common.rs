//! 共享类型与内部小工具(所有子模块复用)。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::fs::paths;

/// 单次读取默认页大小(字符)。调用方可用 offset/max_chars 分页续读,文件本身不设大小上限。
pub const MAX_READ_CHARS: usize = 32_000;
/// 列表返回上限。
pub(crate) const MAX_LIST_ENTRIES: usize = 500;

#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub rel: String,
    pub is_dir: bool,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteResult {
    pub rel: String,
    pub bytes: u64,
    /// 覆盖已有文件时生成的备份 id(新建文件为 null)
    pub backup_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SheetInput {
    pub name: String,
    pub rows: Vec<Vec<String>>,
}

/// 判断 session 是否绑定了有效项目路径 —— 用于决定写操作要不要做越界检查。
pub(crate) fn session_id_is_bound_to_project(session_id: Option<&str>) -> bool {
    if let Some(sid) = session_id
        && let Ok(Some(session)) = crate::db::sessions::get(sid)
        && let Some(ref pid) = session.project_id
        && let Ok(Some(proj)) = crate::db::projects::get(pid)
    {
        return std::path::Path::new(&proj.path).is_dir();
    }
    false
}

/// 去掉 canonicalize 产生的 `\\?\` 前缀,便于回显与展示。
pub(crate) fn display_path(p: &Path) -> String {
    let s = p.display().to_string();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

/// 写目标:校验路径并确保父目录存在。
/// 统一走 S1 沙箱语义(resolve_guarded):相对路径必须在 root 内,绝对路径显式放行。
pub(crate) fn write_target(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let path = paths::resolve_guarded(root, rel)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    Ok(path)
}

pub(crate) fn audit(event: &str, rel: &str, bytes: usize) {
    crate::mcp::audit(
        &crate::paths::data_dir(),
        event,
        "fs",
        rel,
        &format!("bytes={bytes}"),
    );
}
