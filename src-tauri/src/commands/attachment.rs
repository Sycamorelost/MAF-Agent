//! 附件相关 Tauri commands(迭代 D1.1)。
//!
//! 落盘约定:`data/attachments/{session_id}/{时间戳}_{安全文件名}`。
//! 文本类附件(docx/xlsx/pdf/txt/md/csv 等)保存时立即提取纯文本并分块入库;
//! 消息 JSON 只存元数据 + 摘要 + 最多 20,000 字符的正文摘录(全文在 `doc_chunks`),
//! 避免 200 页 PDF 把消息表撑大。

use serde::Serialize;
use std::path::PathBuf;

use crate::attachment::{self, AttachmentKind};
use base64::Engine as _;

/// 附件元数据(存 messages.attachments JSON 数组的元素)。
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct AttachmentMeta {
    /// 原始文件名
    pub name: String,
    /// 落盘绝对路径
    pub path: String,
    /// 类别:docx / xlsx / pdf / text / image / other
    pub kind: String,
    /// 字节数
    pub size: u64,
    /// 文本类附件的提取文本(已截断);图片/其他为 None
    pub extracted_text: Option<String>,
    /// 文档指纹(落盘路径 sha256 前 12 位),doc_search / doc_read 定位用(R3 C1)
    #[serde(default)]
    pub doc_id: Option<String>,
    /// 驻留块数(缺省 = 未驻留,前端回退为全文注入)
    #[serde(default)]
    pub chunk_count: Option<usize>,
    /// 200 字摘要(「文档卡」注入用)
    #[serde(default)]
    pub summary: Option<String>,
    /// 提取失败 / 无文本的原因(F7 失败显性化,前端标 ⚠ 提示)
    #[serde(default)]
    pub extract_note: Option<String>,
}

/// 提取正文 → 分块驻留(会话级)→ 返回消息层数据。
/// 返回 (消息用正文摘录, 失败原因, 块数, 摘要, doc_id)。图片/其他格式不提取文本。
/// catch_unwind 防止 pdf-extract / calamine 等解析 panic 导致进程 abort。
type ExtractResult = (
    Option<String>,
    Option<String>,
    Option<usize>,
    Option<String>,
    Option<String>,
);

fn extract_and_index(
    session_id: &str,
    name: &str,
    kind: AttachmentKind,
    path: &str,
) -> ExtractResult {
    let raw = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        attachment::extract_text(std::path::Path::new(path), kind)
    }))
    .ok()
    .and_then(|r| r.ok())
    .flatten();
    let (text, note) = match raw {
        Some(t) if !t.trim().is_empty() => (
            Some(attachment::clamp_text(&t, attachment::EXTRACT_BUDGET_CHARS)),
            None,
        ),
        Some(_) => (
            None,
            Some(if kind == AttachmentKind::Pdf {
                "未提取到文本:可能是扫描版 PDF(需 OCR / 视觉模型)".to_string()
            } else {
                "未提取到文本".to_string()
            }),
        ),
        None if matches!(kind, AttachmentKind::Image | AttachmentKind::Other) => (
            None,
            Some("该格式不做文本提取(图片随消息走视觉通道)".to_string()),
        ),
        None => (None, Some("文本提取失败或该格式无文本层".to_string())),
    };
    let Some(text) = text else {
        return (None, note, None, None, None);
    };
    // 摘要 / 分块入库 / 消息摘录全部交给统一的文档服务(见 doc::service)。
    let ingested = crate::db::conn()
        .lock()
        .map(|conn| crate::doc::service::ingest(&conn, session_id, name, kind.as_str(), &text))
        .unwrap_or_default();
    (
        ingested.excerpt,
        None,
        ingested.chunks,
        ingested.summary,
        ingested.doc_id,
    )
}

/// 附件单文件大小上限:20MB(保存与复用入口共用,前端另有同值预检)。
const MAX_ATTACHMENT_BYTES: u64 = 20 * 1024 * 1024;

/// 附件根目录:`<data_dir>/attachments`
fn attachments_root() -> PathBuf {
    crate::paths::data_dir().join("attachments")
}

