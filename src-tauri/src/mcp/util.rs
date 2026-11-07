//! MCP 子进程辅助工具函数。
//!
//! 把 runtime.rs 里的纯逻辑工具函数抽出来,让 runtime.rs 专注于生命周期管理,
//! 避免超过体积守卫的 700 行上限。

/// === C1:构造增强 PATH ===
/// 把宿主 PATH + 常见用户安装路径(homebrew / cargo / npm / volta / local bin)拼接,
/// 避免 GUI 启动的子进程 PATH 过短找不到 npx / uvx / cargo / node 等。
pub fn build_enhanced_path() -> String {
    let mut dirs: Vec<String> = Vec::new();

    // 宿主原有 PATH 最优先(先插前面,保持原有顺序)
    if let Ok(path) = std::env::var("PATH") {
        dirs.push(path);
    }

    // === 常见用户安装路径 ===
    #[cfg(target_os = "macos")]
    {
        dirs.push("/opt/homebrew/bin".into());
        dirs.push("/usr/local/bin".into());
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(format!("{home}/.nvm/versions/node"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(format!("{home}/.local/bin"));
            dirs.push(format!("{home}/.cargo/bin"));
            dirs.push(format!("{home}/.npm-global/bin"));
            dirs.push(format!("{home}/.volta/bin"));
        }
    }

    #[cfg(not(target_os = "macos"))]
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(format!("{home}/.volta/bin"));
        dirs.push(format!("{home}/.cargo/bin"));
    }

    if let Ok(home) = std::env::var("HOME") {
        dirs.push(format!("{home}/.local/bin"));
    }

    #[cfg(windows)]
    if let Ok(home) = std::env::var("USERPROFILE") {
        dirs.push(format!("{home}/AppData/Roaming/npm"));
        dirs.push(format!("{home}/.cargo/bin"));
        dirs.push(format!("{home}/.volta/bin"));
    }

    #[cfg(not(target_os = "windows"))]
    {
        dirs.push("/usr/bin".into());
        dirs.push("/bin".into());
    }

    #[cfg(windows)]
    let sep = ';';
    #[cfg(not(windows))]
    let sep = ':';

    let mut seen = std::collections::HashSet::new();
    dirs.into_iter()
        .filter(|d| !d.is_empty())
        .filter(|d| seen.insert(d.clone()))
        .collect::<Vec<_>>()
        .join(sep.to_string().as_str())
}

/// C6:该环境变量名是否属于敏感信息(不应被 MCP 子进程继承)。
///
/// 规则:包含 `KEY` / `PASSWORD` / `SECRET` / `TOKEN`(大小写不敏感),或以 `MYAGENT_` 开头。
/// 只移除、不做 `env_clear()` —— Windows 子进程离开 `SystemRoot` / `PATHEXT` / `COMSPEC` 会启动失败。
pub fn is_sensitive_env_key(key: &str) -> bool {
    let k = key.to_ascii_uppercase();
    k.starts_with("MYAGENT_")
        || ["KEY", "PASSWORD", "SECRET", "TOKEN"]
            .iter()
            .any(|w| k.contains(w))
}

/// 检查命令是否可解析为可执行文件(mcp_doctor 用)。
/// Windows 按 PATH + .exe/.cmd/.bat/.com 扩展名解析;Unix 按 PATH 搜索裸名。
pub fn check_command_resolvable(command: &str) -> (bool, Option<String>) {
    use std::path::Path;
    #[cfg(windows)]
    {
        if command.contains('\\') || command.contains('/') {
            let p = Path::new(command);
            return (p.is_file(), Some(p.display().to_string()));
        }
        let dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        for ext in ["exe", "cmd", "bat", "com"] {
            for dir in &dirs {
                let cand = dir.join(format!("{command}.{ext}"));
                if cand.is_file() {
                    return (true, Some(cand.display().to_string()));
                }
            }
        }
        (false, None)
    }
    #[cfg(not(windows))]
    {
        let dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        for dir in &dirs {
            let cand = dir.join(command);
            if cand.is_file() {
                return (true, Some(cand.display().to_string()));
            }
        }
        (false, None)
    }
}

