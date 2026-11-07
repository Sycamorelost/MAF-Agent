//! docx / xlsx 导出:只负责落盘、备份与审计,排版全部委托 `docgen`。

use std::path::Path;

use crate::fs::{backup, docgen};

use super::common::{SheetInput, WriteResult, audit, write_target};
use super::io::root_for_session;

/// 完整文档导出:用途模板、目录、封面元信息均可指定。
pub fn export_docx_ext(
    rel: &str,
    req: &docgen::DocRequest,
    session_id: Option<&str>,
) -> Result<WriteResult, String> {
    let root = root_for_session(session_id)?;
    export_docx_req_in(&root, rel, req, &backup::backup_dir())
}

pub fn export_xlsx(
    rel: &str,
    sheets: &[SheetInput],
    session_id: Option<&str>,
) -> Result<WriteResult, String> {
    let root = root_for_session(session_id)?;
    export_xlsx_in(&root, rel, sheets, &backup::backup_dir())
}

/// 兼容入口(仅测试使用):标题 + 正文,用途/目录/封面自动识别。
#[cfg(test)]
pub(crate) fn export_docx_in(
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
    let backup_id = backup::snapshot_in(backup_base, root, &path, None, None)?;
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

pub(crate) fn export_xlsx_in(
    root: &Path,
    rel: &str,
    sheets: &[SheetInput],
    backup_base: &Path,
) -> Result<WriteResult, String> {
    use rust_xlsxwriter::Workbook;

    let path = write_target(root, rel)?;
    let backup_id = backup::snapshot_in(backup_base, root, &path, None, None)?;

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
