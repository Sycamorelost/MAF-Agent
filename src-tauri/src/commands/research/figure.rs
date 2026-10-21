//! 研究图示:文件名白名单、HTML 内嵌与 data URL 读取。

use crate::memory;
use crate::research::store;

/// 扫描 HTML 中的 `figures/{name}` 引用并替换为 base64 data URI(离线自包含)。
/// 名称白名单同 research_figure_data;未命中文件的引用原样保留。
pub(super) fn inline_figures(html: &str, figures_dir: &std::path::Path) -> String {
    const MARKER: &str = "figures/";
    let mut out = String::with_capacity(html.len() + 4096);
    let mut rest = html;
    while let Some(i) = rest.find(MARKER) {
        out.push_str(&rest[..i]);
        let after = &rest[i + MARKER.len()..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
            .collect();
        if is_valid_figure_name(&name) {
            let path = figures_dir.join(&name);
            if path.parent() == Some(figures_dir) {
                if let Ok(bytes) = std::fs::read(&path) {
                    let mime = figure_mime(&name).unwrap_or("application/octet-stream");
                    use base64::Engine as _;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    out.push_str(&format!("data:{mime};base64,{b64}"));
                    rest = &after[name.len()..];
                    continue;
                }
            }
        }
        out.push_str(MARKER);
        rest = after;
    }
    out.push_str(rest);
    out
}

/// 图示文件名白名单:仅字母数字-_ 与单个点,扩展名必须为图片格式,拒绝路径分隔与 .. 序列。
pub(super) fn is_valid_figure_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.contains("..")
        && !name.contains('/')
        && !name.contains('\\')
        && name.matches('.').count() == 1
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && figure_mime(name).is_some()
}

/// 图示 MIME(按字节嗅探为辅,扩展名为准;仅图片格式)。
pub(super) fn figure_mime(name: &str) -> Option<&'static str> {
    match name.rsplit('.').next()? {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// 读取研究图示为 data URL(报告内联预览;仅允许 figures 目录内的图片文件)。
#[tauri::command]
pub fn research_figure_data(id: String, name: String) -> Result<String, String> {
    memory::validate_id(&id)?;
    if !is_valid_figure_name(&name) {
        return Err("图示文件名非法".into());
    }
    let mime = figure_mime(&name).ok_or("不支持的图示格式")?;
    let dir = store::research_dir(&id).join("figures");
    let path = dir.join(&name);
    if path.parent() != Some(dir.as_path()) {
        return Err("图示路径非法".into());
    }
    let bytes = std::fs::read(&path).map_err(|_| "图示不存在")?;
    use base64::Engine as _;
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 图示文件名白名单:拒绝穿越/分隔/非法扩展。
    #[test]
    fn figure_name_whitelist() {
        assert!(is_valid_figure_name("draft-20260101-000000123.png"));
        assert!(is_valid_figure_name("a-b_c.png"));
        assert!(!is_valid_figure_name("..\\evil.png"));
        assert!(!is_valid_figure_name("a/b.png"));
        assert!(!is_valid_figure_name("a..b.png"));
        assert!(!is_valid_figure_name("a.exe"));
        assert!(!is_valid_figure_name(""));
        assert!(!is_valid_figure_name("a.png.exe"));
    }

    /// HTML 内嵌:figures 引用替换为 data URI,未命中原样保留。
    #[test]
    fn inline_figures_replaces_and_preserves() {
        let tmp = std::env::temp_dir().join("maf-inline-test");
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("t.png"), b"png").unwrap();
        let html = r#"<img src="figures/t.png"><a href="figures/missing.png">x</a>"#;
        let out = inline_figures(html, &tmp);
        assert!(out.starts_with(r#"<img src="data:image/png;base64,"#));
        assert!(out.contains(r#"href="figures/missing.png""#), "未命中引用应原样保留");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
