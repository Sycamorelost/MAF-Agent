//! 读取/分页、写入、删除与目录列表。

use std::path::{Path, PathBuf};

use crate::fs::{backup, paths};

use super::common::{
    FileEntry, MAX_LIST_ENTRIES, MAX_READ_CHARS, WriteResult, audit, display_path, write_target,
};

// ---------------- 对外接口(支持项目工作区绑定) ----------------

/// 全局默认工作区根(未绑定项目时使用)。
pub fn root() -> Result<PathBuf, String> {
    paths::default_workspace_root()
}

/// 解析会话绑定的工作区根 —— 绑定项目且路径有效时返回项目路径,否则回退全局。
pub fn root_for_session(session_id: Option<&str>) -> Result<PathBuf, String> {
    paths::resolve_workspace_root(session_id)
}

pub fn list(rel: &str, session_id: Option<&str>) -> Result<Vec<FileEntry>, String> {
    list_in(&paths::resolve_workspace_root(session_id)?, rel.trim())
}

pub fn read_text(
    rel: &str,
    offset: Option<usize>,
    max_chars: Option<usize>,
    session_id: Option<&str>,
) -> Result<String, String> {
    read_text_in(
        &paths::resolve_workspace_root(session_id)?,
        rel,
        offset.unwrap_or(0),
        max_chars.unwrap_or(MAX_READ_CHARS),
    )
}

/// P11:分页读取并返回「续读字节偏移」。
/// 字符 offset 每次都要从头扫描跳过前缀(O(文件大小));调用方拿到本页的 `next_byte`
/// 后改用 byte_offset 续读,即可保持 O(页大小) 的恒定开销。返回 (文本, 续读偏移)。
pub fn read_text_page(
    rel: &str,
    offset: Option<usize>,
    byte_offset: Option<u64>,
    max_chars: Option<usize>,
    session_id: Option<&str>,
) -> Result<(String, Option<u64>), String> {
    read_text_page_in(
        &paths::resolve_workspace_root(session_id)?,
        rel,
        offset.unwrap_or(0),
        max_chars.unwrap_or(MAX_READ_CHARS),
        byte_offset,
    )
}

pub fn write_text(
    rel: &str,
    content: &str,
    session_id: Option<&str>,
) -> Result<WriteResult, String> {
    let root = root_for_session(session_id)?;
    write_text_in(&root, rel, content, &backup::backup_dir())
}

pub fn delete(rel: &str, session_id: Option<&str>) -> Result<WriteResult, String> {
    let root = root_for_session(session_id)?;
    delete_in(&root, rel, &backup::backup_dir())
}

// ---------------- 内核(显式传入工作区根,便于测试) ----------------

