//! MCP 安全层:防御性路径 / 命令校验,参考 OpenCode (22k⭐) 与 Cherry Studio CVE 教训。
//!
//! # 防御纵深
//! 1. **NUL/CR/LF 拒绝** —— 经典命令注入载体,Windows 上 NUL 还会截断路径
//! 2. **Windows 设备名黑名单** —— CON/PRN/AUX 等,防止 SMB/UNC 绕过后的内核异常
//! 3. **路径穿越拒绝** —— `..` 序列,canonicalize 后再用 starts_with 校验
//! 4. **禁止跟随符号链接逃逸** —— canonicalize 本身会解引用 symlink,天然防御

use std::path::{Path, PathBuf};

/// 允许的根目录列表 —— Server 配置必须显式提供;空列表 = 拒绝一切。
#[allow(dead_code)] // Phase 5e ChatStream filesystem 工具调用前校验路径
pub fn resolve_safe_path(allowed_dirs: &[PathBuf], input: &str) -> Result<PathBuf, String> {
    if input.is_empty() {
        return Err("path 不能为空".into());
    }
    // Layer 1:字节级防御 —— OpenCode 同款
    if input.contains('\0') {
        return Err("path 包含 NUL 字节,已拒绝".into());
    }
    if input.contains('\r') || input.contains('\n') {
        return Err("path 包含 CR/LF,已拒绝".into());
    }
    // 命令注入载体 —— `&`, `|`, `;` 在路径里毫无意义
    if input.contains('&') || input.contains('|') || input.contains(';') {
        return Err("path 包含 shell 注入字符,已拒绝".into());
    }

    // Layer 2:Windows 设备名 —— 大小写不敏感
    let lower = input.trim_start_matches("\\\\?\\").to_lowercase();
    for dev in ["con", "prn", "aux", "nul", "com1", "com2", "lpt1", "lpt2"] {
        if lower == dev || lower.starts_with(&format!("{dev}.")) {
            return Err(format!("path 使用了保留设备名 `{dev}`,已拒绝"));
        }
    }

    // Layer 3:canonicalize 解引用 symlink + 消去 `..` + 规范化
    let input_path = Path::new(input);
    let canonical = input_path
        .canonicalize()
        .map_err(|e| format!("path 无法规范化 `{}`: {e}", input_path.display()))?;

    if allowed_dirs.is_empty() {
        return Err("该 MCP server 未配置 allowed_dirs,已拒绝".into());
    }

    // Layer 4:starts_with 校验 —— 允许列表中的任一目录
    let ok = allowed_dirs.iter().any(|d| {
        // 先 canonicalize allowed_dir 自身(防止配置里也有 symlink)
        let dc = d.clone().canonicalize().unwrap_or_else(|_| d.clone());
        canonical.starts_with(&dc)
    });

    if !ok {
        return Err(format!(
            "path `{}` 不在允许目录内({} 个 allowed_dirs)",
            canonical.display(),
            allowed_dirs.len()
        ));
    }
    Ok(canonical)
}

/// 判断路径是否命中敏感文件黑名单 —— `@modelcontextprotocol/server-filesystem` 默认也屏蔽。
/// 命中则 read_file 允许读取但给出警告;write/delete 直接拒绝。
#[allow(dead_code)] // Phase 5e ChatStream write/delete 操作前检查
pub const SENSITIVE_PATTERNS: &[&str] = &[
    ".env",
    ".env.local",
    ".env.production",
    "*.pem",
    "*.key",
    "*id_rsa*",
    "*id_ed25519*",
    "*credentials*.json",
    "*secrets*",
    "*.pgpass",
];

