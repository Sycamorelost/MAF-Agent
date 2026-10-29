//! 文件读写与办公格式导出(绝对路径可直达本机任意位置,相对路径以工作区为基准)。
//!
//! 所有写操作写前自动备份(可回滚),路径经 `fs::paths` 解析(位置与文件名均不设限)。
//! 导出支持 docx / xlsx,供 Agent 产出可直接交付的文件。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::fs::{backup, docgen, paths};

/// 单次读取默认上限(字符)。大文件请用 offset 分页,避免一次拉满上下文。
pub const MAX_READ_CHARS: usize = 32_000;
/// 硬上限:模型显式传入更大的 max_chars 也会被压到这里(约 2.5 万 token)。
pub const HARD_MAX_READ_CHARS: usize = 50_000;
/// 列表返回上限。
const MAX_LIST_ENTRIES: usize = 500;

#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub rel: String,
    pub is_dir: bool,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteResult {
    pub rel: String,
    pub bytes: u64,
    /// 覆盖已有文件时生成的备份 id(新建文件为 null)
    pub backup_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SheetInput {
    pub name: String,
    pub rows: Vec<Vec<String>>,
}

// ---------------- 对外接口(使用默认工作区) ----------------

pub fn root() -> Result<PathBuf, String> {
    paths::workspace_root()
}

pub fn list(rel: &str) -> Result<Vec<FileEntry>, String> {
    list_in(&paths::workspace_root()?, rel.trim())
}

pub fn read_text(
    rel: &str,
    offset: Option<usize>,
    max_chars: Option<usize>,
) -> Result<String, String> {
    read_text_in(
        &paths::workspace_root()?,
        rel,
        offset.unwrap_or(0),
        max_chars.unwrap_or(MAX_READ_CHARS),
    )
}

pub fn write_text(rel: &str, content: &str) -> Result<WriteResult, String> {
    write_text_in(&root()?, rel, content, &backup::backup_dir())
}

pub fn delete(rel: &str) -> Result<WriteResult, String> {
    delete_in(&root()?, rel, &backup::backup_dir())
}

/// 完整文档导出:用途模板、目录、封面元信息均可指定。
pub fn export_docx_ext(rel: &str, req: &docgen::DocRequest) -> Result<WriteResult, String> {
    export_docx_req_in(&root()?, rel, req, &backup::backup_dir())
}

pub fn export_xlsx(rel: &str, sheets: &[SheetInput]) -> Result<WriteResult, String> {
    export_xlsx_in(&root()?, rel, sheets, &backup::backup_dir())
}

// ---------------- 内核(显式传入工作区根,便于测试) ----------------

fn list_in(root: &Path, rel: &str) -> Result<Vec<FileEntry>, String> {
    let dir = if rel.trim().is_empty() {
        root.to_path_buf()
    } else {
        paths::resolve(root, rel, true)?
    };
    if !dir.is_dir() {
        return Err(format!("`{rel}` 不是目录"));
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)
        .map_err(|e| format!("读取目录失败: {e}"))?
        .flatten()
    {
        let path = entry.path();
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let meta = entry.metadata().ok();
        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        out.push(FileEntry {
            name,
            // 工作区外的绝对路径没有相对形式,直接回显绝对路径
            rel: paths::rel_of(root, &path).unwrap_or_else(|_| display_path(&path)),
            is_dir,
            bytes: meta.map(|m| m.len()).unwrap_or(0),
        });
        if out.len() >= MAX_LIST_ENTRIES {
            break;
        }
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

/// 去掉 canonicalize 产生的 `\\?\` 前缀,便于回显与展示。
fn display_path(p: &Path) -> String {
    let s = p.display().to_string();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

fn read_text_in(root: &Path, rel: &str, offset: usize, max_chars: usize) -> Result<String, String> {
    let path = paths::resolve(root, rel, true)?;
    if !path.is_file() {
        return Err(format!("`{rel}` 不是文件"));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("读取失败: {e}"))?;
    let probe = &bytes[..bytes.len().min(8192)];
    if probe.contains(&0) {
        return Err(format!(
            "`{rel}` 看起来是二进制文件,请使用专用导出/解析工具"
        ));
    }
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    // C3/F6:支持按字符偏移分页读取(大文件不必一次读完);硬上限防一次拉爆上下文。
    let max_chars = max_chars.clamp(1, HARD_MAX_READ_CHARS);
    let skipped: String = text.chars().skip(offset).collect();
    let total = skipped.chars().count();
    let body = crate::mcp::safety::truncate_chars(&skipped, max_chars);
    let mut out = if offset == 0 {
        body
    } else {
        format!("…(已跳过前 {offset} 字符)\n{body}")
    };
    if total > max_chars {
        out.push_str(&format!(
            "\n…(本段已截断:{max_chars} 字符;剩余 {} 字符,继续请用 offset={})",
            total - max_chars,
            offset + max_chars
        ));
    }
    Ok(out)
}

/// 文本检索(仅文本类扩展名,单文件上限 2MB),返回「文件:行号:内容」。
/// 绝对路径直接检索;相对路径以工作区为基准。
pub fn grep(pattern: &str, rel: &str, limit: usize) -> Result<Vec<String>, String> {
    grep_in(&paths::workspace_root()?, pattern, rel, limit)
}

fn grep_in(root: &Path, pattern: &str, rel: &str, limit: usize) -> Result<Vec<String>, String> {
    let needle = pattern.trim().to_lowercase();
    if needle.is_empty() {
        return Err("pattern 不能为空".into());
    }
    let start = if rel.trim().is_empty() {
        root.to_path_buf()
    } else {
        paths::resolve(root, rel, true)?
    };
    let limit = limit.clamp(1, 200);
    let mut out = Vec::new();
    if start.is_file() {
        scan_file(&start, root, &needle, limit, &mut out);
    } else {
        walk_grep(&start, root, &needle, limit, &mut out)?;
    }
    Ok(out)
}

/// 单文件检索(跳过 >2MB 与含 NUL 的二进制内容)。
fn scan_file(path: &Path, root: &Path, needle: &str, limit: usize, out: &mut Vec<String>) {
    if out.len() >= limit {
        return;
    }
    let Ok(meta) = path.metadata() else {
        return;
    };
    if meta.len() > 2 * 1024 * 1024 {
        return;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    if bytes[..bytes.len().min(4096)].contains(&0) {
        return;
    }
    let text = String::from_utf8_lossy(&bytes);
    let rel_path = paths::rel_of(root, path).unwrap_or_else(|_| display_path(path));
    for (i, line) in text.lines().enumerate() {
        if line.to_lowercase().contains(needle) {
            let line = crate::mcp::safety::truncate_chars(line.trim(), 300);
            out.push(format!("{rel_path}:{}:{}", i + 1, line));
            if out.len() >= limit {
                break;
            }
        }
    }
}

/// 递归检索:跳过超过 2MB 的文件与已知二进制扩展名。
fn walk_grep(
    dir: &Path,
    root: &Path,
    needle: &str,
    limit: usize,
    out: &mut Vec<String>,
) -> Result<(), String> {
    if out.len() >= limit {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("读取目录失败: {e}"))?
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if out.len() >= limit {
            break;
        }
        if path.is_dir() {
            walk_grep(&path, root, needle, limit, out)?;
            continue;
        }
        scan_file(&path, root, needle, limit, out);
    }
    Ok(())
}

fn write_text_in(
    root: &Path,
    rel: &str,
    content: &str,
    backup_base: &Path,
) -> Result<WriteResult, String> {
    let path = write_target(root, rel)?;
    let backup_id = backup::snapshot_in(backup_base, root, &path)?;
    std::fs::write(&path, content).map_err(|e| format!("写入失败: {e}"))?;
    audit("fs_write", rel, content.len());
    Ok(WriteResult {
        rel: rel.to_string(),
        bytes: content.len() as u64,
        backup_id,
    })
}

fn delete_in(root: &Path, rel: &str, backup_base: &Path) -> Result<WriteResult, String> {
    let path = paths::resolve(root, rel, true)?;
    if !path.is_file() {
        return Err("仅支持删除文件(目录请先清空)".into());
    }
    let backup_id = backup::snapshot_in(backup_base, root, &path)?;
    std::fs::remove_file(&path).map_err(|e| format!("删除失败: {e}"))?;
    audit("fs_delete", rel, 0);
    Ok(WriteResult {
        rel: rel.to_string(),
        bytes: 0,
        backup_id,
    })
}

/// 兼容入口(仅测试使用):标题 + 正文,用途/目录/封面自动识别。
#[cfg(test)]
fn export_docx_in(
    root: &Path,
    rel: &str,
    title: &str,
    body: &str,
    backup_base: &Path,
) -> Result<WriteResult, String> {
    let req = docgen::DocRequest {
        title: title.to_string(),
        body: body.to_string(),
        ..Default::default()
    };
    export_docx_req_in(root, rel, &req, backup_base)
}

/// 文档导出内核:只负责落盘、备份与审计;排版全部委托 `docgen`。
fn export_docx_req_in(
    root: &Path,
    rel: &str,
    req: &docgen::DocRequest,
    backup_base: &Path,
) -> Result<WriteResult, String> {
    let path = write_target(root, rel)?;
    let backup_id = backup::snapshot_in(backup_base, root, &path)?;
    let mut req = req.clone();
    // 相对图片路径默认展开到工作区根目录
    if req.base_dir.as_os_str().is_empty() {
        req.base_dir = root.to_path_buf();
    }
    let bytes = docgen::render(&req)?;
    std::fs::write(&path, &bytes).map_err(|e| format!("写入失败: {e}"))?;
    audit("fs_export_docx", rel, bytes.len());
    Ok(WriteResult {
        rel: rel.to_string(),
        bytes: bytes.len() as u64,
        backup_id,
    })
}

fn export_xlsx_in(
    root: &Path,
    rel: &str,
    sheets: &[SheetInput],
    backup_base: &Path,
) -> Result<WriteResult, String> {
    use rust_xlsxwriter::Workbook;

    let path = write_target(root, rel)?;
    let backup_id = backup::snapshot_in(backup_base, root, &path)?;

    let mut wb = Workbook::new();
    if sheets.is_empty() {
        let ws = wb.add_worksheet();
        ws.set_name("Sheet1")
            .map_err(|e| format!("设置表名失败: {e}"))?;
    }
    for sheet in sheets {
        let ws = wb.add_worksheet();
        let name = if sheet.name.trim().is_empty() {
            "Sheet"
        } else {
            sheet.name.trim()
        };
        ws.set_name(name)
            .map_err(|e| format!("表名非法 `{name}`: {e}"))?;
        for (r, row) in sheet.rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                if cell.is_empty() {
                    continue;
                }
                ws.write_string(r as u32, c as u16, cell)
                    .map_err(|e| format!("写单元格失败: {e}"))?;
            }
        }
    }
    wb.save(&path).map_err(|e| format!("保存 xlsx 失败: {e}"))?;
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    audit("fs_export_xlsx", rel, bytes as usize);
    Ok(WriteResult {
        rel: rel.to_string(),
        bytes,
        backup_id,
    })
}

/// 写目标:校验路径并确保父目录存在。
fn write_target(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let path = paths::resolve(root, rel, false)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    Ok(path)
}

fn audit(event: &str, rel: &str, bytes: usize) {
    crate::mcp::audit(
        &crate::paths::data_dir(),
        event,
        "fs",
        rel,
        &format!("bytes={bytes}"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> PathBuf {
        let p = std::env::temp_dir().join(format!("maf-fs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// 测试专用备份根,避免污染真实数据目录。
    fn bak() -> PathBuf {
        let p = std::env::temp_dir().join(format!("maf-bak-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn write_read_delete_with_backup() {
        let root = ws();
        let base = bak();
        let r1 = write_text_in(&root, "docs/a.md", "第一版", &base).unwrap();
        assert!(r1.backup_id.is_none());
        assert_eq!(read_text_in(&root, "docs/a.md", 0, 100).unwrap(), "第一版");

        let r2 = write_text_in(&root, "docs/a.md", "第二版", &base).unwrap();
        assert!(r2.backup_id.is_some(), "覆盖必须产生备份");
        assert_eq!(read_text_in(&root, "docs/a.md", 0, 100).unwrap(), "第二版");

        let d = delete_in(&root, "docs/a.md", &base).unwrap();
        assert!(d.backup_id.is_some());
        assert!(!root.join("docs/a.md").exists());
    }

    #[test]
    fn rejects_binary_and_allows_parent_jump() {
        let root = ws();
        let base = bak();
        std::fs::write(root.join("bin.dat"), [0u8, 1, 2]).unwrap();
        assert!(read_text_in(&root, "bin.dat", 0, 100).is_err());
        // 无沙箱:`..` 可跳出工作区(写入仍自动备份)
        let escaped = root
            .parent()
            .unwrap()
            .join(format!("maf-escape-{}.txt", std::process::id()));
        let rel = format!("../{}", escaped.file_name().unwrap().to_str().unwrap());
        write_text_in(&root, &rel, "x", &base).expect("`..` 不再被拦截");
        assert!(escaped.exists(), "应写入工作区外:{}", escaped.display());
        std::fs::remove_file(&escaped).ok();
    }

    #[test]
    fn read_offset_pages_through_text() {
        let root = ws();
        let base = bak();
        let body: String = "0123456789".repeat(10); // 100 字符
        write_text_in(&root, "big.txt", &body, &base).unwrap();
        let tail = read_text_in(&root, "big.txt", 90, 50).unwrap();
        assert!(tail.contains("已跳过前 90 字符"), "got: {tail}");
        assert!(tail.ends_with("0123456789"), "got: {tail}");
        // 偏移越过文件尾 → 空正文,不报错
        assert!(read_text_in(&root, "big.txt", 500, 10).is_ok());
    }

    #[test]
    fn grep_finds_matching_lines() {
        let root = ws();
        let base = bak();
        write_text_in(&root, "src/a.txt", "第一行\n错误:超时\n第三行", &base).unwrap();
        write_text_in(&root, "src/b.md", "# 说明\n无关内容", &base).unwrap();
        let hits = grep_in(&root, "超时", "", 50).unwrap();
        assert_eq!(hits.len(), 1, "got: {hits:?}");
        assert!(hits[0].contains("a.txt:2:"), "got: {}", hits[0]);
        // 限定子目录 + 上限
        let single = grep_in(&root, "说明", "src/b.md", 1).unwrap();
        assert_eq!(single.len(), 1);
        assert!(grep_in(&root, "  ", "", 10).is_err());
    }

    #[test]
    fn exports_docx_and_xlsx() {
        let root = ws();
        let base = bak();
        let d = export_docx_in(&root, "out/report.docx", "标题", "第一段\n第二段", &base).unwrap();
        assert!(d.bytes > 0);
        assert!(root.join("out/report.docx").exists());

        let sheets = vec![SheetInput {
            name: "数据".into(),
            rows: vec![
                vec!["名称".into(), "值".into()],
                vec!["A".into(), "1".into()],
            ],
        }];
        let x = export_xlsx_in(&root, "out/table.xlsx", &sheets, &base).unwrap();
        assert!(x.bytes > 0);
        assert!(root.join("out/table.xlsx").exists());
    }

    #[test]
    fn exports_docx_with_markdown_body() {
        let root = ws();
        let base = bak();
        let body = "# 一级标题\n\n## 二级标题\n\n- 要点一\n- 要点二\n\n| 名称 | 值 |\n|---|---|\n| A | 1 |\n\n```\nlet x = 1;\n```\n\n> 引用说明\n\n正文含 **粗体** 与 `code`。";
        let r = export_docx_in(&root, "out/md.docx", "文档标题", body, &base).unwrap();
        assert!(r.bytes > 0);
        assert!(root.join("out/md.docx").exists());
    }
}
