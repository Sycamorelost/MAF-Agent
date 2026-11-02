//! 附件拒绝原因留存(迭代 F7.1)。
//!
//! 导入被拒绝的文件(格式黑名单 / 空文件 / 超 20MB / 配额超限 / 后端保存失败)
//! 以 JSONL 追加落盘到 `<数据目录>/attachment_rejects.jsonl`,最多保留最近 2000 条。
//! 附件中心可查看与清空,便于事后排查「为什么这个文件进不来」。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// 落盘记录(不含会话标题;标题在读取时联表补全)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredReject {
    pub time: String,
    pub session_id: String,
    pub name: String,
    pub reason: String,
    pub source: String,
}

/// 前端提交的拒绝项
#[derive(Debug, Clone, Deserialize)]
pub struct RejectInput {
    pub name: String,
    pub reason: String,
    #[serde(default)]
    pub source: String,
}

/// 返回给附件中心的记录(带会话标题)
#[derive(Debug, Clone, Serialize)]
pub struct RejectEntry {
    pub time: String,
    pub session_id: String,
    pub session_title: String,
    pub name: String,
    pub reason: String,
    pub source: String,
}

/// 列表结果:总数 + 最新若干条
#[derive(Debug, Clone, Serialize)]
pub struct RejectList {
    pub total: usize,
    pub items: Vec<RejectEntry>,
}

/// 最多保留的记录条数
const MAX_RECORDS: usize = 2000;
/// 单次最多写入的条目(防御一次 IPC 写入过多)
const MAX_APPEND: usize = 500;
/// 列表默认 / 上限
const DEFAULT_LIST: u32 = 200;
const MAX_LIST: u32 = 1000;
/// 字段截断长度(字符)
const MAX_NAME_CHARS: usize = 120;
const MAX_REASON_CHARS: usize = 240;

/// 同进程内的写入串行化(命令可能被并发调用)
static WRITE_LOCK: Mutex<()> = Mutex::new(());

fn rejects_path() -> PathBuf {
    crate::paths::data_dir().join("attachment_rejects.jsonl")
}

/// 清洗单行字段:换行折叠为空格并截断,避免一条记录跨行或撑爆文件。
fn sanitize_field(raw: &str, max_chars: usize) -> String {
    let s = raw
        .replace(['\r', '\n'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed: String = s.chars().take(max_chars).collect();
    if trimmed.is_empty() {
        "未知".into()
    } else {
        trimmed
    }
}

/// 解析 JSONL 文本(损坏行直接跳过,不让单行坏数据拖垮整份记录)。
fn parse_records(text: &str) -> Vec<StoredReject> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            serde_json::from_str::<StoredReject>(line).ok()
        })
        .collect()
}

/// 读取全部记录(文件缺失 = 空)。
fn read_records_at(path: &Path) -> Vec<StoredReject> {
    fs::read_to_string(path)
        .map(|t| parse_records(&t))
        .unwrap_or_default()
}

/// 追加记录并按上限裁剪(写入失败返回中文原因)。
fn append_records_at(
    path: &Path,
    records: Vec<StoredReject>,
    max: usize,
) -> Result<(), String> {
    let mut all = read_records_at(path);
    all.extend(records);
    if all.len() > max {
        let drop_n = all.len() - max;
        all.drain(0..drop_n);
    }
    let mut text = String::new();
    for r in &all {
        text.push_str(&serde_json::to_string(r).map_err(|e| e.to_string())?);
        text.push('\n');
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("创建拒绝记录目录失败: {e}"))?;
    }
    fs::write(path, text).map_err(|e| format!("写入拒绝记录失败: {e}"))
}

/// 按会话 id 批量取标题(取不到时返回空串)
fn session_titles(ids: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(conn) = crate::db::conn().lock() else {
        return map;
    };
    for id in ids {
        if let Ok(title) = conn.query_row(
            "SELECT COALESCE(title,'') FROM sessions WHERE id=?1",
            rusqlite::params![id],
            |r| r.get::<_, String>(0),
        ) {
            map.insert(id.clone(), title);
        }
    }
    map
}