#[allow(dead_code)] // Phase 5e ChatStream 检查 write/delete 前调
pub fn is_sensitive(path: &Path) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return false,
    };
    let lower = name.to_lowercase();
    for pat in SENSITIVE_PATTERNS {
        let p = pat.to_lowercase();
        let starts_star = p.starts_with('*');
        let ends_star = p.ends_with('*');
        match (starts_star, ends_star) {
            (true, true) => {
                // *xxx* → contains
                let core = p.trim_matches('*');
                if lower.contains(core) {
                    return true;
                }
            }
            (true, false) => {
                // *xxx → ends_with
                if lower.ends_with(p.trim_start_matches('*')) {
                    return true;
                }
            }
            (false, true) => {
                // xxx* → starts_with
                if lower.starts_with(p.trim_end_matches('*')) {
                    return true;
                }
            }
            (false, false) => {
                if lower == p || lower.contains(&p) {
                    return true;
                }
            }
        }
    }
    false
}

/// MCP 工具参数里视为路径的键(只校验这些键,避免误伤普通字符串参数)。
const PATH_KEYS: &[&str] = &[
    "path",
    "paths",
    "file_path",
    "filepath",
    "file",
    "directory",
    "dir",
    "target_path",
];

/// 统一预检 MCP 工具参数中的路径(M-2:让 allowed_dirs / deny_patterns 真正生效)。
/// - 命中敏感文件名 → 直接拒绝(无论是否配置目录);
/// - 配置了 allowed_dirs 时,路径必须落在其中某个目录内(相对路径按首个目录解析)。
pub fn validate_tool_paths(
    args: &serde_json::Value,
    allowed_dirs: &[PathBuf],
    deny_patterns: &[String],
) -> Result<(), String> {
    let mut paths: Vec<String> = Vec::new();
    if let Some(obj) = args.as_object() {
        for key in PATH_KEYS {
            match obj.get(*key) {
                Some(serde_json::Value::String(s)) => paths.push(s.clone()),
                Some(serde_json::Value::Array(a)) => {
                    paths.extend(a.iter().filter_map(|v| v.as_str().map(str::to_string)))
                }
                _ => {}
            }
        }
    }
    for p in paths {
        let name = Path::new(&p)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&p);
        if is_sensitive(Path::new(&p)) || deny_patterns.iter().any(|pat| glob_match(name, pat)) {
            return Err(format!("路径 `{p}` 命中敏感文件黑名单,已拒绝"));
        }
        if allowed_dirs.is_empty() {
            continue; // 未配置目录:交给 MCP server 自身策略
        }
        let candidate = if Path::new(&p).is_absolute() {
            PathBuf::from(&p)
        } else {
            allowed_dirs[0].join(&p)
        };
        let canonical = canonical_or_parent(&candidate)?;
        let in_scope = allowed_dirs.iter().any(|d| {
            d.canonicalize()
                .map(|dc| canonical.starts_with(&dc))
                .unwrap_or(false)
        });
        if !in_scope {
            return Err(format!("路径 `{p}` 不在允许目录内,已拒绝"));
        }
    }
    Ok(())
}

/// 规范化路径;目标尚不存在(写新文件)时规范化其父目录再拼接文件名。
fn canonical_or_parent(path: &Path) -> Result<PathBuf, String> {
    if let Ok(c) = path.canonicalize() {
        return Ok(c);
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("路径 `{}` 无法解析", path.display()))?;
    let name = path
        .file_name()
        .ok_or_else(|| format!("路径 `{}` 缺少文件名", path.display()))?;
    let pc = parent
        .canonicalize()
        .map_err(|e| format!("路径 `{}` 的父目录不可访问: {e}", path.display()))?;
    Ok(pc.join(name))
}

/// 文件名 glob 匹配(与 `is_sensitive` 同一套语义:* 前缀=后缀匹配,* 后缀=前缀匹配,*a*=包含)。
fn glob_match(name: &str, pattern: &str) -> bool {
    let n = name.to_lowercase();
    let p = pattern.to_lowercase();
    match (p.starts_with('*'), p.ends_with('*')) {
        (true, true) => n.contains(p.trim_matches('*')),
        (true, false) => n.ends_with(p.trim_start_matches('*')),
        (false, true) => n.starts_with(p.trim_end_matches('*')),
        (false, false) => n == p || n.contains(&p),
    }
}

/// 按字符(而非字节)截断,避免在 UTF-8 多字节字符中间切片导致 panic。
pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    let total = s.chars().count();
    if total <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push_str(&format!("…(已截断,原始 {total} 字符)"));
    out
}