fn attachments_dir(session_id: &str) -> PathBuf {
    attachments_root().join(session_id)
}

/// 校验路径必须位于 attachments 目录内(防越权读盘,OWASP Path Traversal / CWE-22)。
fn ensure_in_attachments_dir(p: &std::path::Path) -> Result<(), String> {
    let root = attachments_root()
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let canon = p.canonicalize().map_err(|e| format!("路径无效: {e}"))?;
    if canon.starts_with(&root) {
        Ok(())
    } else {
        Err("路径不在 attachments 目录内".into())
    }
}

/// 文件名安全化:只保留字母数字/点/下划线/中横线与 CJK,其余替换为下划线,最长 60 字符。
/// 额外把 ".." 序列替换为 "__",防御纵深(虽然有 ts 前缀,但避免任何穿越歧义)。
pub(crate) fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .take(60)
        .collect();
    let cleaned = cleaned.replace("..", "__");
    if cleaned.is_empty() {
        "file".into()
    } else {
        cleaned
    }
}

/// 保存附件:base64 → 落盘 → 分类 → 文本类立即提取文本。返回元数据(不含 base64)。
/// 改为 async 避免阻塞主线程(WebView2 大文件场景),catch_unwind 捕获解析 panic 防进程崩溃。
#[tauri::command]
pub async fn save_attachment(
    session_id: String,
    file_name: String,
    data_base64: String,
) -> Result<AttachmentMeta, String> {
    crate::memory::validate_id(&session_id)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("base64 解码失败: {e}"))?;
    if bytes.is_empty() {
        return Err("附件内容为空".into());
    }
    // 单文件 20MB 护栏
    if bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
        return Err("附件超过 20MB 上限".into());
    }

    let kind = attachment::classify(&file_name);
    let dir = attachments_dir(&session_id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建附件目录失败: {e}"))?;

    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S%3f");
    let safe = sanitize_name(&file_name);
    let path = dir.join(format!("{ts}_{safe}"));
    std::fs::write(&path, &bytes).map_err(|e| format!("写入附件失败: {e}"))?;

    // 文本类立即提取并分块驻留(失败不阻塞附件保存,仅无 extracted_text);
    // catch_unwind 防止 pdf-extract/calamine 等解析 panic 导致进程 abort
    let path_str = path.to_string_lossy().to_string();
    let (extracted_text, extract_note, chunk_count, summary, doc_id) =
        extract_and_index(&session_id, &file_name, kind, &path_str);

    Ok(AttachmentMeta {
        name: file_name,
        doc_id,
        path: path_str,
        kind: kind.as_str().to_string(),
        size: bytes.len() as u64,
        extracted_text,
        chunk_count,
        summary,
        extract_note,
    })
}

/// 只提取文本、不落盘:供「去 AI 味」工作台等页面把拖入的文件直接填进输入区。
/// 写临时文件后复用统一提取管线(编码/格式处理一致),用完即删。
#[tauri::command]
pub fn extract_text_preview(file_name: String, data_base64: String) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("base64 解码失败: {e}"))?;
    if bytes.is_empty() {
        return Err("文件内容为空".into());
    }
    if bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
        return Err("文件超过 20MB 上限".into());
    }
    let kind = attachment::classify(&file_name);
    if matches!(kind, AttachmentKind::Image | AttachmentKind::Other) {
        return Err("该格式不支持文本提取(请直接粘贴文本)".into());
    }
    let tmp = std::env::temp_dir().join(format!(
        "maf-preview-{}-{}",
        std::process::id(),
        sanitize_name(&file_name)
    ));
    std::fs::write(&tmp, &bytes).map_err(|e| format!("写入临时文件失败: {e}"))?;
    let raw = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        attachment::extract_text(&tmp, kind)
    }))
    .ok()
    .and_then(|r| r.ok())
    .flatten();
    let _ = std::fs::remove_file(&tmp);
    match raw {
        Some(t) if !t.trim().is_empty() => {
            Ok(attachment::clamp_text(&t, attachment::EXTRACT_BUDGET_CHARS)
                .trim()
                .to_string())
        }
        _ => Err(if kind == AttachmentKind::Pdf {
            "未提取到文本:可能是扫描版 PDF(需 OCR / 视觉模型)".into()
        } else {
            "文本提取失败或该格式无文本层".into()
        }),
    }
}