/// 记录一次导入中被拒绝的文件(前端采集阶段与后端保存失败共用)。
#[tauri::command]
pub async fn record_attachment_rejects(
    session_id: String,
    items: Vec<RejectInput>,
) -> Result<(), String> {
    if !session_id.is_empty() {
        crate::memory::validate_id(&session_id)?;
    }
    if items.is_empty() {
        return Ok(());
    }
    let _guard = WRITE_LOCK.lock().map_err(|e| e.to_string())?;
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let records: Vec<StoredReject> = items
        .into_iter()
        .take(MAX_APPEND)
        .map(|item| StoredReject {
            time: now.clone(),
            session_id: session_id.clone(),
            name: sanitize_field(&item.name, MAX_NAME_CHARS),
            reason: sanitize_field(&item.reason, MAX_REASON_CHARS),
            source: sanitize_field(&item.source, 24),
        })
        .collect();
    append_records_at(&rejects_path(), records, MAX_RECORDS)
}

/// 列出最近被拒绝的文件(最新在前)。
#[tauri::command]
pub async fn list_attachment_rejects(limit: Option<u32>) -> Result<RejectList, String> {
    let limit = limit.unwrap_or(DEFAULT_LIST).clamp(1, MAX_LIST) as usize;
    let all = read_records_at(&rejects_path());
    let total = all.len();
    let items: Vec<StoredReject> = all.into_iter().rev().take(limit).collect();
    let ids: Vec<String> = items
        .iter()
        .map(|r| r.session_id.clone())
        .filter(|id| !id.is_empty())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let titles = session_titles(&ids);
    let items = items
        .into_iter()
        .map(|r| RejectEntry {
            session_title: titles.get(&r.session_id).cloned().unwrap_or_default(),
            time: r.time,
            session_id: r.session_id,
            name: r.name,
            reason: r.reason,
            source: r.source,
        })
        .collect();
    Ok(RejectList { total, items })
}

/// 清空拒绝记录。
#[tauri::command]
pub async fn clear_attachment_rejects() -> Result<(), String> {
    let _guard = WRITE_LOCK.lock().map_err(|e| e.to_string())?;
    let p = rejects_path();
    if p.exists() {
        fs::remove_file(&p).map_err(|e| format!("清空拒绝记录失败: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "maf-reject-test-{}-{}.jsonl",
            std::process::id(),
            tag
        ))
    }

    fn record(n: usize) -> StoredReject {
        StoredReject {
            time: "2026-11-01 12:00:00".into(),
            session_id: "s1".into(),
            name: format!("f{n}.exe"),
            reason: "为格式黑名单文件".into(),
            source: "import".into(),
        }
    }

    #[test]
    fn sanitize_field_flattens_and_truncates() {
        assert_eq!(sanitize_field("a\r\nb   c", 20), "a b c");
        assert_eq!(sanitize_field("   ", 20), "未知");
        let long = "x".repeat(300);
        assert_eq!(sanitize_field(&long, MAX_REASON_CHARS).chars().count(), MAX_REASON_CHARS);
    }

    #[test]
    fn parse_skips_malformed_lines() {
        let text = concat!(
            "{\"time\":\"t\",\"session_id\":\"s\",\"name\":\"a\",\"reason\":\"r\",\"source\":\"import\"}\n",
            "not json\n",
            "\n"
        );
        let list = parse_records(text);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "a");
    }

    #[test]
    fn append_trims_to_newest_records() {
        let p = temp_path("trim");
        let _ = fs::remove_file(&p);
        append_records_at(&p, (0..5).map(record).collect(), 3).unwrap();
        // 再追加 2 条,仍只保留最新 3 条
        append_records_at(&p, (5..7).map(record).collect(), 3).unwrap();
        let all = read_records_at(&p);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].name, "f4.exe");
        assert_eq!(all[1].name, "f5.exe");
        assert_eq!(all[2].name, "f6.exe");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_tolerates_missing_file_and_dir() {
        let p = temp_path("new");
        let _ = fs::remove_file(&p);
        assert!(append_records_at(&p, vec![record(1)], 10).is_ok());
        assert_eq!(read_records_at(&p).len(), 1);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_keeps_newest_records_at_capacity() {
        let p = temp_path("cap");
        let _ = fs::remove_file(&p);
        // 超过上限一次性写入:只保留最新 MAX_RECORDS 条
        append_records_at(
            &p,
            (0..MAX_RECORDS + 50).map(record).collect(),
            MAX_RECORDS,
        )
        .unwrap();
        let all = read_records_at(&p);
        assert_eq!(all.len(), MAX_RECORDS);
        assert_eq!(all[0].name, "f50.exe");
        assert_eq!(
            all[MAX_RECORDS - 1].name,
            format!("f{}.exe", MAX_RECORDS + 49)
        );
        let _ = fs::remove_file(&p);
    }
}