/// 首尾保留截断:超长时保留头部与尾部(各一半),中间省略。
///
/// 与 `truncate_chars`(只留头部)的区别:命令输出里的编译错误、异常堆栈、
/// `[ERROR]` 几乎都在尾部,只留头部会让模型看不到真正的失败原因。
pub fn truncate_head_tail(s: &str, max_chars: usize) -> String {
    let total = s.chars().count();
    if total <= max_chars {
        return s.to_string();
    }
    let head_n = max_chars / 2;
    let tail_n = max_chars - head_n;
    let head: String = s.chars().take(head_n).collect();
    let tail: String = s.chars().skip(total - tail_n).collect();
    format!(
        "{head}\n…(省略中间 {} 字符)…\n{tail}",
        total - head_n - tail_n
    )
}

/// MCP HTTP 端点校验 —— 只允许 http/https;明文 http 仅限本机回环。
///
/// 远端 MCP server 会把工具调用的全部参数发出去,明文链路上等同裸奔;
/// 因此非回环地址必须走 https。URL 内嵌账号密码也一律拒绝(改走 headers)。
pub fn validate_server_url(url: &str) -> Result<(), String> {
    let raw = url.trim();
    if raw.is_empty() {
        return Err("HTTP 传输的 URL 不能为空".into());
    }
    let u = reqwest::Url::parse(raw).map_err(|e| format!("URL 无法解析: {e}"))?;
    match u.scheme() {
        "https" => {}
        "http" => {
            if !is_loopback_host(u.host_str()) {
                return Err(format!(
                    "明文 http 只允许本机回环地址(127.0.0.1 / localhost / [::1]),`{}` 请改用 https",
                    u.host_str().unwrap_or("")
                ));
            }
        }
        other => return Err(format!("不支持的协议 `{other}`,仅允许 http / https")),
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err("URL 不能内嵌账号密码;请改用 headers 传递令牌".into());
    }
    Ok(())
}

/// 本机回环判断(不含 `0.0.0.0`,那会暴露到局域网)。
fn is_loopback_host(host: Option<&str>) -> bool {
    match host {
        Some(h) => {
            let h = h.trim_start_matches('[').trim_end_matches(']');
            h.eq_ignore_ascii_case("localhost")
                || h == "::1"
                || h.parse::<std::net::Ipv4Addr>()
                    .map(|ip| ip.is_loopback())
                    .unwrap_or(false)
        }
        None => false,
    }
}

/// 这些请求头由传输层自己维护,用户配置覆盖会造成协议错乱或走私。
const RESERVED_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "accept",
    "content-type",
    "mcp-session-id",
    "mcp-protocol-version",
];

/// HTTP 传输附加请求头校验:名称必须是合法 header token,值不得含 CR/LF,禁止覆盖保留头。
pub fn validate_headers(headers: &std::collections::HashMap<String, String>) -> Result<(), String> {
    if headers.len() > 32 {
        return Err("请求头数量过多(上限 32)".into());
    }
    for (k, v) in headers {
        let name = http::header::HeaderName::from_bytes(k.trim().as_bytes())
            .map_err(|_| format!("非法请求头名称 `{k}`"))?;
        if RESERVED_HEADERS.contains(&name.as_str()) {
            return Err(format!("请求头 `{name}` 由传输层维护,不能自定义"));
        }
        if v.contains('\r') || v.contains('\n') || v.contains('\0') {
            return Err(format!("请求头 `{name}` 的值包含换行或 NUL,已拒绝"));
        }
        if v.len() > 8192 {
            return Err(format!("请求头 `{name}` 的值过长(上限 8KB)"));
        }
        http::header::HeaderValue::from_str(v)
            .map_err(|_| format!("请求头 `{name}` 的值含非法字符"))?;
    }
    Ok(())
}