/// 读取附件字节并返回 data URL(前端渲染历史图片附件用)。
#[tauri::command]
pub fn read_attachment_data(path: String) -> Result<String, String> {
    let p = std::path::Path::new(&path);
    // 白名单校验:只允许读 attachments 目录内的文件(防任意文件读取)
    ensure_in_attachments_dir(p)?;
    let bytes = std::fs::read(p).map_err(|e| format!("读取附件失败: {e}"))?;
    let mime = match attachment::classify(&path) {
        AttachmentKind::Image => {
            let ext = path
                .rsplit('.')
                .next()
                .unwrap_or("png")
                .to_ascii_lowercase();
            match ext.as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "bmp" => "image/bmp",
                _ => "image/png",
            }
        }
        _ => "application/octet-stream",
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    ))
}

// ---------------- 附件管理中心(迭代 F7) ----------------

/// 附件中心条目:展开 messages.attachments JSON,联表 sessions 取标题。
#[derive(Debug, Clone, Serialize)]
pub struct AttachmentEntry {
    pub message_id: i64,
    pub session_id: String,
    pub session_title: String,
    /// 来源消息所在分支(前端定位时需先切换分支)
    pub branch_id: String,
    /// 来源消息创建时间
    pub created: String,
    pub meta: AttachmentMeta,
}

/// 聚合全部历史附件(按消息 id 倒序 = 最新在前)。可测试核心。
fn list_all_on(conn: &rusqlite::Connection) -> Result<Vec<AttachmentEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.session_id, COALESCE(s.title,''), m.branch_id, m.created, m.attachments
             FROM messages m JOIN sessions s ON s.id = m.session_id
             WHERE m.attachments IS NOT NULL AND m.attachments <> '[]'
             ORDER BY m.id DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (message_id, session_id, session_title, branch_id, created, raw) =
            row.map_err(|e| e.to_string())?;
        let metas: Vec<AttachmentMeta> = serde_json::from_str(&raw).unwrap_or_default();
        for meta in metas {
            out.push(AttachmentEntry {
                message_id,
                session_id: session_id.clone(),
                session_title: session_title.clone(),
                branch_id: branch_id.clone(),
                created: created.clone(),
                meta,
            });
        }
    }
    Ok(out)
}

/// 列出全部历史附件(附件中心数据源)。
#[tauri::command]
pub fn list_all_attachments() -> Result<Vec<AttachmentEntry>, String> {
    let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
    list_all_on(&conn)
}

/// 从落盘文件名剥离 `{ts}_` 前缀,还原展示名(落盘格式:{yyyyMMdd-HHmmssSSS}_{safe})。
fn display_name_from_path(p: &std::path::Path) -> String {
    let file = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    match file.split_once('_') {
        Some((prefix, rest))
            if !rest.is_empty()
                && prefix.len() >= 8
                && prefix.chars().all(|c| c.is_ascii_digit() || c == '-') =>
        {
            rest.to_string()
        }
        _ => file,
    }
}

/// 把附件中心已有文件再次引入某会话:复用磁盘文件(不复制),重新生成元数据。
/// 文件可能被外部改动过,文本类重新提取保证注入内容新鲜。
#[tauri::command]
pub fn insert_attachment_to_session(
    session_id: String,
    path: String,
) -> Result<AttachmentMeta, String> {
    crate::memory::validate_id(&session_id)?;
    let p = std::path::Path::new(&path);
    ensure_in_attachments_dir(p)?;
    let fs_meta = std::fs::metadata(p).map_err(|e| format!("附件不存在或不可读: {e}"))?;
    // H-3 复检:复用入口的文件可能在落盘后被外部替换为超大文件,再次施加 20MB 上限
    if fs_meta.len() > MAX_ATTACHMENT_BYTES {
        return Err("附件超过 20MB 上限".into());
    }
    let name = display_name_from_path(p);
    let kind = attachment::classify(&name);
    let path_str = p.to_string_lossy().to_string();
    let (extracted_text, extract_note, chunk_count, summary, doc_id) =
        extract_and_index(&session_id, &name, kind, &path_str);
    Ok(AttachmentMeta {
        doc_id,
        name,
        path: path_str,
        kind: kind.as_str().to_string(),
        size: fs_meta.len(),
        extracted_text,
        chunk_count,
        summary,
        extract_note,
    })
}

