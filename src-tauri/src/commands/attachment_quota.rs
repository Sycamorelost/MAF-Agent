//! 附件会话配额:统计单会话去重后的附件用量,并在保存/复用前做权威校验。
//!
//! 用量按「磁盘路径」去重(同一文件被多次复用不会重复计入),容量按元数据 size 求和。
//! 配额上限来自设置(`attachment_session_max_files` / `attachment_session_max_mb`),
//! 每次写入前读取,改设置后即时生效。

use std::collections::HashSet;

use crate::commands::attachment::AttachmentMeta;
use crate::settings::{AttachmentQuota, MAX_ATTACHMENT_SESSION_MAX_MB};

/// 单会话附件用量(去重)。
#[derive(Debug, Clone, Default)]
pub struct SessionUsage {
    /// 去重后的文件数
    pub files: usize,
    /// 去重后的总字节数
    pub bytes: u64,
    /// 已计入用量(同会话引用过)的路径集合
    pub paths: HashSet<String>,
}

/// 统计某会话的附件用量(按路径去重)。可测试核心。
pub(crate) fn session_usage_on(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> Result<SessionUsage, String> {
    let mut stmt = conn
        .prepare(
            "SELECT attachments FROM messages
             WHERE session_id=?1 AND attachments IS NOT NULL AND attachments <> '[]'",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![session_id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut usage = SessionUsage::default();
    for row in rows {
        let raw: String = row.map_err(|e| e.to_string())?;
        let list: Vec<AttachmentMeta> = serde_json::from_str(&raw).unwrap_or_default();
        for meta in list {
            if usage.paths.insert(meta.path.clone()) {
                usage.files += 1;
                usage.bytes = usage.bytes.saturating_add(meta.size);
            }
        }
    }
    Ok(usage)
}

/// 纯校验:给定当前用量、目标路径(Some = 复用已有文件,None = 新增)与字节数,
/// 判断是否仍可加入。返回中文拒绝原因。
/// 复用已在会话中的同一路径不会增加用量,直接放行(兼容历史超配额数据)。
pub(crate) fn quota_violation(
    usage: &SessionUsage,
    path: Option<&str>,
    size: u64,
    quota: AttachmentQuota,
) -> Option<String> {
    if let Some(p) = path {
        if usage.paths.contains(p) {
            return None;
        }
    }
    if usage.files + 1 > quota.max_files {
        return Some(format!(
            "本会话附件数量已达上限 {} 个,请先在附件中心清理",
            quota.max_files
        ));
    }
    if usage.bytes.saturating_add(size) > quota.max_bytes {
        return Some(format!(
            "本会话附件容量将超过上限 {}MB(已用 {:.1}MB),请先在附件中心清理",
            quota.max_mb(),
            usage.bytes as f64 / (1024.0 * 1024.0)
        ));
    }
    None
}

/// 读取设置中的当前配额(供前端展示与后端校验共用同一口径)。
pub(crate) fn current_quota() -> AttachmentQuota {
    crate::settings::attachment_quota()
}

/// 保存/复用前的权威校验:超配额直接拒绝。
pub(crate) fn check_quota_on(
    conn: &rusqlite::Connection,
    session_id: &str,
    path: Option<&str>,
    size: u64,
) -> Result<(), String> {
    // 防御异常值:单个文件本身也不应超过容量配额的展示范围
    let quota = current_quota();
    if size > MAX_ATTACHMENT_SESSION_MAX_MB as u64 * 1024 * 1024 {
        return Err("附件容量超出配额可表达范围".into());
    }
    let usage = session_usage_on(conn, session_id)?;
    match quota_violation(&usage, path, size, quota) {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AttachmentQuota;

    fn setup_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, title, created, updated) VALUES('s1','配额测试','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        crate::db::branches::ensure_main_on(&conn, "s1").unwrap();
        conn
    }

    fn meta(path: &str, size: u64) -> AttachmentMeta {
        AttachmentMeta {
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            path: path.into(),
            kind: "text".into(),
            size,
            extracted_text: None,
            doc_id: None,
            chunk_count: None,
            summary: None,
            extract_note: None,
        }
    }

    fn add_msg(conn: &rusqlite::Connection, metas: &[AttachmentMeta]) {
        crate::db::messages::append_on(
            conn,
            "s1",
            "user",
            "附件消息",
            None,
            0,
            Some(&serde_json::to_string(metas).unwrap()),
            None,
            None,
            "main",
        )
        .unwrap();
    }

    #[test]
    fn usage_dedupes_same_path() {
        let conn = setup_db();
        let same = meta(r"D:\att\s1\a.png", 10);
        add_msg(&conn, &[same.clone()]);
        // 同路径再次复用:不应重复计数
        add_msg(&conn, &[same]);
        add_msg(&conn, &[meta(r"D:\att\s1\b.txt", 30)]);
        let usage = session_usage_on(&conn, "s1").unwrap();
        assert_eq!(usage.files, 2);
        assert_eq!(usage.bytes, 40);
        assert!(usage.paths.contains(r"D:\att\s1\a.png"));
        // 其他会话不串味
        add_msg(&conn, &[]);
        assert_eq!(session_usage_on(&conn, "s2").unwrap().files, 0);
    }

    #[test]
    fn violation_reports_count_and_capacity() {
        let mut usage = SessionUsage::default();
        usage.files = 2;
        usage.bytes = 3;
        usage.paths.insert(r"D:\att\s1\a.png".into());
        let quota = AttachmentQuota {
            max_files: 3,
            max_bytes: 8 * 1024 * 1024,
        };

        // 第三个文件放行;第四个被数量拦截
        assert!(quota_violation(&usage, None, 1, quota).is_none());
        usage.files = 3;
        let err = quota_violation(&usage, None, 1, quota).unwrap();
        assert!(err.contains("数量已达上限 3 个"), "{err}");

        // 同路径复用不占新名额
        assert!(quota_violation(&usage, Some(r"D:\att\s1\a.png"), 99, quota).is_none());

        // 容量超限(7MB + 2MB > 8MB)
        usage.files = 1;
        usage.bytes = 7 * 1024 * 1024;
        let err = quota_violation(&usage, None, 2 * 1024 * 1024, quota).unwrap();
        assert!(err.contains("容量将超过上限 8MB"), "{err}");
        assert!(quota_violation(&usage, None, 1024 * 1024, quota).is_none());
    }

    #[test]
    fn violation_tolerates_zero_quota_usage() {
        let quota = AttachmentQuota {
            max_files: 1,
            max_bytes: 1024,
        };
        let empty = SessionUsage::default();
        assert!(quota_violation(&empty, None, 512, quota).is_none());
    }

    #[test]
    fn usage_skips_malformed_attachments_json() {
        let conn = setup_db();
        // 历史坏数据(截断的 JSON)不应让用量统计整体报错
        crate::db::messages::append_on(
            &conn,
            "s1",
            "user",
            "坏数据",
            None,
            0,
            Some("[{\"name\":"),
            None,
            None,
            "main",
        )
        .unwrap();
        add_msg(&conn, &[meta(r"D:\att\s1\a.png", 10)]);
        let usage = session_usage_on(&conn, "s1").unwrap();
        assert_eq!(usage.files, 1);
        assert_eq!(usage.bytes, 10);
    }
}
