#![allow(dead_code)]

//! 文件写入备份与回滚 —— 覆盖/删除前自动快照,支持按 id 恢复。
//!
//! 存储布局:`<数据目录>/backups/<id>/payload` + `meta.json`。
//! meta 记录相对路径、时间、写入前是否存在,恢复时据此还原或删除新建文件。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 备份条目(列表展示用)。
#[derive(Debug, Clone, Serialize)]
pub struct BackupEntry {
    pub id: String,
    pub rel: String,
    pub created: String,
    pub bytes: u64,
    pub existed: bool,
    /// V28/P8:归属会话 id(旧备份没有 → null)。
    pub session_id: Option<String>,
    /// V28/P8:归属文件夹 id(旧备份没有 → null)。
    pub folder_id: Option<String>,
}

/// 备份根目录。
pub fn backup_dir() -> PathBuf {
    crate::paths::data_dir().join("backups")
}

/// 列出备份(按 id 倒序,即时间倒序),最多 200 条。
/// `session_id` 为 Some 时只返回该会话创建的备份;None 时返回全部。
pub fn list(session_id: Option<&str>) -> Vec<BackupEntry> {
    list_in(&backup_dir(), session_id)
}

/// 恢复到指定备份;恢复前会对当前文件再做一次快照,保证恢复本身可撤销。
pub fn restore(root: &Path, id: &str) -> Result<String, String> {
    restore_in(&backup_dir(), root, id)
}

/// 读取备份 payload 文本(P6 检查器 diff 视图);备份为「新建前不存在」或二进制时返回说明。
pub fn read_payload(id: &str) -> Result<String, String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("备份 id 非法".into());
    }
    let dir = backup_dir().join(id);
    let raw = std::fs::read_to_string(dir.join("meta.json"))
        .map_err(|e| format!("备份不存在或已损坏: {e}"))?;
    let meta: Meta = serde_json::from_str(&raw).map_err(|e| format!("备份元数据损坏: {e}"))?;
    if !meta.existed {
        return Ok("（该备份记录的是「新建文件」:写入前目标不存在,恢复即删除该文件）".into());
    }
    let bytes = std::fs::read(dir.join("payload")).map_err(|e| format!("读取备份内容失败: {e}"))?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(text.chars().take(200_000).collect())
}

#[derive(Debug, Serialize, Deserialize)]
struct Meta {
    rel: String,
    created: String,
    existed: bool,
    bytes: u64,
    /// V28/P8:归属会话 id(旧备份没有 → None)。
    #[serde(default)]
    session_id: Option<String>,
    /// V28/P8:归属文件夹 id(旧备份没有 → None)。
    #[serde(default)]
    folder_id: Option<String>,
}

// ---------------- 可测试的内核(显式传入备份根) ----------------

/// 对 `path` 做一次快照,返回备份 id(文件不存在时返回 None)。
/// `session_id` / `folder_id` 记录归属,供检查器按会话过滤。
pub(crate) fn snapshot_in(
    base: &Path,
    root: &Path,
    path: &Path,
    session_id: Option<&str>,
    folder_id: Option<&str>,
) -> Result<Option<String>, String> {
    if !path.exists() {
        return Ok(None);
    }
    // 工作区内的文件存相对路径;绝对路径(工作区外)直接落绝对路径,恢复时原样解析
    let rel = crate::fs::paths::rel_of(root, path).unwrap_or_else(|_| {
        let s = path.display().to_string();
        s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
    });
    let created = chrono::Local::now().to_rfc3339();
    let short = uuid::Uuid::new_v4().simple().to_string();
    let id = format!(
        "{}-{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S"),
        &short[..6]
    );
    let dir = base.join(&id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建备份目录失败: {e}"))?;
    let bytes =
        std::fs::copy(path, dir.join("payload")).map_err(|e| format!("备份文件失败: {e}"))?;
    let meta = Meta {
        rel,
        created,
        existed: true,
        bytes,
        session_id: session_id.map(|s| s.to_string()),
        folder_id: folder_id.map(|s| s.to_string()),
    };
    write_meta(&dir, &meta)?;
    Ok(Some(id))
}

fn list_in(base: &Path, session_id: Option<&str>) -> Vec<BackupEntry> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(base) else {
        return out;
    };
    for entry in rd.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(dir.join("meta.json")) else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<Meta>(&raw) else {
            continue;
        };
        // 按 session_id 过滤:调用方指定时才过滤
        if let Some(sid) = session_id {
            if meta.session_id.as_deref() != Some(sid) {
                continue;
            }
        }
        out.push(BackupEntry {
            id: entry.file_name().to_string_lossy().to_string(),
            rel: meta.rel,
            created: meta.created,
            bytes: meta.bytes,
            existed: meta.existed,
            session_id: meta.session_id,
            folder_id: meta.folder_id,
        });
    }
    out.sort_by(|a, b| b.id.cmp(&a.id));
    out.truncate(200);
    out
}

fn restore_in(base: &Path, root: &Path, id: &str) -> Result<String, String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("备份 id 非法".into());
    }
    let dir = base.join(id);
    let raw = std::fs::read_to_string(dir.join("meta.json"))
        .map_err(|e| format!("备份不存在或已损坏: {e}"))?;
    let meta: Meta = serde_json::from_str(&raw).map_err(|e| format!("备份元数据损坏: {e}"))?;
    let target = crate::fs::paths::resolve(root, &meta.rel, false)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    // 恢复前先快照当前内容,使"恢复"本身可撤销
    if target.exists() {
        snapshot_in(base, root, &target, None, None)?;
    }
    if meta.existed {
        let payload = dir.join("payload");
        std::fs::copy(&payload, &target).map_err(|e| format!("恢复失败: {e}"))?;
    } else if target.exists() {
        std::fs::remove_file(&target).map_err(|e| format!("撤销新建文件失败: {e}"))?;
    }
    Ok(meta.rel)
}

fn write_meta(dir: &Path, meta: &Meta) -> Result<(), String> {
    let json = serde_json::to_string_pretty(meta).map_err(|e| format!("序列化失败: {e}"))?;
    std::fs::write(dir.join("meta.json"), json).map_err(|e| format!("写 meta 失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("maf-bak-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn snapshot_and_restore_roundtrip() {
        let base = temp("base");
        let root = temp("ws");
        let file = root.join("notes/a.txt");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "v1").unwrap();

        let id = snapshot_in(&base, &root, &file, None, None)
            .unwrap()
            .expect("应生成备份");
        std::fs::write(&file, "v2").unwrap();
        let rel = restore_in(&base, &root, &id).unwrap();
        assert_eq!(rel, "notes/a.txt");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1");
    }

    #[test]
    fn list_and_reject_bad_id() {
        let base = temp("base2");
        let root = temp("ws2");
        let file = root.join("x.txt");
        std::fs::write(&file, "x").unwrap();
        snapshot_in(&base, &root, &file, None, None).unwrap();
        assert_eq!(list_in(&base, None).len(), 1);
        assert!(restore_in(&base, &root, "../evil").is_err());
    }
}
