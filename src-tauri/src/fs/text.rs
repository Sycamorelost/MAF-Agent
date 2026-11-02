#![allow(dead_code)]

//! 文件读写与办公格式导出(绝对路径可直达本机任意位置,相对路径以工作区为基准)。
//!
//! 所有写操作写前自动备份(可回滚),路径经 `fs::paths` 解析(位置与文件名均不设限)。
//! 导出支持 docx / xlsx,供 Agent 产出可直接交付的文件。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::fs::{backup, docgen, paths};

/// 单次读取默认页大小(字符)。调用方可用 offset/max_chars 分页续读,文件本身不设大小上限。
pub const MAX_READ_CHARS: usize = 32_000;
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

// ---------------- 对外接口(支持项目工作区绑定) ----------------

/// 全局默认工作区根(未绑定项目时使用)。
pub fn root() -> Result<PathBuf, String> {
    paths::workspace_root()
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
    let enforce = session_id_is_bound_to_project(session_id);
    write_text_in(&root, rel, content, &backup::backup_dir(), enforce)
}

pub fn delete(rel: &str, session_id: Option<&str>) -> Result<WriteResult, String> {
    let root = root_for_session(session_id)?;
    let enforce = session_id_is_bound_to_project(session_id);
    delete_in(&root, rel, &backup::backup_dir(), enforce)
}

/// 完整文档导出:用途模板、目录、封面元信息均可指定。
pub fn export_docx_ext(
    rel: &str,
    req: &docgen::DocRequest,
    session_id: Option<&str>,
) -> Result<WriteResult, String> {
    let root = root_for_session(session_id)?;
    let enforce = session_id_is_bound_to_project(session_id);
    export_docx_req_in(&root, rel, req, &backup::backup_dir(), enforce)
}

pub fn export_xlsx(
    rel: &str,
    sheets: &[SheetInput],
    session_id: Option<&str>,
) -> Result<WriteResult, String> {
    let root = root_for_session(session_id)?;
    let enforce = session_id_is_bound_to_project(session_id);
    export_xlsx_in(&root, rel, sheets, &backup::backup_dir(), enforce)
}

