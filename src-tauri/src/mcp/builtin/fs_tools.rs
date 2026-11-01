//! 文件类内置工具:读 / 写 / 编辑 / 批次写 / 删除 / 导出 / 备份恢复 / 提取 / zip。
//!
//! 只做参数解析与转发,路径策略与备份由 `fs` 模块负责;
//! 审批(D3 工作区外读取、写入一律审批)由 dispatcher 负责,执行层不拦。

use serde_json::Value;

/// 本模块负责的工具名。
pub(super) fn handles(name: &str) -> bool {
    name.starts_with("fs_") || name.starts_with("zip_") || name == "file_extract"
}

pub(super) fn exec(name: &str, args: &Value, session_id: Option<&str>) -> (String, bool) {
    match name {
        "fs_list" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            match crate::fs::text::list(path, session_id) {
                Ok(entries) => (serde_json::to_string(&entries).unwrap_or_default(), true),
                Err(e) => (format!("列目录失败:{e}"), false),
            }
        }
        "fs_read" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            let max = args
                .get("max_chars")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            // C3/F6:大文件分页读取;P11:按字节偏移续读,避免每次从头扫描前缀
            let offset = args
                .get("offset")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            let byte_offset = args.get("byte_offset").and_then(|v| v.as_u64());
            match crate::fs::text::read_text_page(path, offset, byte_offset, max, session_id) {
                Ok((text, _)) => (text, true),
                Err(e) => (format!("读取失败:{e}"), false),
            }
        }
        "fs_grep" => {
            let pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if pattern.is_empty() {
                return ("缺少 pattern 参数".into(), false);
            }
            let rel = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
            match crate::fs::text::grep(pattern, rel, limit, session_id) {
                Ok(v) if v.is_empty() => ("未命中任何文本行".into(), true),
                Ok(v) => (v.join("\n"), true),
                Err(e) => (format!("检索失败:{e}"), false),
            }
        }
        "fs_write" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            // content 必须显式给出:省略时不能按空串写入(会把已有文件清空)
            let Some(content) = args.get("content").and_then(|v| v.as_str()) else {
                return ("缺少 content 参数(删除全部内容请显式传空字符串)".into(), false);
            };
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            match crate::fs::text::write_text(path, content, session_id) {
                Ok(r) => (serde_json::to_string(&r).unwrap_or_default(), true),
                Err(e) => (format!("写入失败:{e}"), false),
            }
        }
        "fs_edit" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let old = args.get("old").and_then(|v| v.as_str()).unwrap_or("");
            let Some(new) = args.get("new").and_then(|v| v.as_str()) else {
                return ("缺少 new 参数(删除片段请显式传空字符串)".into(), false);
            };
            let replace_all = args
                .get("replace_all")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            if old.is_empty() {
                return ("缺少 old 参数:请给出待替换的原文片段".into(), false);
            }
            match crate::fs::text::edit_text(path, old, new, replace_all, session_id) {
                Ok(r) => (serde_json::to_string(&r).unwrap_or_default(), true),
                Err(e) => (format!("编辑失败:{e}"), false),
            }
        }
        "fs_write_begin" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            match crate::fs::text::write_chunk_begin(path, session_id) {
                Ok(r) => (serde_json::to_string(&r).unwrap_or_default(), true),
                Err(e) => (format!("开始分批写入失败:{e}"), false),
            }
        }
        "fs_write_chunk" => {
            let id = args.get("write_id").and_then(|v| v.as_str()).unwrap_or("");
            let Some(content) = args.get("content").and_then(|v| v.as_str()) else {
                return ("缺少 content 参数".into(), false);
            };
            if id.is_empty() {
                return ("缺少 write_id 参数".into(), false);
            }
            match crate::fs::text::write_chunk_append(id, content) {
                Ok(r) => (serde_json::to_string(&r).unwrap_or_default(), true),
                Err(e) => (format!("追加写入失败:{e}"), false),
            }
        }
        "fs_write_finish" => {
            let id = args.get("write_id").and_then(|v| v.as_str()).unwrap_or("");
            if id.is_empty() {
                return ("缺少 write_id 参数".into(), false);
            }
            match crate::fs::text::write_chunk_finish(id) {
                Ok(r) => (serde_json::to_string(&r).unwrap_or_default(), true),
                Err(e) => (format!("收尾写入失败:{e}"), false),
            }
        }
        "fs_delete" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            match crate::fs::text::delete(path, session_id) {
                Ok(r) => (serde_json::to_string(&r).unwrap_or_default(), true),
                Err(e) => (format!("删除失败:{e}"), false),
            }
        }
        "fs_export_docx" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            let profile = args.get("profile").and_then(|v| v.as_str());
            let toc = args.get("toc").and_then(|v| v.as_bool());
            let meta: crate::fs::docgen::DocMeta = args
                .get("meta")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let req = crate::fs::docgen::DocRequest::from_parts(title, body, profile, toc, meta);
            match crate::fs::text::export_docx_ext(path, &req, session_id) {
                Ok(r) => (serde_json::to_string(&r).unwrap_or_default(), true),
                Err(e) => (format!("导出 docx 失败:{e}"), false),
            }
        }
        "fs_export_xlsx" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            let sheets: Vec<crate::fs::text::SheetInput> = match args.get("sheets") {
                Some(v) => match serde_json::from_value(v.clone()) {
                    Ok(s) => s,
                    Err(e) => return (format!("sheets 参数格式错误:{e}"), false),
                },
                None => Vec::new(),
            };
            match crate::fs::text::export_xlsx(path, &sheets, session_id) {
                Ok(r) => (serde_json::to_string(&r).unwrap_or_default(), true),
                Err(e) => (format!("导出 xlsx 失败:{e}"), false),
            }
        }
        "fs_restore" => {
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if id.is_empty() {
                return ("缺少 id 参数".into(), false);
            }
            let root = match crate::fs::text::root_for_session(session_id) {
                Ok(r) => r,
                Err(e) => return (format!("工作区不可用:{e}"), false),
            };
            match crate::fs::backup::restore(&root, id) {
                Ok(rel) => (format!("已恢复 `{rel}`"), true),
                Err(e) => (format!("恢复失败:{e}"), false),
            }
        }
        "file_extract" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            match crate::fs::formats::extract(path, session_id) {
                Ok(text) => (text, true),
                Err(e) => (format!("提取失败:{e}"), false),
            }
        }
        "zip_list" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ("缺少 path 参数".into(), false);
            }
            match crate::fs::formats::zip_list(path, session_id) {
                Ok(entries) => (serde_json::to_string(&entries).unwrap_or_default(), true),
                Err(e) => (format!("列压缩包失败:{e}"), false),
            }
        }
        "zip_extract" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let dest = args.get("dest").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() || dest.is_empty() {
                return ("缺少 path / dest 参数".into(), false);
            }
            match crate::fs::formats::zip_extract(path, dest, session_id) {
                Ok(n) => (format!("解包完成,共 {n} 个文件"), true),
                Err(e) => (format!("解包失败:{e}"), false),
            }
        }
        _ => ("未知内置工具".into(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::exec;
    use serde_json::json;

    /// 缺少 content 参数时不得把已有文件清空。
    #[tokio::test]
    async fn fs_write_requires_content_argument() {
        let dir = std::env::temp_dir().join(format!("maf-e2e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keep.txt").to_str().unwrap().to_string();
        std::fs::write(&path, "原有内容").unwrap();
        let (content, ok) = exec("fs_write", &json!({ "path": path }), None);
        assert!(!ok);
        assert!(content.contains("content"));
        // 已有文件内容未被清空
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "原有内容");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 文件工具:绝对路径可访问工作区外(全盘);D3 的审批在 dispatcher 层,执行层不拦。
    #[tokio::test]
    async fn fs_tools_accept_absolute_paths_outside_workspace() {
        let outside = std::env::temp_dir().join(format!("maf-abs-{}.txt", std::process::id()));
        std::fs::write(&outside, "absolute-ok").unwrap();
        let (content, ok) = exec(
            "fs_read",
            &json!({ "path": outside.to_str().unwrap() }),
            None,
        );
        assert!(ok, "绝对路径应可读取,实际内容:{content}");
        assert!(content.contains("absolute-ok"));

        // 相对路径 `..`:不再要求留在工作区内
        let (content, ok) = exec("fs_list", &json!({ "path": ".." }), None);
        assert!(ok, "`..` 应可列出,实际内容:{content}");
        std::fs::remove_file(&outside).ok();
    }
}