/// MCP server 启动命令校验 —— 拒绝 shell / 脚本宿主,防止"配置即执行任意代码"。
/// 允许 npx / node / uvx / python 等 MCP 生态常用启动器;shell 包装器一律拒绝。
pub const FORBIDDEN_SERVER_CMDS: &[&str] = &[
    "cmd",
    "cmd.exe",
    "powershell",
    "powershell.exe",
    "pwsh",
    "pwsh.exe",
    "bash",
    "sh",
    "zsh",
    "cscript",
    "wscript",
    "mshta",
    "rundll32",
    "regsvr32",
    "wmic",
    "start",
    "explorer",
];

pub fn validate_server_command(cmd: &str) -> Result<(), String> {
    let name = std::path::Path::new(cmd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(cmd)
        .to_lowercase();
    if name.is_empty() {
        return Err("启动命令不能为空".into());
    }
    if FORBIDDEN_SERVER_CMDS.contains(&name.as_str()) {
        return Err(format!(
            "`{cmd}` 属于 shell/脚本宿主,不能作为 MCP server 启动命令;请填写具体程序(如 npx、node、uvx)"
        ));
    }
    Ok(())
}

/// Shell 命令白名单校验 —— Phase 6 自建 whitelist-shell 使用。
/// 返回 Ok(()) 表示放行;Err 带可读错误。
#[allow(dead_code)] // Phase 6 whitelist-shell 用
pub fn validate_command(allowed: &[String], cmd: &str) -> Result<(), String> {
    if cmd.is_empty() {
        return Err("命令为空".into());
    }
    // 基础防御
    if cmd.contains('\0') || cmd.contains('\r') || cmd.contains('\n') {
        return Err("命令包含非法字符".into());
    }
    // 必须在白名单里(大小写不敏感,trim 后匹配)
    let cmd_clean = cmd.trim();
    if !allowed.iter().any(|a| a.eq_ignore_ascii_case(cmd_clean)) {
        return Err(format!("命令 `{cmd_clean}` 不在白名单内"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(p: &str) -> PathBuf {
        PathBuf::from(p)
    }

    #[test]
    fn rejects_nul_byte() {
        let e = resolve_safe_path(&[d("C:\\tmp")], "C:\\tmp\\a\0b.txt");
        assert!(e.is_err_and(|s| s.contains("NUL")));
    }
    #[test]
    fn rejects_path_traversal() {
        // 用真实路径测试(canonicalize 要求文件存在),这里只测字节防御:
        assert!(resolve_safe_path(&[d("C:\\tmp")], "C:\\tmp\\..\\windows").is_err());
    }
    #[test]
    fn rejects_device_name() {
        let e = resolve_safe_path(&[d("C:\\tmp")], "C:\\tmp\\CON");
        assert!(e.is_err_and(|s| s.contains("CON")));
    }
    #[test]
    fn rejects_shell_injection_chars() {
        let e = resolve_safe_path(&[d("C:\\tmp")], "C:\\tmp\\a.txt; calc.exe");
        assert!(e.is_err());
    }
    #[test]
    fn validate_command_checks_whitelist() {
        let ok = validate_command(&["ls".into(), "pwd".into()], "ls");
        assert!(ok.is_ok());
        let bad = validate_command(&["ls".into()], "rm -rf /");
        assert!(bad.is_err());
    }
    #[test]
    fn is_sensitive_env_file() {
        assert!(is_sensitive(Path::new(".env")));
        assert!(is_sensitive(Path::new("id_rsa")));
        assert!(!is_sensitive(Path::new("readme.md")));
    }

    /// 回归:中文长输出按字符截断,不得 panic(H-4)。
    #[test]
    fn truncate_chars_handles_multibyte_boundary() {
        let s = "中文".repeat(2000); // 4000 字符 / 12000 字节
        let out = truncate_chars(&s, 100);
        assert!(out.starts_with(&"中文".repeat(50)));
        assert!(out.contains("已截断"));
        assert_eq!(truncate_chars("短", 10), "短");
    }

    /// 首尾截断:头部与尾部都要保留(报错信息通常在最末尾)。
    #[test]
    fn truncate_head_tail_keeps_both_ends() {
        let s = format!("HEAD{}TAIL", "中".repeat(1000));
        let out = truncate_head_tail(&s, 100);
        assert!(
            out.starts_with("HEAD"),
            "头部丢失:{}",
            &out[..20.min(out.len())]
        );
        assert!(out.trim_end().ends_with("TAIL"), "尾部丢失");
        assert!(out.contains("省略中间"));
        assert_eq!(truncate_head_tail("短", 10), "短");
    }

    #[test]
    fn rejects_shell_as_mcp_server_command() {
        for cmd in [
            "cmd",
            "cmd.exe",
            "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
            "bash",
        ] {
            assert!(validate_server_command(cmd).is_err(), "`{cmd}` 应被拒绝");
        }
        assert!(validate_server_command("npx").is_ok());
        assert!(validate_server_command("C:\\Program Files\\nodejs\\node.exe").is_ok());
    }

    #[test]
    fn tool_paths_enforce_scope_and_sensitivity() {
        use serde_json::json;
        // 敏感文件:无论目录配置如何都拒绝
        assert!(validate_tool_paths(&json!({ "path": ".env" }), &[], &[]).is_err());
        // 命中 deny_patterns
        let deny = vec!["*.pem".to_string()];
        assert!(validate_tool_paths(&json!({ "path": "a.key.pem" }), &[], &deny).is_err());
        // 越界绝对路径(允许目录取系统临时目录,越界目标取系统自带文件)
        let allowed = std::env::temp_dir();
        let outside = if cfg!(windows) {
            "C:\\Windows\\win.ini"
        } else {
            "/etc/hosts"
        };
        assert!(
            validate_tool_paths(
                &json!({ "path": outside }),
                std::slice::from_ref(&allowed),
                &[]
            )
            .is_err()
        );
        // 允许目录内的相对路径
        let cwd = std::env::current_dir().unwrap();
        assert!(
            validate_tool_paths(&json!({ "path": "." }), std::slice::from_ref(&cwd), &[]).is_ok()
        );
    }

    /// HTTP 端点:https 放行;明文 http 只允许回环;其它协议与非回环明文一律拒绝。
    #[test]
    fn http_endpoint_requires_tls_or_loopback() {
        assert!(validate_server_url("https://mcp.example.com/mcp").is_ok());
        assert!(validate_server_url("http://127.0.0.1:8931/mcp").is_ok());
        assert!(validate_server_url("http://localhost:3000").is_ok());
        assert!(validate_server_url("http://[::1]:8899/mcp").is_ok());

        assert!(validate_server_url("http://mcp.example.com/mcp").is_err());
        assert!(validate_server_url("http://192.168.1.10:8080/mcp").is_err());
        assert!(validate_server_url("http://0.0.0.0:8080").is_err());
        assert!(validate_server_url("ftp://example.com/mcp").is_err());
        assert!(validate_server_url("file:///c:/x").is_err());
        assert!(validate_server_url("").is_err());
        // URL 内嵌凭据:拒绝,避免令牌进入日志与审计
        assert!(validate_server_url("https://user:pass@mcp.example.com/mcp").is_err());
    }

    /// 自定义请求头:合法头放行;保留头、换行注入、非法名称一律拒绝。
    #[test]
    fn http_headers_reject_reserved_and_injection() {
        let mut ok = std::collections::HashMap::new();
        ok.insert("Authorization".to_string(), "Bearer abc".to_string());
        ok.insert("X-API-Key".to_string(), "k-1".to_string());
        assert!(validate_headers(&ok).is_ok());

        let mut reserved = std::collections::HashMap::new();
        reserved.insert("Content-Length".to_string(), "10".to_string());
        assert!(validate_headers(&reserved).is_err());

        let mut injected = std::collections::HashMap::new();
        injected.insert("X-A".to_string(), "a\r\nX-Evil: 1".to_string());
        assert!(validate_headers(&injected).is_err());

        let mut bad_name = std::collections::HashMap::new();
        bad_name.insert("Bad Header".to_string(), "v".to_string());
        assert!(validate_headers(&bad_name).is_err());
    }
}