/// 判断 session 是否绑定了有效项目路径 —— 用于决定写操作要不要做越界检查。
fn session_id_is_bound_to_project(session_id: Option<&str>) -> bool {
    if let Some(sid) = session_id
        && let Ok(Some(session)) = crate::db::sessions::get(sid)
        && let Some(ref pid) = session.project_id
        && let Ok(Some(proj)) = crate::db::projects::get(pid)
    {
        return std::path::Path::new(&proj.path).is_dir();
    }
    false
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

fn read_text_in(root: &Path, rel: &str, offset: usize, max_chars: usize) -> Result<String, String> {
    read_text_page_in(root, rel, offset, max_chars, None).map(|(text, _)| text)
}

/// 分页读取内核:返回 (本页文本, 续读字节偏移/截断时才有)。
fn read_text_page_in(
    root: &Path,
    rel: &str,
    offset: usize,
    max_chars: usize,
    byte_offset: Option<u64>,
) -> Result<(String, Option<u64>), String> {
    use std::io::{Read, Seek, SeekFrom};

    let path = paths::resolve(root, rel, true)?;
    if !path.is_file() {
        return Err(format!("`{rel}` 不是文件"));
    }
    let max_chars = max_chars.max(1);
    let mut file = std::fs::File::open(&path).map_err(|e| format!("读取失败: {e}"))?;
    // 二进制探测只读前 8KB —— 不再 `std::fs::read` 整份文件(GB 级文件曾直接 OOM)。
    let mut probe = [0u8; 8192];
    let probe_len = file.read(&mut probe).map_err(|e| format!("读取失败: {e}"))?;
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

/// 文本检索(仅文本类扩展名,单文件上限 2MB),返回「文件:行号:内容」。
/// 绝对路径直接检索;相对路径以工作区为基准。
pub fn grep(
    pattern: &str,
    rel: &str,
    limit: usize,
    session_id: Option<&str>,
) -> Result<Vec<String>, String> {
    grep_in(&paths::resolve_workspace_root(session_id)?, pattern, rel, limit)
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
    enforce_boundary: bool,
) -> Result<WriteResult, String> {
    let path = write_target(root, rel, enforce_boundary)?;
    let backup_id = backup::snapshot_in(backup_base, root, &path, None, None)?;
    std::fs::write(&path, content).map_err(|e| format!("写入失败: {e}"))?;
    audit("fs_write", rel, content.len());
    Ok(WriteResult {
        rel: rel.to_string(),
        bytes: content.len() as u64,
        backup_id,
    })
}

fn delete_in(
    root: &Path,
    rel: &str,
    backup_base: &Path,
    enforce_boundary: bool,
) -> Result<WriteResult, String> {
    // 删除操作也要做越界检查(禁止删项目外的文件)
    let path = if enforce_boundary {
        paths::ensure_within(root, Path::new(rel))?
    } else {
        paths::resolve(root, rel, true)?
    };
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

// ---------------- 精确编辑(局部替换) ----------------

#[derive(Debug, Clone, Serialize)]
pub struct EditResult {
    pub rel: String,
    /// 实际替换处数
    pub replaced: usize,
    pub bytes: u64,
    pub backup_id: Option<String>,
}

/// 精确替换文件中的一段原文。比 fs_write 整体重写更省 token、也更不容易误伤其他内容。
/// 片段出现 0 次或多次(未开 replace_all)时直接报错,要求模型给出更精确的上下文。
pub fn edit_text(
    rel: &str,
    old: &str,
    new: &str,
    replace_all: bool,
    session_id: Option<&str>,
) -> Result<EditResult, String> {
    let root = root_for_session(session_id)?;
    let enforce = session_id_is_bound_to_project(session_id);
    edit_text_in(
        &root,
        rel,
        old,
        new,
        replace_all,
        &backup::backup_dir(),
        enforce,
    )
}

fn edit_text_in(
    root: &Path,
    rel: &str,
    old: &str,
    new: &str,
    replace_all: bool,
    backup_base: &Path,
    enforce: bool,
) -> Result<EditResult, String> {
    if old.is_empty() {
        return Err("old 不能为空:请给出待替换的原文片段".into());
    }
    let path = if enforce {
        paths::ensure_within(root, Path::new(rel))?
    } else {
        paths::resolve(root, rel, true)?
    };
    if !path.is_file() {
        return Err(format!(
            "`{rel}` 不是文件(精确编辑需目标已存在,新建请用 fs_write)"
        ));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("读取失败: {e}"))?;
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return Err(format!("`{rel}` 看起来是二进制文件,无法精确编辑"));
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        format!("`{rel}` 不是合法 UTF-8 文本,无法精确替换;请用 fs_write 整体重写")
    })?;
    let count = text.matches(old).count();
    if count == 0 {
        return Err(format!(
            "未在 `{rel}` 中找到待替换片段(区分大小写与空白)。请先用 fs_read 读取原文,再逐字给出完全一致的片段"
        ));
    }
    if count > 1 && !replace_all {
        return Err(format!(
            "待替换片段在 `{rel}` 中出现 {count} 次;请提供更长、更唯一的上下文,或设置 replace_all=true 全部替换"
        ));
    }
    let updated = if replace_all {
        text.replace(old, new)
    } else {
        text.replacen(old, new, 1)
    };
    let backup_id = backup::snapshot_in(backup_base, root, &path, None, None)?;
    std::fs::write(&path, updated.as_bytes()).map_err(|e| format!("写入失败: {e}"))?;
    audit("fs_edit", rel, updated.len());
    Ok(EditResult {
        rel: rel.to_string(),
        replaced: if replace_all { count } else { 1 },
        bytes: updated.len() as u64,
        backup_id,
    })
}

// ---------------- 大文件分批写入(临时文件 → 收尾落盘) ----------------
// 模型单次回复有 token 上限,一次性写几十万字符必然被截断。这里按
// 「fs_write_begin → 多次 fs_write_chunk → fs_write_finish」三段式:
//   1. begin:按目标路径创建临时分片(数据目录 tmp/write,不污染工作区);
//   2. chunk:逐段追加(不设字节上限,模型每次输出一页即可);
//   3. finish:备份原文件后把分片整体落到目标路径(跨卷自动回退为复制)。
// 中途中断不会留下半截的目标文件(目标只在 finish 时被写)。

#[derive(Debug, Clone, Serialize)]
pub struct WriteChunkState {
    pub write_id: String,
    pub path: String,
    /// 已累计写入的字节数
    pub bytes: u64,
}

/// P10:单个写会话的存活上限 —— 超过即视为中断残留并清理。
const CHUNK_SESSION_TTL: std::time::Duration = std::time::Duration::from_secs(2 * 3600);
/// P10:同一会话最多并存的写会话数,防止模型忘记 finish 导致临时文件堆积。
const MAX_CHUNK_SESSIONS_PER_OWNER: usize = 4;

struct ChunkWrite {
    root: PathBuf,
    rel: String,
    target: PathBuf,
    temp: PathBuf,
    bytes: u64,
    /// 所属聊天会话(用于并发上限;None = 无会话上下文)
    owner: Option<String>,
    /// 创建时刻(用于 2h TTL;用墙钟便于测试回拨时间)
    created: std::time::SystemTime,
}

static CHUNK_WRITES: OnceLock<Mutex<HashMap<String, ChunkWrite>>> = OnceLock::new();

fn chunk_map() -> &'static Mutex<HashMap<String, ChunkWrite>> {
    CHUNK_WRITES.get_or_init(|| {
        // P10:进程启动后首次使用时清理过期分片(此前只在 begin 时清理)
        cleanup_stale_parts(&chunk_tmp_dir());
        Mutex::new(HashMap::new())
    })
}

