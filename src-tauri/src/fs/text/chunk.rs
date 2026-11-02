//! 大文件分批写入(begin → append → finish,临时分片收尾落盘)。

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::fs::backup;

use super::common::{audit, session_id_is_bound_to_project, write_target, WriteResult};
use super::io::root_for_session;

// ---------------- 大文件分批写入(临时文件 → 收尾落盘) ----------------
// 模型单次回复有 token 上限,一次性写几十万字符必然被截断。这里按
// 「fs_write_begin → 多次 fs_write_chunk → fs_write_finish」三段式:
//   1. begin:按目标路径创建临时分片(数据目录 tmp/write,不污染工作区);
//   2. chunk:逐段追加(不设字节上限,模型每次输出一页即可);
//   3. finish:备份原文件后把分片整体落到目标路径(跨卷自动回退为复制)。
// 中途中断不会留下半截的目标文件(目标只在 finish 时被写)。

#[derive(Debug, Clone, Serialize)]
pub struct WriteChunkState {
    pub write_id: String,
    pub path: String,
    /// 已累计写入的字节数
    pub bytes: u64,
}

/// P10:单个写会话的存活上限 —— 超过即视为中断残留并清理。
pub(crate) const CHUNK_SESSION_TTL: std::time::Duration = std::time::Duration::from_secs(2 * 3600);
/// P10:同一会话最多并存的写会话数,防止模型忘记 finish 导致临时文件堆积。
pub(crate) const MAX_CHUNK_SESSIONS_PER_OWNER: usize = 4;

struct ChunkWrite {
    root: PathBuf,
    rel: String,
    target: PathBuf,
    temp: PathBuf,
    bytes: u64,
    /// 所属聊天会话(用于并发上限;None = 无会话上下文)
    owner: Option<String>,
    /// 创建时刻(用于 2h TTL;用墙钟便于测试回拨时间)
    created: std::time::SystemTime,
}

static CHUNK_WRITES: OnceLock<Mutex<HashMap<String, ChunkWrite>>> = OnceLock::new();

fn chunk_map() -> &'static Mutex<HashMap<String, ChunkWrite>> {
    CHUNK_WRITES.get_or_init(|| {
        // P10:进程启动后首次使用时清理过期分片(此前只在 begin 时清理)
        cleanup_stale_parts(&chunk_tmp_dir());
        Mutex::new(HashMap::new())
    })
}

/// 判断写会话是否已超过 TTL(时钟回拨时保守视为未过期)。
fn chunk_expired(w: &ChunkWrite) -> bool {
    std::time::SystemTime::now()
        .duration_since(w.created)
        .map(|elapsed| elapsed >= CHUNK_SESSION_TTL)
        .unwrap_or(false)
}

/// 清理 map 中已过期的写会话,返回需要删除的临时文件路径。
fn take_expired_parts(map: &mut HashMap<String, ChunkWrite>) -> Vec<PathBuf> {
    let mut expired = Vec::new();
    map.retain(|_, w| {
        if chunk_expired(w) {
            expired.push(w.temp.clone());
            false
        } else {
            true
        }
    });
    expired
}

/// 分批写入的临时目录(应用数据目录下 tmp/write)。
fn chunk_tmp_dir() -> PathBuf {
    crate::paths::data_dir().join("tmp").join("write")
}

pub fn write_chunk_begin(rel: &str, session_id: Option<&str>) -> Result<WriteChunkState, String> {
    let root = root_for_session(session_id)?;
    let enforce = session_id_is_bound_to_project(session_id);
    write_chunk_begin_in(&root, rel, enforce, &chunk_tmp_dir(), session_id)
}

pub(crate) fn write_chunk_begin_in(
    root: &Path,
    rel: &str,
    enforce: bool,
    tmp: &Path,
    owner: Option<&str>,
) -> Result<WriteChunkState, String> {
    if rel.trim().is_empty() {
        return Err("缺少 path 参数".into());
    }
    std::fs::create_dir_all(tmp).map_err(|e| format!("创建临时目录失败: {e}"))?;
    cleanup_stale_parts(tmp);
    let mut map = chunk_map()
        .lock()
        .map_err(|_| "写入会话锁已被污染".to_string())?;
    // P10:先回收超过 TTL 的写会话,再校验并发上限,避免无效目标目录被提前创建
    let expired = take_expired_parts(&mut map);
    for p in expired {
        let _ = std::fs::remove_file(&p);
    }
    if let Some(sid) = owner {
        let active = map
            .values()
            .filter(|w| w.owner.as_deref() == Some(sid))
            .count();
        if active >= MAX_CHUNK_SESSIONS_PER_OWNER {
            return Err(format!(
                "本会话仍有 {active} 个未收尾的分批写入(上限 {MAX_CHUNK_SESSIONS_PER_OWNER});\
                 请先 fs_write_finish 完成或放弃其中一个,再重新 begin"
            ));
        }
    }
    // 目标路径在这次 begin 就解析并校验(项目绑定场景做越界检查),后续分片只写临时文件
    let target = write_target(root, rel, enforce)?;
    let id = uuid::Uuid::new_v4().to_string();
    let temp = tmp.join(format!("{id}.part"));
    std::fs::write(&temp, b"").map_err(|e| format!("创建临时文件失败: {e}"))?;
    map.insert(
        id.clone(),
        ChunkWrite {
            root: root.to_path_buf(),
            rel: rel.to_string(),
            target,
            temp,
            bytes: 0,
            owner: owner.map(str::to_string),
            created: std::time::SystemTime::now(),
        },
    );
    drop(map);
    audit("fs_write_begin", rel, 0);
    Ok(WriteChunkState {
        write_id: id,
        path: rel.to_string(),
        bytes: 0,
    })
}