pub(crate) fn list_in(root: &Path, rel: &str) -> Result<Vec<FileEntry>, String> {
    let dir = if rel.trim().is_empty() {
        root.to_path_buf()
    } else {
        paths::resolve_guarded(root, rel)?
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

/// 流式分页状态:边解码边计数,只保留当前页的字符。
struct PageState {
    offset: usize,
    max_chars: usize,
    skipped: usize,
    produced: usize,
    out: String,
    truncated: bool,
    first: bool,
}

impl PageState {
    fn new(offset: usize, max_chars: usize) -> Self {
        Self {
            offset,
            max_chars,
            skipped: 0,
            produced: 0,
            out: String::new(),
            truncated: false,
            first: true,
        }
    }

    /// 消费一个字符;返回 false 表示本页已满,调用方应停止读取。
    fn push(&mut self, ch: char) -> bool {
        // 文件头的 UTF-8 BOM 不计入正文,也不占字符偏移
        if self.first {
            self.first = false;
            if ch == '\u{feff}' {
                return true;
            }
        }
        if self.skipped < self.offset {
            self.skipped += 1;
            return true;
        }
        if self.produced < self.max_chars {
            self.out.push(ch);
            self.produced += 1;
            return true;
        }
        self.truncated = true;
        false
    }
}

pub(crate) fn read_text_in(
    root: &Path,
    rel: &str,
    offset: usize,
    max_chars: usize,
) -> Result<String, String> {
    read_text_page_in(root, rel, offset, max_chars, None).map(|(text, _)| text)
}

/// 分页读取内核:返回 (本页文本, 续读字节偏移/截断时才有)。
pub(crate) fn read_text_page_in(
    root: &Path,
    rel: &str,
    offset: usize,
    max_chars: usize,
    byte_offset: Option<u64>,
) -> Result<(String, Option<u64>), String> {
    use std::io::{Read, Seek, SeekFrom};

    let path = paths::resolve_guarded(root, rel)?;
    if !path.is_file() {
        return Err(format!("`{rel}` 不是文件"));
    }
    let max_chars = max_chars.max(1);
    let mut file = std::fs::File::open(&path).map_err(|e| format!("读取失败: {e}"))?;
    // 二进制探测只读前 8KB —— 不再 `std::fs::read` 整份文件(GB 级文件曾直接 OOM)。
    let mut probe = [0u8; 8192];
    let probe_len = file
        .read(&mut probe)
        .map_err(|e| format!("读取失败: {e}"))?;
    if probe[..probe_len].contains(&0) {
        return Err(format!(
            "`{rel}` 看起来是二进制文件,请使用专用导出/解析工具"
        ));
    }
    // P11:有 byte_offset 时直接定位(续读为 O(页大小));否则从文件头扫描跳过 offset 字符。
    let start = byte_offset.unwrap_or(0);
    file.seek(SeekFrom::Start(start))
        .map_err(|e| format!("读取失败: {e}"))?;

    // C3/F6:字符级流式分页。跳过 offset 个字符后收集满 max_chars 个字符即停止,
    // 全程只占用一个 64KB 读缓冲,文件多大都能通过 offset 翻页读完且不占额外内存。
    let mut page = PageState::new(offset, max_chars);
    let mut buf = vec![0u8; 64 * 1024];
    let mut pending: Vec<u8> = Vec::with_capacity(4);
    // P11:记录页边界字节位置(read_bytes = 已从文件读出的总字节数,
    // data 起点 = read_bytes - data.len();页边界 = 起点 + 块内偏移)
    let mut read_bytes: u64 = start;
    let mut next_byte: Option<u64> = None;
    'outer: loop {
        let n = file.read(&mut buf).map_err(|e| format!("读取失败: {e}"))?;
        if n == 0 {
            break;
        }
        read_bytes += n as u64;
        let data = {
            let mut d = Vec::with_capacity(pending.len() + n);
            d.extend_from_slice(&pending);
            d.extend_from_slice(&buf[..n]);
            d
        };
        pending.clear();
        let base = read_bytes - data.len() as u64;
        let mut pos = 0usize;
        while pos < data.len() {
            match std::str::from_utf8(&data[pos..]) {
                Ok(s) => {
                    for (i, ch) in s.char_indices() {
                        if !page.push(ch) {
                            next_byte = Some(base + (pos + i) as u64);
                            break 'outer;
                        }
                    }
                    pos = data.len();
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    if valid > 0 {
                        // valid_up_to 保证前缀合法
                        let s = std::str::from_utf8(&data[pos..pos + valid]).unwrap_or("");
                        for (i, ch) in s.char_indices() {
                            if !page.push(ch) {
                                next_byte = Some(base + (pos + i) as u64);
                                break 'outer;
                            }
                        }
                    }
                    pos += valid;
                    match e.error_len() {
                        // 非法字节序列:与 from_utf8_lossy 一致,以替换字符占位
                        Some(len) => {
                            if !page.push('\u{fffd}') {
                                next_byte = Some(base + pos as u64);
                                break 'outer;
                            }
                            pos += len;
                        }
                        // 块尾截断的多字节字符,留到下一块拼接
                        None => {
                            pending.extend_from_slice(&data[pos..]);
                            break;
                        }
                    }
                }
            }
        }
    }
    if !pending.is_empty() && !page.truncated {
        // 文件末尾残留的不完整 UTF-8 序列(损失解码)
        page.push('\u{fffd}');
    }

    let mut out = String::new();
    if page.skipped > 0 {
        out.push_str(&format!("…(已跳过前 {} 字符)\n", page.skipped));
    }
    out.push_str(&page.out);
    if page.truncated {
        match next_byte {
            // 优先给字节偏移:续读无需重扫前缀,恒定开销
            Some(nb) => out.push_str(&format!(
                "\n…(本段已截断:{max_chars} 字符;继续请用 byte_offset={nb},或 offset={})",
                page.skipped.saturating_add(page.produced)
            )),
            None => out.push_str(&format!(
                "\n…(本段已截断:{max_chars} 字符;继续请用 offset={})",
                page.skipped.saturating_add(page.produced)
            )),
        }
    }
    Ok((out, next_byte))
}

pub(crate) fn write_text_in(
    root: &Path,
    rel: &str,
    content: &str,
    backup_base: &Path,
) -> Result<WriteResult, String> {
    let path = write_target(root, rel)?;
    let backup_id = backup::snapshot_in(backup_base, root, &path, None, None)?;
    std::fs::write(&path, content).map_err(|e| format!("写入失败: {e}"))?;
    audit("fs_write", rel, content.len());
    Ok(WriteResult {
        rel: rel.to_string(),
        bytes: content.len() as u64,
        backup_id,
    })
}

pub(crate) fn delete_in(root: &Path, rel: &str, backup_base: &Path) -> Result<WriteResult, String> {
    // 删除统一走沙箱语义:相对路径禁止越界,绝对路径视为用户显式指定
    let path = paths::resolve_guarded(root, rel)?;
    if !path.is_file() {
        return Err("仅支持删除文件(目录请先清空)".into());
    }
    let backup_id = backup::snapshot_in(backup_base, root, &path, None, None)?;
    std::fs::remove_file(&path).map_err(|e| format!("删除失败: {e}"))?;
    audit("fs_delete", rel, 0);
    Ok(WriteResult {
        rel: rel.to_string(),
        bytes: 0,
        backup_id,
    })
}