/// 判断写会话是否已超过 TTL(时钟回拨时保守视为未过期)。
fn chunk_expired(w: &ChunkWrite) -> bool {
    std::time::SystemTime::now()
        .duration_since(w.created)
        .map(|elapsed| elapsed >= CHUNK_SESSION_TTL)
        .unwrap_or(false)
}

/// 清理 map 中已过期的写会话,返回需要删除的临时文件路径。
fn take_expired_parts(map: &mut HashMap<String, ChunkWrite>) -> Vec<PathBuf> {
    let mut expired = Vec::new();
    map.retain(|_, w| {
        if chunk_expired(w) {
            expired.push(w.temp.clone());
            false
        } else {
            true
        }
    });
    expired
}

/// 分批写入的临时目录(应用数据目录下 tmp/write)。
fn chunk_tmp_dir() -> PathBuf {
    crate::paths::data_dir().join("tmp").join("write")
}

pub fn write_chunk_begin(rel: &str, session_id: Option<&str>) -> Result<WriteChunkState, String> {
    let root = root_for_session(session_id)?;
    let enforce = session_id_is_bound_to_project(session_id);
    write_chunk_begin_in(&root, rel, enforce, &chunk_tmp_dir(), session_id)
}

fn write_chunk_begin_in(
    root: &Path,
    rel: &str,
    enforce: bool,
    tmp: &Path,
    owner: Option<&str>,
) -> Result<WriteChunkState, String> {
    if rel.trim().is_empty() {
        return Err("缺少 path 参数".into());
    }
    std::fs::create_dir_all(tmp).map_err(|e| format!("创建临时目录失败: {e}"))?;
    cleanup_stale_parts(tmp);
    let mut map = chunk_map()
        .lock()
        .map_err(|_| "写入会话锁已被污染".to_string())?;
    // P10:先回收超过 TTL 的写会话,再校验并发上限,避免无效目标目录被提前创建
    let expired = take_expired_parts(&mut map);
    for p in expired {
        let _ = std::fs::remove_file(&p);
    }
    if let Some(sid) = owner {
        let active = map
            .values()
            .filter(|w| w.owner.as_deref() == Some(sid))
            .count();
        if active >= MAX_CHUNK_SESSIONS_PER_OWNER {
            return Err(format!(
                "本会话仍有 {active} 个未收尾的分批写入(上限 {MAX_CHUNK_SESSIONS_PER_OWNER});\
                 请先 fs_write_finish 完成或放弃其中一个,再重新 begin"
            ));
        }
    }
    // 目标路径在这次 begin 就解析并校验(项目绑定场景做越界检查),后续分片只写临时文件
    let target = write_target(root, rel, enforce)?;
    let id = uuid::Uuid::new_v4().to_string();
    let temp = tmp.join(format!("{id}.part"));
    std::fs::write(&temp, b"").map_err(|e| format!("创建临时文件失败: {e}"))?;
    map.insert(
        id.clone(),
        ChunkWrite {
            root: root.to_path_buf(),
            rel: rel.to_string(),
            target,
            temp,
            bytes: 0,
            owner: owner.map(str::to_string),
            created: std::time::SystemTime::now(),
        },
    );
    drop(map);
    audit("fs_write_begin", rel, 0);
    Ok(WriteChunkState {
        write_id: id,
        path: rel.to_string(),
        bytes: 0,
    })
}