/// 构造 stdio MCP 子进程命令。
///
/// Windows 上 `Command::new("npx")` 必然失败 —— CreateProcess 只会自动补 `.exe`,
/// 而 npm 装的 npx 是 `npx.cmd`。这里先按 PATH + 常见扩展名解析出真实文件,
/// `.cmd/.bat` 再交给 `cmd.exe /C` 执行。
#[cfg(windows)]
pub fn build_command(
    command: &str,
    args: &[String],
) -> Result<tokio::process::Command, String> {
    // tokio::process::Command 在 Windows 上自带 raw_arg / creation_flags
    use tokio::process::Command;

    // 禁止子进程弹出黑色控制台窗口(npx/node/python 等)
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let (exe, needs_shell) = resolve_windows_command(command)?;
    if needs_shell {
        // 走 cmd.exe 时参数会二次解析,必须拒绝 shell 元字符
        if let Some(bad) = args.iter().find(|a| has_shell_metachar(a)) {
            return Err(format!("参数包含 shell 元字符,已拒绝: `{bad}`"));
        }
        let mut c = Command::new("cmd");
        c.raw_arg("/C");
        c.raw_arg(build_cmd_line(&exe, args));
        c.creation_flags(CREATE_NO_WINDOW);
        return Ok(c);
    }
    let mut c = Command::new(&exe);
    c.args(args);
    c.creation_flags(CREATE_NO_WINDOW);
    Ok(c)
}

#[cfg(not(windows))]
pub fn build_command(
    command: &str,
    args: &[String],
) -> Result<tokio::process::Command, String> {
    let mut c = tokio::process::Command::new(command);
    c.args(args);
    Ok(c)
}

/// 拼 `cmd /C` 的命令行:整条命令再包一层引号 `"\"exe\" \"a1\""`。
/// 不这么写的话,参数里带空格时 cmd 会剥掉最外层引号,把 `C:\Program` 当成命令。
#[cfg(windows)]
pub(crate) fn build_cmd_line(exe: &str, args: &[String]) -> String {
    let mut line = format!("\"\"{exe}\"");
    for a in args {
        line.push_str(" \"");
        line.push_str(a);
        line.push('"');
    }
    line.push('"');
    line
}

/// 按 PATH 与扩展名优先级解析命令;返回(真实路径, 是否需要 cmd.exe 启动)。
#[cfg(windows)]
fn resolve_windows_command(command: &str) -> Result<(String, bool), String> {
    use std::path::Path;

    let is_shell = |p: &Path| {
        matches!(
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .as_deref(),
            Some("cmd") | Some("bat")
        )
    };

    if command.contains('\\') || command.contains('/') {
        let p = Path::new(command);
        return if p.is_file() {
            Ok((command.to_string(), is_shell(p)))
        } else {
            Err(format!("命令不存在: `{command}`"))
        };
    }

    let dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    // .exe 优先(原生可执行),其次才是需要 shell 的 .cmd/.bat
    for ext in ["exe", "cmd", "bat", "com"] {
        for dir in &dirs {
            let cand = dir.join(format!("{command}.{ext}"));
            if cand.is_file() {
                return Ok((cand.display().to_string(), ext == "cmd" || ext == "bat"));
            }
        }
    }
    Err(format!(
        "找不到命令 `{command}`。请确认已安装 Node.js(自带 npx),或改用绝对路径。"
    ))
}

#[cfg(windows)]
fn has_shell_metachar(arg: &str) -> bool {
    arg.chars().any(|c| {
        matches!(
            c,
            '&' | '|' | '>' | '<' | '^' | '%' | '!' | '"' | '\n' | '\r'
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_env_keys_are_scrubbed() {
        for key in [
            "OPENAI_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "DB_PASSWORD",
            "client_secret",
            "MYAGENT_WORKSPACE",
            "myagent_data_dir",
        ] {
            assert!(is_sensitive_env_key(key), "应判定为敏感: {key}");
        }
        // 系统必需变量绝不能被移除,否则 Windows 子进程无法启动
        for key in ["PATH", "SystemRoot", "PATHEXT", "COMSPEC", "TEMP", "HOME", "USERPROFILE"] {
            assert!(!is_sensitive_env_key(key), "不得移除系统变量: {key}");
        }
    }

    #[test]
    fn enhanced_path_is_non_empty_and_deduplicated() {
        let p = build_enhanced_path();
        assert!(!p.is_empty());
        #[cfg(windows)]
        let sep = ';';
        #[cfg(not(windows))]
        let sep = ':';
        let parts: Vec<&str> = p.split(sep).collect();
        let mut seen = std::collections::HashSet::new();
        for part in &parts {
            assert!(!part.is_empty(), "不应出现空路径段");
            assert!(seen.insert(part.to_string()), "路径段重复: {part}");
        }
    }
}