/// 从指定消息的 attachments JSON 中移除 path 条目,返回被移除的元数据。可测试核心。
fn remove_from_message_on(
    conn: &rusqlite::Connection,
    message_id: i64,
    path: &str,
) -> Result<Vec<AttachmentMeta>, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT attachments FROM messages WHERE id=?1",
            rusqlite::params![message_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("消息不存在: {e}"))?;
    let mut metas: Vec<AttachmentMeta> = raw
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    let mut removed = Vec::new();
    metas.retain(|m| {
        if m.path == path {
            removed.push(m.clone());
            false
        } else {
            true
        }
    });
    if removed.is_empty() {
        return Err("该消息中未找到此附件".into());
    }
    let new_json: Option<String> = if metas.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&metas).map_err(|e| e.to_string())?)
    };
    conn.execute(
        "UPDATE messages SET attachments=?1 WHERE id=?2",
        rusqlite::params![new_json, message_id],
    )
    .map_err(|e| format!("更新消息附件失败: {e}"))?;
    Ok(removed)
}

/// 同会话内仍在引用该 doc_id 的附件数(内容相同的重复上传会共享 doc_id)。
fn count_doc_id_refs_on(
    conn: &rusqlite::Connection,
    session_id: &str,
    doc_id: &str,
) -> Result<usize, String> {
    let mut stmt = conn
        .prepare(
            "SELECT attachments FROM messages
             WHERE session_id=?1 AND attachments IS NOT NULL AND attachments <> '[]'",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![session_id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut n = 0;
    for row in rows {
        let raw: String = row.map_err(|e| e.to_string())?;
        if let Ok(list) = serde_json::from_str::<Vec<AttachmentMeta>>(&raw) {
            n += list
                .iter()
                .filter(|m| m.doc_id.as_deref() == Some(doc_id))
                .count();
        }
    }
    Ok(n)
}

/// 删除附件:从来源消息 JSON 移除条目;全库无其他引用时一并删除磁盘文件,
/// 并级联清理该文档的检索块(R13:删附件后 doc_search 不应再返回旧内容)。
#[tauri::command]
pub fn delete_attachment(message_id: i64, path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    ensure_in_attachments_dir(p)?;
    let refs = {
        let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
        let removed = remove_from_message_on(&conn, message_id, &path)?;
        let session_id: String = conn
            .query_row(
                "SELECT session_id FROM messages WHERE id=?1",
                rusqlite::params![message_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let refs = count_references_on(&conn, &path)?;
        if refs == 0 {
            for meta in &removed {
                let Some(doc_id) = meta.doc_id.as_deref() else {
                    continue;
                };
                if count_doc_id_refs_on(&conn, &session_id, doc_id)? == 0 {
                    crate::doc::service::purge(&conn, &session_id, doc_id)?;
                }
            }
        }
        refs
    };
    // 无引用则清理磁盘(文件已不存在则忽略)
    if refs == 0 && p.exists() {
        std::fs::remove_file(p).map_err(|e| format!("删除磁盘文件失败: {e}"))?;
    }
    Ok(())
}

/// 全库精确统计同一文件路径的引用数(Rust 侧比对,避免 LIKE 通配符误伤)。可测试核心。
fn count_references_on(conn: &rusqlite::Connection, path: &str) -> Result<usize, String> {
    let mut stmt = conn
        .prepare("SELECT attachments FROM messages WHERE attachments IS NOT NULL AND attachments <> '[]'")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut n = 0;
    for row in rows {
        let raw: String = row.map_err(|e| e.to_string())?;
        if let Ok(list) = serde_json::from_str::<Vec<AttachmentMeta>>(&raw) {
            n += list.iter().filter(|m| m.path == path).count();
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_name_strips_unsafe_chars() {
        assert_eq!(sanitize_name("报告 2026/终版.docx"), "报告_2026_终版.docx");
        assert_eq!(sanitize_name("a<b>:c.pdf"), "a_b__c.pdf");
        // ".." 序列会被替换(防御纵深)
        assert_eq!(sanitize_name("..."), "__.");
        assert_eq!(sanitize_name("..\\evil"), "___evil");
        // 超长截断
        let long = format!("{}.txt", "x".repeat(100));
        assert!(sanitize_name(&long).len() <= 60);
    }

    #[test]
    fn sanitize_name_keeps_cjk() {
        assert_eq!(sanitize_name("会议纪要.docx"), "会议纪要.docx");
    }

    // ---------------- F7 附件中心测试 ----------------

    /// 内存库 + 两个会话(main 分支元数据)。
    fn setup_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        for (id, title) in [("s1", "项目讨论"), ("s2", "翻译记录")] {
            conn.execute(
                "INSERT INTO sessions(id, title, created, updated) VALUES(?1,?2,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
                rusqlite::params![id, title],
            )
            .unwrap();
            crate::db::branches::ensure_main_on(&conn, id).unwrap();
        }
        conn
    }

    fn meta_json(metas: &[AttachmentMeta]) -> String {
        serde_json::to_string(metas).unwrap()
    }

    fn fake_meta(name: &str, path: &str, kind: &str) -> AttachmentMeta {
        AttachmentMeta {
            name: name.into(),
            path: path.into(),
            kind: kind.into(),
            size: 42,
            extracted_text: None,
            doc_id: None,
            chunk_count: None,
            summary: None,
            extract_note: None,
        }
    }

    fn add_msg(conn: &rusqlite::Connection, session: &str, attachments: Option<&str>) -> i64 {
        crate::db::messages::append_on(
            conn,
            session,
            "user",
            "带附件的消息",
            None,
            0,
            attachments,
            None,
            None,
            "main",
        )
        .unwrap()
    }

    #[test]
    fn list_all_expands_json_and_joins_title() {
        let conn = setup_db();
        let m1 = add_msg(
            &conn,
            "s1",
            Some(&meta_json(&[
                fake_meta("a.png", r"D:\att\s1\a.png", "image"),
                fake_meta("b.txt", r"D:\att\s1\b.txt", "text"),
            ])),
        );
        let _m2 = add_msg(&conn, "s1", None);
        let m3 = add_msg(
            &conn,
            "s2",
            Some(&meta_json(&[fake_meta("c.pdf", r"D:\att\s2\c.pdf", "pdf")])),
        );

        let all = list_all_on(&conn).unwrap();
        assert_eq!(all.len(), 3, "两条消息共 3 个附件, got: {}", all.len());
        // 按消息 id 倒序:最新的 m3 在最前
        assert_eq!(all[0].message_id, m3);
        assert_eq!(all[0].session_title, "翻译记录");
        assert_eq!(all[0].meta.name, "c.pdf");
        assert_eq!(all[0].branch_id, "main");
        // m1 的两个附件保持 JSON 内顺序
        assert_eq!(all[1].message_id, m1);
        assert_eq!(all[1].session_title, "项目讨论");
        assert_eq!(all[1].meta.name, "a.png");
        assert_eq!(all[2].meta.name, "b.txt");
    }

    #[test]
    fn remove_from_message_updates_json() {
        let conn = setup_db();
        let m1 = add_msg(
            &conn,
            "s1",
            Some(&meta_json(&[
                fake_meta("a.png", r"D:\att\s1\a.png", "image"),
                fake_meta("b.txt", r"D:\att\s1\b.txt", "text"),
            ])),
        );

        // 移除一个 → 剩一个
        remove_from_message_on(&conn, m1, r"D:\att\s1\a.png").unwrap();
        let raw: String = conn
            .query_row("SELECT attachments FROM messages WHERE id=?1", [m1], |r| {
                r.get(0)
            })
            .unwrap();
        let left: Vec<AttachmentMeta> = serde_json::from_str(&raw).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].name, "b.txt");

        // 再移除 → attachments 变 NULL
        remove_from_message_on(&conn, m1, r"D:\att\s1\b.txt").unwrap();
        let raw: Option<String> = conn
            .query_row("SELECT attachments FROM messages WHERE id=?1", [m1], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(raw.is_none(), "清空后应为 NULL");

        // 路径不存在 / 消息不存在均报错
        assert!(remove_from_message_on(&conn, m1, r"D:\att\s1\ghost.png").is_err());
        assert!(remove_from_message_on(&conn, 99999, r"D:\att\s1\a.png").is_err());
    }

    #[test]
    fn count_references_exact_match() {
        let conn = setup_db();
        let shared = r"D:\att\shared\same.png";
        add_msg(
            &conn,
            "s1",
            Some(&meta_json(&[fake_meta("same.png", shared, "image")])),
        );
        add_msg(
            &conn,
            "s2",
            Some(&meta_json(&[
                fake_meta("same.png", shared, "image"),
                // 相似前缀路径不得误匹配(LIKE 会踩的坑)
                fake_meta("same2.png", r"D:\att\shared\same.png.bak", "image"),
            ])),
        );

        assert_eq!(count_references_on(&conn, shared).unwrap(), 2);
        assert_eq!(
            count_references_on(&conn, r"D:\att\shared\none.png").unwrap(),
            0
        );
    }

    #[test]
    fn remove_from_message_returns_removed_meta() {
        let conn = setup_db();
        let mut m = fake_meta("a.png", r"D:\att\s1\a.png", "image");
        m.doc_id = Some("abc123".into());
        let m1 = add_msg(&conn, "s1", Some(&meta_json(&[m])));

        let removed = remove_from_message_on(&conn, m1, r"D:\att\s1\a.png").unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].doc_id.as_deref(), Some("abc123"));
    }

    /// R13:同一内容(doc_id 相同)在多条消息中引用时,删除一条不应误清块。
    #[test]
    fn count_doc_id_refs_detects_shared_content() {
        let conn = setup_db();
        let mut a = fake_meta("a.pdf", r"D:\att\s1\a.pdf", "pdf");
        a.doc_id = Some("dup".into());
        let mut b = fake_meta("b.pdf", r"D:\att\s1\b.pdf", "pdf");
        b.doc_id = Some("dup".into());
        add_msg(&conn, "s1", Some(&meta_json(&[a])));
        add_msg(&conn, "s1", Some(&meta_json(&[b])));
        add_msg(
            &conn,
            "s2",
            Some(&meta_json(&[fake_meta("c.pdf", r"D:\att\s2\c.pdf", "pdf")])),
        );

        assert_eq!(count_doc_id_refs_on(&conn, "s1", "dup").unwrap(), 2);
        assert_eq!(count_doc_id_refs_on(&conn, "s2", "dup").unwrap(), 0);
    }

    #[test]
    fn display_name_strips_ts_prefix() {
        use std::path::Path;
        assert_eq!(
            display_name_from_path(Path::new(r"D:\att\s1\20261015-123000123_报告.docx")),
            "报告.docx"
        );
        // 名字里含下划线:只剥第一个 ts 段
        assert_eq!(
            display_name_from_path(Path::new("20261015-123000123_my_file.txt")),
            "my_file.txt"
        );
        // 无 ts 前缀(手工放入的文件):保留整体
        assert_eq!(display_name_from_path(Path::new("plain.txt")), "plain.txt");
        // 过短的类前缀不剥
        assert_eq!(
            display_name_from_path(Path::new("2026-10_a.txt")),
            "2026-10_a.txt"
        );
    }
}