/// 过期会话的收口提示(统一文案,便于模型据此重新 begin)。
fn chunk_expired_msg(write_id: &str) -> String {
    format!(
        "写入会话 `{write_id}` 已超时(超过 {} 小时未收尾),临时分片已清理;请重新调用 fs_write_begin",
        CHUNK_SESSION_TTL.as_secs() / 3600
    )
}

/// 追加上一段内容。返回累计字节数,模型可据此确认是否写全。
pub fn write_chunk_append(write_id: &str, content: &str) -> Result<WriteChunkState, String> {
    use std::io::Write as _;

    let mut map = chunk_map()
        .lock()
        .map_err(|_| "写入会话锁已被污染".to_string())?;
    // P10:过期会话立即清理并提示重新 begin,避免往已被外部清理的分片里写
    if map.get(write_id).is_some_and(chunk_expired) {
        if let Some(old) = map.remove(write_id) {
            let _ = std::fs::remove_file(&old.temp);
        }
        return Err(chunk_expired_msg(write_id));
    }
    let w = map.get_mut(write_id).ok_or_else(|| {
        format!("写入会话 `{write_id}` 不存在或已结束;请重新调用 fs_write_begin")
    })?;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&w.temp)
        .map_err(|e| format!("打开临时文件失败: {e}"))?;
    f.write_all(content.as_bytes())
        .map_err(|e| format!("写入临时文件失败: {e}"))?;
    w.bytes += content.len() as u64;
    Ok(WriteChunkState {
        write_id: write_id.to_string(),
        path: w.rel.clone(),
        bytes: w.bytes,
    })
}

/// 收尾:备份目标文件后把分片落到目标路径。
/// 中途失败时把写会话塞回注册表,模型可修正后重试 finish,不必重传所有分片。
pub fn write_chunk_finish(write_id: &str) -> Result<WriteResult, String> {
    let w = {
        let mut map = chunk_map()
            .lock()
            .map_err(|_| "写入会话锁已被污染".to_string())?;
        map.remove(write_id)
            .ok_or_else(|| format!("写入会话 `{write_id}` 不存在或已结束"))?
    };
    if chunk_expired(&w) {
        let _ = std::fs::remove_file(&w.temp);
        return Err(chunk_expired_msg(write_id));
    }
    let fail = |w: ChunkWrite, msg: String| -> Result<WriteResult, String> {
        if let Ok(mut map) = chunk_map().lock() {
            map.insert(write_id.to_string(), w);
        }
        Err(msg)
    };
    let backup_id = match backup::snapshot_in(&backup::backup_dir(), &w.root, &w.target, None, None) {
        Ok(b) => b,
        Err(e) => return fail(w, e),
    };
    if let Some(parent) = w.target.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return fail(w, format!("创建目标目录失败: {e}"));
    }
    // 同卷优先 rename(原子);数据目录与工作区跨卷时回退为复制
    if std::fs::rename(&w.temp, &w.target).is_err() {
        if let Err(e) = std::fs::copy(&w.temp, &w.target) {
            return fail(w, format!("落盘失败: {e}"));
        }
        let _ = std::fs::remove_file(&w.temp);
    }
    audit("fs_write", &w.rel, w.bytes as usize);
    Ok(WriteResult {
        rel: w.rel,
        bytes: w.bytes,
        backup_id,
    })
}

/// write_id 是否仍有效(供审批策略判断:目标路径已在 begin 时校验通过)。
pub fn write_chunk_known(write_id: &str) -> bool {
    let Ok(mut map) = chunk_map().lock() else {
        return false;
    };
    match map.get(write_id) {
        Some(w) if !chunk_expired(w) => true,
        Some(_) => {
            if let Some(old) = map.remove(write_id) {
                let _ = std::fs::remove_file(&old.temp);
            }
            false
        }
        None => false,
    }
}

/// 清理超过 TTL(2h)未收尾的临时分片,避免中断后残留堆积。
fn cleanup_stale_parts(tmp: &Path) {
    let cutoff = std::time::SystemTime::now().checked_sub(CHUNK_SESSION_TTL);
    let Ok(entries) = std::fs::read_dir(tmp) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("part") {
            continue;
        }
        if let (Some(cutoff), Ok(t)) = (cutoff, entry.metadata().and_then(|m| m.modified()))
            && t.duration_since(cutoff).is_err()
        {
            let _ = std::fs::remove_file(&path);
        }
    }
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
    export_docx_req_in(root, rel, &req, backup_base, false)
}

