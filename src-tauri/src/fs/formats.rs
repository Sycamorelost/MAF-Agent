//! 全格式文本提取与压缩包解包(均限定在工作区内)。
//!
//! 覆盖:
//! - 办公:docx / xlsx / pdf 复用 `attachment` 的解析器;pptx 抽取幻灯片文本;
//! - 网页/电子书:html / htm / epub 去标签取正文;
//! - 压缩包:zip 列表 + 安全解包(enclosed_name 防 zip-slip + 体积上限)。

use std::io::Read;
use std::path::Path;

use crate::attachment::{AttachmentKind, classify, extract_text};
use crate::fs::paths;

/// 文本输出上限(字符)。
const MAX_CHARS: usize = 200_000;
/// 单文件解包上限 20 MB,总上限 100 MB。
const MAX_ENTRY_BYTES: u64 = 20 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ZipEntry {
    pub name: String,
    pub is_dir: bool,
    pub bytes: u64,
}

/// 从工作区文件提取纯文本(按扩展名分派)。
pub fn extract(rel: &str) -> Result<String, String> {
    extract_in(&paths::workspace_root()?, rel)
}

/// 列出 zip 内容。
pub fn zip_list(rel: &str) -> Result<Vec<ZipEntry>, String> {
    zip_list_in(&paths::workspace_root()?, rel)
}

/// 解包 zip 到工作区目录,返回解包文件数。
pub fn zip_extract(rel: &str, dest: &str) -> Result<usize, String> {
    zip_extract_in(&paths::workspace_root()?, rel, dest)
}

// ---------------- 内核 ----------------

pub fn extract_in(root: &Path, rel: &str) -> Result<String, String> {
    let path = paths::resolve(root, rel, true)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "文件名无法识别".to_string())?;
    let ext = ext_of(name);
    let text = match ext.as_str() {
        "pptx" => pptx_text(&path)?,
        "epub" => epub_text(&path)?,
        "html" | "htm" => strip_tags(&read_lossy(&path)?),
        "zip" => return Err("zip 不能直接读取文本,请使用 zip_list / zip_extract".into()),
        _ => match classify(name) {
            AttachmentKind::Image => return Err("图片无法直接转文本(OCR 能力尚未接入)".into()),
            AttachmentKind::Other => return Err(format!("暂不支持 `{ext}` 格式的文本提取")),
            kind => extract_text(&path, kind)?.unwrap_or_default(),
        },
    };
    Ok(crate::mcp::safety::truncate_chars(text.trim(), MAX_CHARS))
}

