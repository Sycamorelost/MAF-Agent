//! 工作区文件读写与办公格式导出。
//!
//! 所有写操作写前自动备份(可回滚),所有路径经 `fs::paths` 校验在
//! 工作区内且非敏感文件。导出支持 docx / xlsx,供 Agent 产出可直接交付的文件。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::fs::{backup, paths};

/// 单次读取上限(字符)。
pub const MAX_READ_CHARS: usize = 200_000;
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
    list_in(&root()?, rel)
}

pub fn read_text(rel: &str, max_chars: Option<usize>) -> Result<String, String> {
    read_text_in(&root()?, rel, max_chars.unwrap_or(MAX_READ_CHARS))
}

pub fn write_text(rel: &str, content: &str) -> Result<WriteResult, String> {
    write_text_in(&root()?, rel, content, &backup::backup_dir())
}

pub fn delete(rel: &str) -> Result<WriteResult, String> {
    delete_in(&root()?, rel, &backup::backup_dir())
}

pub fn export_docx(rel: &str, title: &str, body: &str) -> Result<WriteResult, String> {
    export_docx_in(&root()?, rel, title, body, &backup::backup_dir())
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
            rel: paths::rel_of(root, &path).unwrap_or_default(),
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

fn read_text_in(root: &Path, rel: &str, max_chars: usize) -> Result<String, String> {
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
    Ok(crate::mcp::safety::truncate_chars(
        text.trim_start_matches('\u{feff}'),
        max_chars,
    ))
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

fn export_docx_in(
    root: &Path,
    rel: &str,
    title: &str,
    body: &str,
    backup_base: &Path,
) -> Result<WriteResult, String> {
    use docx_rs::{Docx, Paragraph, Run};

    let path = write_target(root, rel)?;
    let backup_id = backup::snapshot_in(backup_base, root, &path)?;

    let mut docx = Docx::new();
    if !title.trim().is_empty() {
        let head = Paragraph::new().add_run(Run::new().add_text(title.trim()).size(32));
        docx = docx.add_paragraph(head);
    }
    for line in body.lines() {
        docx = docx.add_paragraph(Paragraph::new().add_run(Run::new().add_text(line)));
    }
    let file = std::fs::File::create(&path).map_err(|e| format!("创建 docx 失败: {e}"))?;
    docx.pack(file)
        .map_err(|e| format!("写入 docx 失败: {e}"))?;
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    audit("fs_export_docx", rel, bytes as usize);
    Ok(WriteResult {
        rel: rel.to_string(),
        bytes,
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
        assert_eq!(read_text_in(&root, "docs/a.md", 100).unwrap(), "第一版");

        let r2 = write_text_in(&root, "docs/a.md", "第二版", &base).unwrap();
        assert!(r2.backup_id.is_some(), "覆盖必须产生备份");
        assert_eq!(read_text_in(&root, "docs/a.md", 100).unwrap(), "第二版");

        let d = delete_in(&root, "docs/a.md", &base).unwrap();
        assert!(d.backup_id.is_some());
        assert!(!root.join("docs/a.md").exists());
    }

    #[test]
    fn rejects_binary_and_escape() {
        let root = ws();
        let base = bak();
        std::fs::write(root.join("bin.dat"), [0u8, 1, 2]).unwrap();
        assert!(read_text_in(&root, "bin.dat", 100).is_err());
        assert!(write_text_in(&root, "../escape.txt", "x", &base).is_err());
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
}