/// 文档导出内核:只负责落盘、备份与审计;排版全部委托 `docgen`。
fn export_docx_req_in(
    root: &Path,
    rel: &str,
    req: &docgen::DocRequest,
    backup_base: &Path,
    enforce_boundary: bool,
) -> Result<WriteResult, String> {
    let path = write_target(root, rel, enforce_boundary)?;
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

fn export_xlsx_in(
    root: &Path,
    rel: &str,
    sheets: &[SheetInput],
    backup_base: &Path,
    enforce_boundary: bool,
) -> Result<WriteResult, String> {
    use rust_xlsxwriter::Workbook;

    let path = write_target(root, rel, enforce_boundary)?;
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

/// 写目标:校验路径并确保父目录存在。
/// `enforce_boundary=true` 时额外做 ensure_within(项目绑定场景的越界检查)。
fn write_target(root: &Path, rel: &str, enforce_boundary: bool) -> Result<PathBuf, String> {
    let path = if enforce_boundary {
        paths::ensure_within(root, Path::new(rel))?
    } else {
        paths::resolve(root, rel, false)?
    };
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

    /// 分批写会话是进程级全局表,且 `write_chunk_begin_in` 会顺带清扫过期会话:
    /// 并行跑测试时,别的用例 begin 会把「被回拨成过期」的会话扫掉,
    /// 令 TTL 用例拿到「不存在」而非「已超时」。相关用例统一持此锁串行执行。
    static CHUNK_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn chunk_guard() -> std::sync::MutexGuard<'static, ()> {
        CHUNK_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn write_read_delete_with_backup() {
        let root = ws();
        let base = bak();
        let r1 = write_text_in(&root, "docs/a.md", "第一版", &base, false).unwrap();
        assert!(r1.backup_id.is_none());
        assert_eq!(read_text_in(&root, "docs/a.md", 0, 100).unwrap(), "第一版");

        let r2 = write_text_in(&root, "docs/a.md", "第二版", &base, false).unwrap();
        assert!(r2.backup_id.is_some(), "覆盖必须产生备份");
        assert_eq!(read_text_in(&root, "docs/a.md", 0, 100).unwrap(), "第二版");

        let d = delete_in(&root, "docs/a.md", &base, false).unwrap();
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
        write_text_in(&root, &rel, "x", &base, false).expect("`..` 不再被拦截");
        assert!(escaped.exists(), "应写入工作区外:{}", escaped.display());
        std::fs::remove_file(&escaped).ok();
    }

    #[test]
    fn read_offset_pages_through_text() {
        let root = ws();
        let base = bak();
        let body: String = "0123456789".repeat(10); // 100 字符
        write_text_in(&root, "big.txt", &body, &base, false).unwrap();
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
        write_text_in(&root, "src/a.txt", "第一行\n错误:超时\n第三行", &base, false).unwrap();
        write_text_in(&root, "src/b.md", "# 说明\n无关内容", &base, false).unwrap();
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
        let x = export_xlsx_in(&root, "out/table.xlsx", &sheets, &base, false).unwrap();
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

    #[test]
    fn read_pagination_covers_every_char() {
        let root = ws();
        let base = bak();
        // 中英混排,确保分页边界经常落在多字节字符上
        let body: String = "中文内容测试ABC\n1234567890\n".repeat(300);
        write_text_in(&root, "mix.txt", &body, &base, false).unwrap();
        let mut offset = 0usize;
        let mut pages: Vec<String> = Vec::new();
        while offset < body.chars().count() {
            let page = read_text_in(&root, "mix.txt", offset, 700).unwrap();
            // 去掉分页提示后还原正文
            let page = match page.split_once('\n') {
                Some((first, rest)) if first.starts_with("…(已跳过前") => rest.to_string(),
                _ => page,
            };
            let page = match page.rsplit_once("\n…(本段已截断") {
                Some((rest, _)) => rest.to_string(),
                None => page,
            };
            offset += page.chars().count();
            pages.push(page);
        }
        let joined: String = pages.concat();
        assert_eq!(joined, body, "分页拼接必须与原文完全一致(无重复无遗漏)");
    }

    /// P11:byte_offset 续读 —— 偏移单调前进、拼接完整、截断提示带字节续读位置。
    #[test]
    fn byte_offset_pagination_covers_every_char() {
        let root = ws();
        let base = bak();
        let body: String = "中文内容测试ABC\n1234567890\n".repeat(300);
        write_text_in(&root, "mix2.txt", &body, &base, false).unwrap();
        let mut byte = 0u64;
        let mut joined = String::new();
        let mut pages = 0;
        loop {
            let (page, next) = read_text_page_in(&root, "mix2.txt", 0, 700, Some(byte)).unwrap();
            if pages == 0 {
                assert!(
                    page.contains("byte_offset="),
                    "截断提示必须给出字节续读位置, got: {page}"
                );
            }
            // 去掉截断提示,还原正文
            let page = match page.rsplit_once("\n…(本段已截断") {
                Some((rest, _)) => rest.to_string(),
                None => page,
            };
            joined.push_str(&page);
            pages += 1;
            match next {
                Some(nb) => {
                    assert!(nb > byte, "续读偏移必须前进: {byte} -> {nb}");
                    byte = nb;
                }
                None => break,
            }
            assert!(pages < 1_000, "分页未收敛");
        }
        assert_eq!(joined, body, "byte_offset 分页拼接必须与原文完全一致");
    }

    #[test]
    fn edit_replaces_unique_occurrence() {
        let root = ws();
        let base = bak();
        write_text_in(&root, "doc.md", "标题\n旧句子\n结尾", &base, false).unwrap();
        let r = edit_text_in(&root, "doc.md", "旧句子", "新句子", false, &base, false).unwrap();
        assert_eq!(r.replaced, 1);
        assert_eq!(
            read_text_in(&root, "doc.md", 0, 100).unwrap(),
            "标题\n新句子\n结尾"
        );
        assert!(r.backup_id.is_some(), "编辑前必须备份");
    }

    #[test]
    fn edit_rejects_missing_and_ambiguous_fragment() {
        let root = ws();
        let base = bak();
        write_text_in(&root, "doc.md", "重复\n重复\n唯一", &base, false).unwrap();
        // 未命中
        let err = edit_text_in(&root, "doc.md", "不存在", "x", false, &base, false)
            .expect_err("未命中必须报错");
        assert!(err.contains("未在"), "got: {err}");
        // 多处且未开 replace_all
        let err = edit_text_in(&root, "doc.md", "重复", "x", false, &base, false)
            .expect_err("多处必须报错");
        assert!(err.contains("出现 2 次"), "got: {err}");
        // 多处 + replace_all → 全部替换
        let r = edit_text_in(&root, "doc.md", "重复", "替换", true, &base, false).unwrap();
        assert_eq!(r.replaced, 2);
        assert_eq!(
            read_text_in(&root, "doc.md", 0, 100).unwrap(),
            "替换\n替换\n唯一"
        );
        // old 为空直接拒绝
        assert!(edit_text_in(&root, "doc.md", "", "x", false, &base, false).is_err());
    }

    #[test]
    fn chunked_write_restores_large_file_exactly() {
        let _g = chunk_guard();
        let root = ws();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        // 100 段 × 1.5 万字符(≈4.3MB 字节),模拟模型分段输出
        let chunk = "分批写入测试内容🙂\n".repeat(1500);
        let total_chars = chunk.chars().count() * 100;
        let st = write_chunk_begin_in(&root, "big/out.txt", false, &tmp, None).unwrap();
        assert!(write_chunk_known(&st.write_id));
        let mut last = 0u64;
        for _ in 0..100 {
            let s = write_chunk_append(&st.write_id, &chunk).unwrap();
            assert!(s.bytes > last, "累计字节必须单调递增");
            last = s.bytes;
        }
        let fin = write_chunk_finish(&st.write_id).unwrap();
        assert!(!write_chunk_known(&st.write_id), "finish 后会话必须失效");
        assert_eq!(fin.bytes, last);
        let body = std::fs::read_to_string(root.join("big/out.txt")).unwrap();
        assert_eq!(body.chars().count(), total_chars);
        assert_eq!(body, chunk.repeat(100), "分批内容必须精确还原");
        assert!(body.len() > 3 * 1024 * 1024, "测试内容需超过 3MB");
        assert!(fin.backup_id.is_none(), "新建文件无需备份");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn chunked_write_rejects_unknown_id_and_backs_up_overwrite() {
        let _g = chunk_guard();
        let root = ws();
        let base = bak();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        assert!(write_chunk_append("no-such-id", "x").is_err());
        assert!(write_chunk_finish("no-such-id").is_err());
        write_text_in(&root, "a.txt", "旧内容", &base, false).unwrap();
        let st = write_chunk_begin_in(&root, "a.txt", false, &tmp, None).unwrap();
        write_chunk_append(&st.write_id, "新内容").unwrap();
        let fin = write_chunk_finish(&st.write_id).unwrap();
        assert!(fin.backup_id.is_some(), "覆盖已有文件必须备份");
        assert_eq!(read_text_in(&root, "a.txt", 0, 100).unwrap(), "新内容");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn read_without_max_chars_returns_whole_file() {
        let root = ws();
        let base = bak();
        let body = "长文".repeat(200_000); // 40 万字符
        write_text_in(&root, "huge.txt", &body, &base, false).unwrap();
        let got = read_text_in(&root, "huge.txt", 0, usize::MAX).unwrap();
        assert_eq!(got, body, "max_chars 不设上限时应一次读完");
    }

    #[test]
    fn read_strips_bom_and_tolerates_invalid_utf8() {
        let root = ws();
        std::fs::write(root.join("bom.txt"), "\u{feff}正文AB").unwrap();
        assert_eq!(read_text_in(&root, "bom.txt", 0, 10).unwrap(), "正文AB");
        // 非法 UTF-8 字节按替换字符占位,不 panic、不中断
        std::fs::write(root.join("bad.txt"), [b'h', 0xFF, b'i']).unwrap();
        assert_eq!(
            read_text_in(&root, "bad.txt", 0, 10).unwrap(),
            "h\u{fffd}i"
        );
    }

    #[test]
    fn boundary_check_blocks_outside_write() {
        // enforce_boundary=true 时,相对 `../escape.txt` 被拦截
        let root = ws();
        let base = bak();
        let escaped = root
            .parent()
            .unwrap()
            .join(format!("maf-guard-{}.txt", std::process::id()));
        let rel = format!("../{}", escaped.file_name().unwrap().to_str().unwrap());
        let err = write_text_in(&root, &rel, "x", &base, true)
            .expect_err("越界写入必须被拦截");
        assert!(
            err.contains("不在项目工作区"),
            "错误应提示越界,实际: {err}"
        );
        assert!(!escaped.exists(), "文件不应被创建");
        // 正常项目内写入仍可成功
        let ok = write_text_in(&root, "safe.txt", "ok", &base, true);
        assert!(ok.is_ok(), "项目内写入应成功");
        // 精确编辑同样受限
        let outside = root
            .parent()
            .unwrap()
            .join(format!("maf-guard-edit-{}.txt", std::process::id()));
        std::fs::write(&outside, "outside").unwrap();
        let rel_out = format!("../{}", outside.file_name().unwrap().to_str().unwrap());
        assert!(
            edit_text_in(&root, &rel_out, "outside", "x", false, &base, true).is_err(),
            "越界编辑必须被拦截"
        );
        std::fs::remove_file(&outside).ok();
        std::fs::remove_file(&escaped).ok();
    }

    #[test]
    fn extreme_inputs_fail_gracefully() {
        let root = ws();
        // 空文件:返回空串,不是错误
        std::fs::write(root.join("empty.txt"), "").unwrap();
        assert_eq!(read_text_in(&root, "empty.txt", 0, 32_000).unwrap(), "");
        // 不存在 / 非法文件名 / 目标是目录:都必须是可读错误,不能 panic
        assert!(read_text_in(&root, "no-such.txt", 0, 10).is_err());
        assert!(read_text_in(&root, "bad?name.txt", 0, 10).is_err());
        std::fs::create_dir_all(root.join("sub")).unwrap();
        assert!(read_text_in(&root, "sub", 0, 10).is_err());
        // 路径含 NUL:拒绝而非崩溃
        assert!(read_text_in(&root, "a\u{0}b.txt", 0, 10).is_err());
    }

    #[test]
    fn chunked_write_handles_10mb_across_many_chunks() {
        let _g = chunk_guard();
        let root = ws();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        let chunk = "A".repeat(50_000); // 每片 50KB,共 200 片 = 10MB
        let st = write_chunk_begin_in(&root, "big10.txt", false, &tmp, None).unwrap();
        for _ in 0..200 {
            write_chunk_append(&st.write_id, &chunk).unwrap();
        }
        let fin = write_chunk_finish(&st.write_id).unwrap();
        assert_eq!(fin.bytes, 10_000_000);
        let body = std::fs::read_to_string(root.join("big10.txt")).unwrap();
        assert_eq!(body.len(), 10_000_000, "10MB 内容必须完整落盘");
        assert!(body.chars().all(|c| c == 'A'));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn chunked_writes_are_isolated_across_threads() {
        let _g = chunk_guard();
        let root = ws();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        std::thread::scope(|s| {
            for name in ["a", "b"] {
                let root = root.clone();
                let tmp = tmp.clone();
                s.spawn(move || {
                    let st = write_chunk_begin_in(&root, &format!("{name}.txt"), false, &tmp, None)
                        .expect("begin 应成功");
                    let chunk = name.repeat(1000);
                    for _ in 0..40 {
                        write_chunk_append(&st.write_id, &chunk).expect("append 应成功");
                    }
                    write_chunk_finish(&st.write_id).expect("finish 应成功");
                    let body = std::fs::read_to_string(root.join(format!("{name}.txt"))).unwrap();
                    assert_eq!(body, name.repeat(1000).repeat(40), "并发会话不得串写");
                });
            }
        });
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn chunked_write_supports_unicode_nested_path() {
        let _g = chunk_guard();
        let root = ws();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        let rel = "中文目录/深层结构/实验报告.md";
        let st = write_chunk_begin_in(&root, rel, false, &tmp, None).unwrap();
        let chunk = "# 标题\n正文🙂\n".repeat(100);
        for _ in 0..3 {
            write_chunk_append(&st.write_id, &chunk).unwrap();
        }
        write_chunk_finish(&st.write_id).unwrap();
        assert_eq!(
            read_text_in(&root, rel, 0, usize::MAX).unwrap(),
            chunk.repeat(3),
            "中文深层路径的分批写入必须可原样读回"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn chunk_begin_enforces_workspace_boundary() {
        let _g = chunk_guard();
        let root = ws();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        let err = write_chunk_begin_in(&root, "../outside-part.txt", true, &tmp, None)
            .expect_err("越界 begin 必须被拦截");
        assert!(err.contains("不在项目工作区"), "实际: {err}");
        assert!(!root.join("../outside-part.txt").exists());
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// P10:测试专用 —— 把写会话的创建时间回拨,模拟超时。
    fn backdate_chunk(write_id: &str, age: std::time::Duration) {
        if let Ok(mut map) = chunk_map().lock()
            && let Some(w) = map.get_mut(write_id)
        {
            w.created = std::time::SystemTime::now()
                .checked_sub(age)
                .unwrap_or(w.created);
        }
    }

    #[test]
    fn chunk_session_expires_after_ttl() {
        let _g = chunk_guard();
        let root = ws();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        let st = write_chunk_begin_in(&root, "ttl.txt", false, &tmp, None).unwrap();
        write_chunk_append(&st.write_id, "前段").unwrap();
        // 回拨到 TTL 之外
        backdate_chunk(
            &st.write_id,
            CHUNK_SESSION_TTL + std::time::Duration::from_secs(60),
        );
        let err = write_chunk_append(&st.write_id, "后段").expect_err("过期会话必须拒绝追加");
        assert!(err.contains("已超时"), "got: {err}");
        assert!(!write_chunk_known(&st.write_id), "过期会话必须失效");
        assert!(
            !tmp.join(format!("{}.part", st.write_id)).exists(),
            "过期临时分片应被清理"
        );
        assert!(write_chunk_finish(&st.write_id).is_err(), "过期会话不可收尾");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn chunk_sessions_per_owner_are_capped() {
        let _g = chunk_guard();
        let root = ws();
        let tmp = std::env::temp_dir().join(format!("maf-part-{}", uuid::Uuid::new_v4()));
        let sid = format!("owner-{}", uuid::Uuid::new_v4());
        let mut ids = Vec::new();
        for _ in 0..MAX_CHUNK_SESSIONS_PER_OWNER {
            let st = write_chunk_begin_in(&root, "cap.txt", false, &tmp, Some(&sid)).unwrap();
            ids.push(st.write_id);
        }
        let err = write_chunk_begin_in(&root, "cap2.txt", false, &tmp, Some(&sid))
            .expect_err("超过并发上限必须拒绝");
        assert!(err.contains("上限"), "got: {err}");
        for id in ids {
            write_chunk_finish(&id).unwrap();
        }
        // 收尾后名额释放,可以继续 begin
        let st = write_chunk_begin_in(&root, "cap3.txt", false, &tmp, Some(&sid)).unwrap();
        write_chunk_finish(&st.write_id).unwrap();
        std::fs::remove_dir_all(&tmp).ok();
    }
}
