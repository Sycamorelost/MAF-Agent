//! 附件保存 / 文本预览 / 字节读取命令。

use base64::Engine as _;

use crate::attachment::{self, AttachmentKind};

use super::meta::{
    AttachmentMeta, MAX_ATTACHMENT_BYTES, attachments_dir, ensure_in_attachments_dir,
    extract_and_index, sanitize_name,
};

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
    // 权威安全闸:扩展名黑名单 + 可执行魔数(前端预检可被绕过,以此为准)
    let blocked = crate::settings::load().attachment_blocked_ext;
    crate::attachment_guard::check_attachment_allowed(&file_name, &bytes, &blocked)?;
    // 会话配额:数量 / 容量(按路径去重)超限直接拒绝
    // 放在安全闸之后,保证「格式被拒」的原因优先于配额提示
    {
        let conn = crate::db::conn().lock().map_err(|e| e.to_string())?;
        crate::commands::attachment_quota::check_quota_on(
            &conn,
            &session_id,
            None,
            bytes.len() as u64,
        )?;
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
pub async fn extract_text_preview(
    file_name: String,
    data_base64: String,
) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("base64 解码失败: {e}"))?;
    if bytes.is_empty() {
        return Err("文件内容为空".into());
    }
    if bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
        return Err("文件超过 20MB 上限".into());
    }
    // 与 save_attachment 同一套安全闸,防止预览入口被用来读可执行文件
    let blocked = crate::settings::load().attachment_blocked_ext;
    crate::attachment_guard::check_attachment_allowed(&file_name, &bytes, &blocked)?;
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
pub async fn read_attachment_data(path: String) -> Result<String, String> {
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