/// 过期会话的收口提示(统一文案,便于模型据此重新 begin)。
fn chunk_expired_msg(write_id: &str) -> String {
    format!(
        "写入会话 `{write_id}` 已超时(超过 {} 小时未收尾),临时分片已清理;请重新调用 fs_write_begin",
        CHUNK_SESSION_TTL.as_secs() / 3600
    )
}

/// 追加上一段内容。返回累计字节数,模型可据此确认是否写全。
pub fn write_chunk_append(write_id: &str, content: &str) -> Result<WriteChunkState, String> {
    use std::io::Write as _;

    let mut map = chunk_map()
        .lock()
        .map_err(|_| "写入会话锁已被污染".to_string())?;
    // P10:过期会话立即清理并提示重新 begin,避免往已被外部清理的分片里写
    if map.get(write_id).is_some_and(chunk_expired) {
        if let Some(old) = map.remove(write_id) {
            let _ = std::fs::remove_file(&old.temp);
        }
        return Err(chunk_expired_msg(write_id));
    }
    let w = map.get_mut(write_id).ok_or_else(|| {
        format!("写入会话 `{write_id}` 不存在或已结束;请重新调用 fs_write_begin")
    })?;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&w.temp)
        .map_err(|e| format!("打开临时文件失败: {e}"))?;
    f.write_all(content.as_bytes())
        .map_err(|e| format!("写入临时文件失败: {e}"))?;
    w.bytes += content.len() as u64;
    Ok(WriteChunkState {
        write_id: write_id.to_string(),
        path: w.rel.clone(),
        bytes: w.bytes,
    })
}

/// 收尾:备份目标文件后把分片落到目标路径。
/// 中途失败时把写会话塞回注册表,模型可修正后重试 finish,不必重传所有分片。
pub fn write_chunk_finish(write_id: &str) -> Result<WriteResult, String> {
    let w = {
        let mut map = chunk_map()
            .lock()
            .map_err(|_| "写入会话锁已被污染".to_string())?;
        map.remove(write_id)
            .ok_or_else(|| format!("写入会话 `{write_id}` 不存在或已结束"))?
    };
    if chunk_expired(&w) {
        let _ = std::fs::remove_file(&w.temp);
        return Err(chunk_expired_msg(write_id));
    }
    let fail = |w: ChunkWrite, msg: String| -> Result<WriteResult, String> {
        if let Ok(mut map) = chunk_map().lock() {
            map.insert(write_id.to_string(), w);
        }
        Err(msg)
    };
    let backup_id = match backup::snapshot_in(&backup::backup_dir(), &w.root, &w.target, None, None) {
        Ok(b) => b,
        Err(e) => return fail(w, e),
    };
    if let Some(parent) = w.target.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return fail(w, format!("创建目标目录失败: {e}"));
    }
    // 同卷优先 rename(原子);数据目录与工作区跨卷时回退为复制
    if std::fs::rename(&w.temp, &w.target).is_err() {
        if let Err(e) = std::fs::copy(&w.temp, &w.target) {
            return fail(w, format!("落盘失败: {e}"));
        }
        let _ = std::fs::remove_file(&w.temp);
    }
    audit("fs_write", &w.rel, w.bytes as usize);
    Ok(WriteResult {
        rel: w.rel,
        bytes: w.bytes,
        backup_id,
    })
}

/// write_id 是否仍有效(供审批策略判断:目标路径已在 begin 时校验通过)。
pub fn write_chunk_known(write_id: &str) -> bool {
    let Ok(mut map) = chunk_map().lock() else {
        return false;
    };
    match map.get(write_id) {
        Some(w) if !chunk_expired(w) => true,
        Some(_) => {
            if let Some(old) = map.remove(write_id) {
                let _ = std::fs::remove_file(&old.temp);
            }
            false
        }
        None => false,
    }
}

/// 清理超过 TTL(2h)未收尾的临时分片,避免中断后残留堆积。
fn cleanup_stale_parts(tmp: &Path) {
    let cutoff = std::time::SystemTime::now().checked_sub(CHUNK_SESSION_TTL);
    let Ok(entries) = std::fs::read_dir(tmp) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("part") {
            continue;
        }
        if let (Some(cutoff), Ok(t)) = (cutoff, entry.metadata().and_then(|m| m.modified()))
            && t.duration_since(cutoff).is_err()
        {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// 测试专用:把写会话的创建时间回拨,模拟超时。
#[cfg(test)]
pub(crate) fn backdate_chunk(write_id: &str, age: std::time::Duration) {
    if let Ok(mut map) = chunk_map().lock()
        && let Some(w) = map.get_mut(write_id)
    {
        w.created = std::time::SystemTime::now()
            .checked_sub(age)
            .unwrap_or(w.created);
    }
}

