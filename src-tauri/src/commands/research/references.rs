//! 研究参考资料导入、列表与删除。

use serde::Serialize;

use crate::research;

/// 单份参考资料原始文件上限(字节):超限直接拒绝,避免落盘超大文件。
const REF_MAX_BYTES: usize = 32 * 1024 * 1024;
/// 参考资料提取文本缓存上限(字符):远大于注入上限,既保留可截取余量又不占满磁盘。
const REF_CACHE_CHARS: usize = 120_000;

/// 前端展示用的参考资料元数据(不返回正文)。
#[derive(Debug, Clone, Serialize)]
pub struct ResearchReferenceInfo {
    /// 落盘后的安全文件名(与 sanitize_name 结果一致,作为 ResearchParams.reference_files 的取值)
    pub name: String,
    /// 格式分类:docx / xlsx / pdf / text
    pub kind: String,
    /// 提取到的字符数
    pub chars: usize,
    /// 前 200 字预览
    pub preview: String,
}

/// 导入一份研究参考资料:base64 → 白名单校验 → 落盘 data/research/references/ →
/// 立即提取文本并缓存到 references/extracted/(研究启动时直接读缓存,不再解析大文件)。
/// 仅支持 docx/xlsx/xls/pdf/文本类;图片与其它格式不支持(无 OCR)。
#[tauri::command]
pub fn import_research_reference(
    file_name: String,
    data_base64: String,
) -> Result<ResearchReferenceInfo, String> {
    let name = crate::commands::attachment::sanitize_name(&file_name);
    let kind = crate::attachment::classify(&name);
    use crate::attachment::AttachmentKind;
    let kind_label = match kind {
        AttachmentKind::Docx => "docx",
        AttachmentKind::Xlsx => "xlsx",
        AttachmentKind::Pdf => "pdf",
        AttachmentKind::Text => "text",
        AttachmentKind::Image => {
            return Err("图片需 OCR 才能作为参考资料，当前版本不支持；请改用文本类文件".into());
        }
        AttachmentKind::Other => {
            return Err("不支持的参考资料格式（仅支持 docx/xlsx/xls/pdf/txt/md/csv 等）".into());
        }
    };

    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("base64 解码失败: {e}"))?;
    if bytes.is_empty() {
        return Err("文件内容为空".into());
    }
    if bytes.len() > REF_MAX_BYTES {
        return Err(format!(
            "文件超出 {}MB 上限",
            REF_MAX_BYTES / 1024 / 1024
        ));
    }

    let dir = research::references_dir();
    let cache = research::references_cache_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建参考资料目录失败: {e}"))?;
    std::fs::create_dir_all(&cache).map_err(|e| format!("创建缓存目录失败: {e}"))?;
    let path = dir.join(&name);
    std::fs::write(&path, &bytes).map_err(|e| format!("写入参考资料失败: {e}"))?;

    // catch_unwind 兜底:pdf-extract / calamine 对畸形文件可能 panic,不得拖垮进程
    let text = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::attachment::extract_text(&path, kind)
    }))
    .ok()
    .and_then(Result::ok)
    .flatten()
    .ok_or_else(|| "文本提取失败（文件可能损坏或加密）".to_string())?;
    // 缓存提取文本(供研究启动时零解析读取);缓存本身限长,避免单文件占满磁盘
    let text = crate::attachment::clamp_text(&text, REF_CACHE_CHARS);
    let _ = std::fs::write(cache.join(format!("{name}.txt")), &text);

    let chars = text.chars().count();
    let preview: String = text.chars().take(200).collect();
    Ok(ResearchReferenceInfo {
        name,
        kind: kind_label.into(),
        chars,
        preview,
    })
}

/// 列出已导入的研究参考资料(扫描 references 目录;字符数取提取缓存长度,缺缓存记 0)。
#[tauri::command]
pub fn list_research_references() -> Result<Vec<ResearchReferenceInfo>, String> {
    let dir = research::references_dir();
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(Vec::new()),
    };
    let cache = research::references_cache_dir();
    let mut out: Vec<ResearchReferenceInfo> = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let Some(name) = p.file_name().and_then(|s| s.to_str()).map(str::to_string) else {
            continue;
        };
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
            continue;
        }
        let (chars, preview) = match std::fs::read_to_string(cache.join(format!("{name}.txt"))) {
            Ok(t) => {
                let n = t.chars().count();
                (n, t.chars().take(200).collect::<String>())
            }
            Err(_) => (0, String::new()),
        };
        out.push(ResearchReferenceInfo {
            kind: classify_label(&name).into(),
            name,
            chars,
            preview,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// 参考资料格式标签(与 import 时的分类文案一致)。
fn classify_label(name: &str) -> &'static str {
    use crate::attachment::AttachmentKind;
    match crate::attachment::classify(name) {
        AttachmentKind::Docx => "docx",
        AttachmentKind::Xlsx => "xlsx",
        AttachmentKind::Pdf => "pdf",
        _ => "text",
    }
}

/// 删除一份已导入的研究参考资料(原始文件 + 提取缓存)。
#[tauri::command]
pub fn delete_research_reference(name: String) -> Result<(), String> {
    // 与 references_block 同款白名单:必须是纯文件名,禁路径分隔与上跳
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("非法文件名".into());
    }
    let dir = research::references_dir();
    let root = dir.canonicalize().map_err(|_| "参考资料目录不存在".to_string())?;
    let target = dir.join(&name);
    if let Ok(canon) = target.canonicalize() {
        if !canon.starts_with(&root) {
            return Err("非法路径".into());
        }
        std::fs::remove_file(&canon).map_err(|e| format!("删除失败: {e}"))?;
    }
    let _ = std::fs::remove_file(research::references_cache_dir().join(format!("{name}.txt")));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 参考资料文件名白名单:路径分隔符与上跳序列必须被拒绝(与 references_block 同款规则)。
    #[test]
    fn reference_name_whitelist() {
        for bad in ["../x", "a/b.txt", "a\\b.txt", "..", "", "a..b"] {
            assert!(
                bad.is_empty()
                    || bad.contains('/')
                    || bad.contains('\\')
                    || bad.contains(".."),
                "{bad} 应命中拒绝分支"
            );
        }
        // 落盘名经 sanitize_name 处理后必然不含分隔符/上跳
        for raw in ["报告 2026/终版.docx", "..\\evil.pdf", "a<b>:c.txt"] {
            let n = crate::commands::attachment::sanitize_name(raw);
            assert!(!n.contains('/') && !n.contains('\\') && !n.contains(".."));
        }
        // 删除接口对非法名直接报错
        assert!(delete_research_reference("../x".into()).is_err());
        assert!(delete_research_reference("a/b".into()).is_err());
        assert!(delete_research_reference(String::new()).is_err());
    }
}