fn zip_list_in(root: &Path, rel: &str) -> Result<Vec<ZipEntry>, String> {
    let path = paths::resolve(root, rel, true)?;
    let file = std::fs::File::open(&path).map_err(|e| format!("打开压缩包失败: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("解析 zip 失败: {e}"))?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| format!("读取条目失败: {e}"))?;
        out.push(ZipEntry {
            name: entry
                .name()
                .map_err(|e| format!("读取条目名失败: {e}"))?
                .to_string(),
            is_dir: entry.is_dir(),
            bytes: entry.size(),
        });
    }
    Ok(out)
}

fn zip_extract_in(root: &Path, rel: &str, dest: &str) -> Result<usize, String> {
    let path = paths::resolve(root, rel, true)?;
    let dest_path = paths::resolve(root, dest, false)?;
    std::fs::create_dir_all(&dest_path).map_err(|e| format!("创建目标目录失败: {e}"))?;

    let file = std::fs::File::open(&path).map_err(|e| format!("打开压缩包失败: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("解析 zip 失败: {e}"))?;
    let mut total = 0u64;
    let mut count = 0usize;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取条目失败: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        // zip-slip 防御:enclosed_name 拒绝绝对路径与 `..`
        let Some(rel_name) = entry.enclosed_name() else {
            continue;
        };
        // 跳过符号链接(Unix mode 高 4 位为 0o120000)
        if let Some(mode) = entry.unix_mode()
            && mode & 0o170000 == 0o120000
        {
            continue;
        }
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(format!("`{}` 超过单文件上限(20 MB)", rel_name.display()));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_TOTAL_BYTES {
            return Err("解包总量超过 100 MB,已中止".into());
        }
        let target = dest_path.join(&rel_name);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| format!("读取 `{}` 失败: {e}", rel_name.display()))?;
        std::fs::write(&target, &buf)
            .map_err(|e| format!("写入 `{}` 失败: {e}", rel_name.display()))?;
        count += 1;
    }
    Ok(count)
}

// ---------------- 格式辅助 ----------------

fn ext_of(name: &str) -> String {
    name.rsplit('.').next().unwrap_or("").to_ascii_lowercase()
}

fn read_lossy(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取失败: {e}"))?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// pptx:抽取所有 `ppt/slides/slide*.xml` 中的 `<a:t>` 文本,按页输出。
fn pptx_text(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("打开 pptx 失败: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("解析 pptx 失败: {e}"))?;
    let mut slides: Vec<(String, String)> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取条目失败: {e}"))?;
        let name = entry
            .name()
            .map_err(|e| format!("读取条目名失败: {e}"))?
            .to_string();
        if !(name.starts_with("ppt/slides/slide") && name.ends_with(".xml")) {
            continue;
        }
        let mut xml = String::new();
        entry
            .read_to_string(&mut xml)
            .map_err(|e| format!("读取 {name} 失败: {e}"))?;
        let body = tags_text(&xml, "a:t").join(" ");
        if !body.trim().is_empty() {
            slides.push((name, body));
        }
    }
    slides.sort_by(|a, b| a.0.cmp(&b.0));
    let out = slides
        .iter()
        .enumerate()
        .map(|(i, (_, body))| format!("## 第 {} 页\n{}", i + 1, body))
        .collect::<Vec<_>>()
        .join("\n\n");
    if out.is_empty() {
        return Err("pptx 中没有可提取的文本".into());
    }
    Ok(out)
}

/// epub:拼接所有 xhtml/html 条目的正文。
fn epub_text(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("打开 epub 失败: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("解析 epub 失败: {e}"))?;
    let mut parts = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取条目失败: {e}"))?;
        let entry_name = entry
            .name()
            .map_err(|e| format!("读取条目名失败: {e}"))?
            .to_string();
        let name = entry_name.to_ascii_lowercase();
        if !(name.ends_with(".xhtml") || name.ends_with(".html") || name.ends_with(".htm")) {
            continue;
        }
        let mut html = String::new();
        entry
            .read_to_string(&mut html)
            .map_err(|e| format!("读取 {entry_name} 失败: {e}"))?;
        let text = strip_tags(&html);
        if !text.trim().is_empty() {
            parts.push(text.trim().to_string());
        }
    }
    if parts.is_empty() {
        return Err("epub 中没有可提取的正文".into());
    }
    Ok(parts.join("\n\n"))
}

/// 提取指定标签内的全部文本。
fn tags_text(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    xml.split(&open)
        .skip(1)
        .filter_map(|seg| seg.split(&close).next())
        .map(unescape)
        .collect()
}

/// 简单去标签:丢弃 `<...>`,压缩空白。
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let lower = html.to_ascii_lowercase();
    let mut i = 0usize;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        let c = html[i..].chars().next().unwrap_or(' ');
        if !in_tag && c == '<' {
            // 跳过整段 script/style
            for tag in ["script", "style"] {
                let open = format!("<{tag}");
                if lower[i..].starts_with(&open) {
                    let close = format!("</{tag}>");
                    if let Some(end) = lower[i..].find(&close) {
                        i += end + close.len();
                        in_script = true;
                        break;
                    }
                }
            }
            if in_script {
                in_script = false;
                continue;
            }
            in_tag = true;
            i += 1;
            continue;
        }
        if in_tag {
            if c == '>' {
                in_tag = false;
            }
            i += c.len_utf8();
            continue;
        }
        out.push(c);
        i += c.len_utf8();
    }
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    unescape(&collapsed)
}

fn unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ws() -> PathBuf {
        let p = std::env::temp_dir().join(format!("maf-fmt-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn make_zip(path: &Path, files: &[(&str, &str)]) {
        use std::io::Write;
        let file = std::fs::File::create(path).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in files {
            zw.start_file(*name, opts).unwrap();
            zw.write_all(body.as_bytes()).unwrap();
        }
        zw.finish().unwrap();
    }

    #[test]
    fn strips_html_and_scripts() {
        let html = "<html><head><style>.a{color:red}</style></head><body><h1>标题</h1><p>正文 &amp; 说明</p><script>alert(1)</script></body></html>";
        let text = strip_tags(html);
        assert!(text.contains("标题"));
        assert!(text.contains("正文 & 说明"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("color:red"));
    }

    #[test]
    fn extracts_pptx_slide_text() {
        let root = ws();
        let path = root.join("deck.pptx");
        make_zip(
            &path,
            &[(
                "ppt/slides/slide1.xml",
                "<p:sld><a:p><a:r><a:t>第一页标题</a:t></a:r></a:p></p:sld>",
            )],
        );
        let text = extract_in(&root, "deck.pptx").unwrap();
        assert!(text.contains("第 1 页"));
        assert!(text.contains("第一页标题"));
    }

    #[test]
    fn zip_extract_blocks_traversal() {
        let root = ws();
        let path = root.join("a.zip");
        make_zip(&path, &[("../evil.txt", "x"), ("ok.txt", "hello")]);
        let n = zip_extract_in(&root, "a.zip", "out").unwrap();
        assert_eq!(n, 1, "越界条目应被跳过");
        assert!(root.join("out/ok.txt").exists());
        assert!(!root.join("out/../evil.txt").exists());
    }
}
