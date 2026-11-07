//! 附件元数据、落盘目录与路径/文件名安全校验、文本提取入库(迭代 D1.1 / F7)。

use std::path::PathBuf;

use serde::Serialize;

use crate::attachment::{self, AttachmentKind};

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
pub(crate) type ExtractResult = (
    Option<String>,
    Option<String>,
    Option<usize>,
    Option<String>,
    Option<String>,
);

pub(crate) fn extract_and_index(
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
pub(crate) const MAX_ATTACHMENT_BYTES: u64 = 20 * 1024 * 1024;

/// 附件根目录:`<data_dir>/attachments`
fn attachments_root() -> PathBuf {
    crate::paths::data_dir().join("attachments")
}

pub(super) fn attachments_dir(session_id: &str) -> PathBuf {
    attachments_root().join(session_id)
}

/// 校验路径必须位于 attachments 目录内(防越权读盘,OWASP Path Traversal / CWE-22)。
/// 文件已被外部删除时回退校验父目录:仍允许清理残留的数据库条目。
pub(crate) fn ensure_in_attachments_dir(p: &std::path::Path) -> Result<(), String> {
    let root = attachments_root()
        .canonicalize()
        .map_err(|e| e.to_string())?;
    match p.canonicalize() {
        Ok(canon) if canon.starts_with(&root) => Ok(()),
        Ok(_) => Err("路径不在 attachments 目录内".into()),
        Err(_) => {
            let parent = p.parent().ok_or_else(|| "路径无效".to_string())?;
            let canon_parent = parent
                .canonicalize()
                .map_err(|e| format!("路径无效: {e}"))?;
            if !canon_parent.starts_with(&root) {
                return Err("路径不在 attachments 目录内".into());
            }
            match p.file_name().and_then(|s| s.to_str()) {
                Some(n) if n != "." && n != ".." => Ok(()),
                _ => Err("路径无效".into()),
            }
        }
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
