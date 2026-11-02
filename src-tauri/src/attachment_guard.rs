//! 附件安全闸(拖入文件夹 / 选择文件共用的权威校验)。
//!
//! 前端 `attachmentDrop` 的黑名单只负责即时提示;这里才是落盘前的最终拦截:
//! 1. 扩展名黑名单(取最后一段扩展名,`a.pdf.exe` 命中 `exe`);
//! 2. 可执行魔数嗅探(PE / ELF / Mach-O / Java class),防「改后缀绕过」。

/// 内置默认黑名单(小写、不含点),与前端 `DEFAULT_BLOCKED_EXT` 保持一致。
/// 刻意不含 `py` / `sh` / `bin` 等,避免误伤文本类代码附件。
pub const DEFAULT_BLOCKED_EXT: &[&str] = &[
    "exe", "dll", "scr", "com", "msi", "msp", "cpl", "sys", "drv", "bat", "cmd", "ps1", "psm1",
    "vbs", "vbe", "wsf", "wsh", "jse", "hta", "reg", "lnk", "url", "jar", "apk", "app",
];

/// 取扩展名(小写、不含点);`a.pdf.exe` → `exe`。
pub fn ext_lower(file_name: &str) -> String {
    match file_name.rsplit_once('.') {
        Some((_, ext)) => ext.to_ascii_lowercase(),
        None => String::new(),
    }
}

/// 命中黑名单时返回该扩展名。内置默认清单始终生效,用户配置只能追加,
/// 避免「清空配置即放行」可执行文件。
pub fn blocked_ext_of(file_name: &str, configured: &[String]) -> Option<String> {
    let ext = ext_lower(file_name);
    if ext.is_empty() {
        return None;
    }
    if DEFAULT_BLOCKED_EXT.iter().any(|e| *e == ext) {
        return Some(ext);
    }
    configured
        .iter()
        .any(|e| e.trim().trim_start_matches('.').eq_ignore_ascii_case(&ext))
        .then_some(ext)
}

/// 可执行文件魔数嗅探(只看文件头,不解析内容)。
fn looks_executable(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    let head = &bytes[..4];
    // Windows PE: "MZ"
    if head[0] == b'M' && head[1] == b'Z' {
        return true;
    }
    // ELF(Linux)
    if head == b"\x7fELF" {
        return true;
    }
    // Mach-O(32/64 位、双端序)与 Java class
    matches!(
        head,
        [0xFE, 0xED, 0xFA, 0xCE]
            | [0xCE, 0xFA, 0xED, 0xFE]
            | [0xFE, 0xED, 0xFA, 0xCF]
            | [0xCF, 0xFA, 0xED, 0xFE]
            | [0xCA, 0xFE, 0xBA, 0xBE]
    )
}

/// 附件落盘前的权威校验:扩展名黑名单 + 可执行魔数。
pub fn check_attachment_allowed(
    file_name: &str,
    bytes: &[u8],
    configured_ext: &[String],
) -> Result<(), String> {
    if let Some(ext) = blocked_ext_of(file_name, configured_ext) {
        return Err(format!(
            "附件类型受限(.{ext}):该类型文件禁止加入会话(可在 设置 · 存储位置 · 附件 调整黑名单)"
        ));
    }
    if looks_executable(bytes) {
        return Err(format!(
            "附件「{file_name}」内容为可执行程序(文件头校验未通过),已拒绝"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn ext_takes_last_segment() {
        assert_eq!(ext_lower("a.pdf.exe"), "exe");
        assert_eq!(ext_lower("REPORT.PDF"), "pdf");
        assert_eq!(ext_lower("noext"), "");
        assert_eq!(ext_lower("a.tar.gz"), "gz");
    }

    #[test]
    fn default_blocks_when_config_empty() {
        // 空配置也走内置清单
        assert_eq!(blocked_ext_of("run.EXE", &[]).as_deref(), Some("exe"));
        assert!(blocked_ext_of("note.md", &[]).is_none());
        // 不误伤文本类代码附件
        assert!(blocked_ext_of("script.sh", &[]).is_none());
        assert!(blocked_ext_of("main.py", &[]).is_none());
    }

    #[test]
    fn configured_list_is_normalized() {
        // 用户输入的 "." 前缀 / 大小写都要能命中
        let list = cfg(&[".7z", "RAR"]);
        assert_eq!(blocked_ext_of("a.7z", &list).as_deref(), Some("7z"));
        assert_eq!(blocked_ext_of("a.rar", &list).as_deref(), Some("rar"));
        assert!(blocked_ext_of("a.pdf", &list).is_none());
    }

    #[test]
    fn rejects_executable_magic_even_with_safe_ext() {
        let exe = b"MZ\x90\x00rest";
        assert!(check_attachment_allowed("looks.txt", exe, &[]).is_err());
        let elf = b"\x7fELF\x02\x01";
        assert!(check_attachment_allowed("looks.png", elf, &[]).is_err());
    }

    #[test]
    fn rejects_blocked_extension() {
        assert!(check_attachment_allowed("setup.exe", b"plain", &[]).is_err());
        // 用户清单只能追加,内置默认仍生效
        let list = cfg(&["iso"]);
        assert!(check_attachment_allowed("a.iso", b"plain", &list).is_err());
        assert!(check_attachment_allowed("setup.exe", b"plain", &list).is_err());
        assert!(check_attachment_allowed("note.md", b"# hi", &list).is_ok());
    }

    #[test]
    fn allows_normal_documents() {
        assert!(check_attachment_allowed("report.pdf", b"%PDF-1.7", &[]).is_ok());
        assert!(check_attachment_allowed("note.md", b"# hello", &[]).is_ok());
        assert!(check_attachment_allowed("pic.png", b"\x89PNG\r\n\x1a\n", &[]).is_ok());
        assert!(check_attachment_allowed("sheet.xlsx", b"PK\x03\x04", &[]).is_ok());
    }
}
