//! 内置 shell 执行内核。
//!
//! 安全模型(逐层):
//! 1. 命令必须在只读白名单内,解释器 / 脚本宿主一律拒绝;
//! 2. 命令名与参数禁止 shell 元字符(NUL/CR/LF/&/|/>/</^/%/!);
//! 3. 只读命令的参数不限目录、不设文件名黑名单(无沙箱);
//! 4. 执行带真实超时,超时直接终止子进程;
//! 5. 输出按字符安全截断(避免 UTF-8 边界 panic)并写审计。
//!
//! 注意:以上 1-3 层仅约束 `run()`(非完全信任模式)。审批策略为「完全信任」时
//! 由 `shell_whitelist_exec` / dispatcher 改走 `run_unrestricted`,只保留空命令拒绝、
//! 300s 超时、输出截断与审计 —— 白名单 / 解释器硬拒 / 元字符限制全部不生效。

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::mcp::safety::truncate_head_tail;

/// 不需要路径参数的只读命令。
pub const READ_ONLY_CMDS: &[&str] = &[
    "pwd", "date", "time", "echo", "hostname", "whoami", "uname", "df", "du", "free",
    // Windows cmd 内建:只读/无副作用(chcp 显示代码页、vol/ver 显示版本、set 列环境变量)
    "chcp", "vol", "ver", "set",
];

/// 接受路径参数的只读命令 —— 路径不限目录、不设文件名黑名单。
pub const PATH_CMDS: &[&str] = &[
    "ls",
    "dir",
    "cat",
    "type",
    "head",
    "tail",
    "find",
    "grep",
    "rg",
    "where",
    "where.exe",
];

/// git 只读子命令。
pub const GIT_READ_ONLY: &[&str] = &[
    "status",
    "log",
    "diff",
    "branch",
    "show",
    "remote",
    "stash list",
    "tag",
    "describe",
    "rev-parse",
    "name-rev",
    "merge-base",
    "for-each-ref",
    "ls-tree",
];

/// 解释器 / 脚本宿主 / shell —— 非完全信任模式下一律硬拒;
/// 完全信任模式走 `run_unrestricted`,不经过本表校验(见文件头注释)。
pub const FORBIDDEN_CMDS: &[&str] = &[
    "python",
    "python3",
    "py",
    "node",
    "nodejs",
    "deno",
    "bun",
    "perl",
    "ruby",
    "php",
    "powershell",
    "pwsh",
    "cmd",
    "cmd.exe",
    "bash",
    "sh",
    "zsh",
    "cscript",
    "wscript",
    "mshta",
    "rundll32",
    "regsvr32",
];

/// 命令名 / 参数中的 shell 元字符。
const METACHARS: &[char] = &['&', '|', '>', '<', '^', '%', '!', '\n', '\r'];

/// 高风险命令模式(完全信任模式下也需二次确认):系统破坏 / 数据丢失 / 不可逆操作。
/// 匹配命令名(忽略大小写)或整行命令中的关键片段。
const DANGEROUS_PATTERNS: &[&str] = &[
    "format ",  // 格式化磁盘
    "rm -rf /", // Linux 删根
    "rm -rf /*",
    "rm -rf ~", // 删家目录
    "rm -rf $",
    "del /q /s", // Windows 批量静默删除
    "del /f /s /q",
    "rmdir /s /q",     // Windows 递归删目录
    "shutdown",        // 关机/重启
    "reg delete",      // 删注册表
    "reg add",         // 写注册表(同样危险)
    "diskpart",        // 磁盘分区
    "cipher /w",       // 安全擦除空闲空间
    "chkdsk /f",       // 强制检查并修复(可能丢数据)
    "fsutil",          // 文件系统底层操作
    "bcdedit",         // 启动配置
    "taskkill /f /im", // 强制杀进程(批量)
];

/// 执行超时。
pub const EXEC_TIMEOUT: Duration = Duration::from_secs(15);

/// 完全信任模式下的执行超时 —— 下载/安装类操作耗时长,放宽到 5 分钟。
pub const TRUST_EXEC_TIMEOUT: Duration = Duration::from_secs(300);

/// 单次输出上限(字符数)。
const MAX_OUTPUT_CHARS: usize = 32_000;

/// 一次命令执行的结果 —— 工具层据此判定成功/失败,并把回显与输出交给模型。
#[derive(Debug, Clone)]
pub struct ShellOutcome {
    /// 首行回显(`[exit=N · 耗时 · cwd] 实际命令行`)+ 输出正文
    pub text: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
}

impl ShellOutcome {
    /// 仅「未超时且退出码为 0」算成功 —— 空输出但 exit≠0 必须判失败。
    pub fn success(&self) -> bool {
        !self.timed_out && self.exit_code == Some(0)
    }
}

/// 解析模型传来的 `args`:数组逐项转字符串;字符串按单个参数接收并给出告警,
/// 避免此前 `filter_map(as_str)` 静默丢参导致「命令跑错却仍报成功」。
pub fn parse_shell_args(v: Option<&serde_json::Value>) -> (Vec<String>, Option<String>) {
    use serde_json::Value;
    match v {
        None => (Vec::new(), None),
        Some(Value::Array(items)) => {
            let mut out = Vec::with_capacity(items.len());
            let mut dropped = false;
            for item in items {
                match item {
                    Value::String(s) => out.push(s.clone()),
                    Value::Number(n) => out.push(n.to_string()),
                    Value::Bool(b) => out.push(b.to_string()),
                    Value::Null => dropped = true,
                    _ => dropped = true,
                }
            }
            let warn =
                dropped.then(|| "args 中的空值/嵌套对象已忽略;args 只能是字符串数组".to_string());
            (out, warn)
        }
        Some(Value::String(s)) => {
            if s.trim().is_empty() {
                (Vec::new(), None)
            } else {
                (
                    vec![s.clone()],
                    Some("args 以字符串传入,已按单个参数处理;多个参数请改成字符串数组".to_string()),
                )
            }
        }
        Some(_) => (
            Vec::new(),
            Some("args 类型非法(需字符串数组),已按无参数执行".to_string()),
        ),
    }
}

/// 校验 + 执行白名单命令。
/// 不限目录:`cwd` 可为任意已存在目录(缺省为工作区根),路径参数不限目录、不设文件黑名单。
pub async fn run(cmd: &str, args: &[String], cwd: Option<&str>) -> Result<ShellOutcome, String> {
    let root = crate::fs::paths::workspace_root()?;
    validate(cmd, args, &root)?;
    let dir = resolve_cwd(cwd, &root)?;
    execute(cmd, args, EXEC_TIMEOUT, false, Some(&dir)).await
}

/// 完全信任模式(审批策略 = 完全访问):跳过白名单/解释器/元字符校验,直接执行任意命令。
/// 仍保留:空命令拒绝、执行超时、输出上限、审计日志。仅由 shell_whitelist_exec 在
/// 确认策略为 Full 后调用,不得在其他路径使用。
///
/// 命令含空白或 shell 操作符时按「整行命令」走 `cmd /c`(仅 Windows):
/// 与 shell 心智一致,支持管道/重定向/内建命令(del/copy/mkdir/set…);
/// 纯程序名(无空白)仍走 argv 直 exec,保持精确语义。
/// `cwd` 缺省为工作区根;任意已存在目录均可使用(仍写审计)。
pub async fn run_unrestricted(
    cmd: &str,
    args: &[String],
    cwd: Option<&str>,
) -> Result<ShellOutcome, String> {
    let cmd_trimmed = cmd.trim();
    if cmd_trimmed.is_empty() {
        return Err("命令不能为空".into());
    }
    let root = crate::fs::paths::workspace_root()?;
    let dir = resolve_cwd(cwd, &root)?;
    let line_mode = needs_line_mode(cmd_trimmed);
    execute(cmd_trimmed, args, TRUST_EXEC_TIMEOUT, line_mode, Some(&dir)).await
}

/// 解析工作目录:缺省用工作区根,其余只要求绝对/相对路径指向已存在目录(不限位置)。
fn resolve_cwd(cwd: Option<&str>, root: &Path) -> Result<PathBuf, String> {
    let target = match cwd.map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) if Path::new(p).is_absolute() => PathBuf::from(p),
        Some(p) => root.join(p),
        None => root.to_path_buf(),
    };
    let canonical = target.canonicalize().map_err(|e| {
        format!(
            "工作目录不存在或不可访问 `{}`: {e}\n[提示] 省略 cwd 使用默认工作区,或传入已存在的目录",
            display_path(&target)
        )
    })?;
    if !canonical.is_dir() {
        return Err(format!("cwd `{}` 不是目录", display_path(&target)));
    }
    Ok(canonical)
}

/// 去掉 canonicalize 产生的 `\\?\` 前缀,便于回显与审计阅读。
fn display_path(p: &Path) -> String {
    let s = p.display().to_string();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

/// 输出解码:先按严格 UTF-8,失败再按 GBK(中文 Windows 控制台 CP936)。
/// 顺序不可反转 —— 否则 UTF-8 文本会被误判为 GBK 乱码。
fn decode_output(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let (text, _, _) = encoding_rs::GBK.decode(bytes);
            text.into_owned()
        }
    }
}

/// 是否需要按整行命令交给 shell:含空白或 shell 操作符(管道/重定向/串联/变量)。
fn needs_line_mode(cmd: &str) -> bool {
    cmd.chars()
        .any(|c| c.is_whitespace() || matches!(c, '|' | '>' | '<' | '&' | '^' | '%' | '!'))
}

/// 危险命令检测(完全信任模式下也需二次确认)。
/// 把 cmd + args 拼成整行(空格分隔),忽略大小写匹配 DANGEROUS_PATTERNS。
/// 返回命中的危险模式描述,供前端展示确认理由。
pub fn dangerous_pattern(cmd: &str, args: &[String]) -> Option<&'static str> {
    let line = if args.is_empty() {
        cmd.trim().to_lowercase()
    } else {
        format!("{} {}", cmd.trim(), args.join(" ")).to_lowercase()
    };
    DANGEROUS_PATTERNS
        .iter()
        .copied()
        .find(|p| line.contains(p))
}

/// 纯校验(不执行),便于单测与防御性复核。`_root` 仅为兼容调用方保留,不再用于限制路径。
pub fn validate(cmd: &str, args: &[String], _root: &Path) -> Result<(), String> {
    let cmd_lower = cmd.trim().to_lowercase();
    if cmd_lower.is_empty() {
        return Err("命令不能为空".into());
    }
    // Layer 1:解释器 / shell 硬拒绝(优先级最高,不受审批模式影响)
    if FORBIDDEN_CMDS.contains(&cmd_lower.as_str()) {
        return Err(format!(
            "`{cmd}` 属于解释器/脚本宿主,禁止通过内置命令执行;如需运行请使用 MCP 工具并逐次审批\n\
             [提示] 当前为受限模式;确需运行解释器,请在 设置 → 授权 → 审批模式 切换为「完全信任」"
        ));
    }
    // Layer 2:元字符
    if cmd.contains(METACHARS) {
        return Err(
            "命令名包含 shell 元字符,已拒绝\n\
             [提示] 受限模式不支持管道/重定向/变量展开;请在「完全信任」模式下把整行命令写入 command,args 留空"
                .into(),
        );
    }
    if let Some(bad) = args.iter().find(|a| a.contains(METACHARS)) {
        return Err(format!(
            "参数包含 shell 元字符,已拒绝:`{bad}`\n\
             [提示] 受限模式不支持管道/重定向/变量展开;请在「完全信任」模式下把整行命令写入 command,args 留空"
        ));
    }
    // Layer 3:设备名
    if matches!(cmd_lower.as_str(), "con" | "prn" | "aux" | "nul") {
        return Err("使用保留设备名被拒绝".into());
    }
    // Layer 4:白名单分流
    if READ_ONLY_CMDS.contains(&cmd_lower.as_str()) {
        return Ok(());
    }
    if cmd_lower == "git" {
        let sub = args.first().map(String::as_str).unwrap_or("");
        if !GIT_READ_ONLY.contains(&sub) {
            return Err(format!(
                "git 子命令 `{sub}` 被禁止(仅允许 status/log/diff/branch/show/remote 等只读)"
            ));
        }
        return Ok(());
    }
    if PATH_CMDS.contains(&cmd_lower.as_str()) {
        return Ok(());
    }
    Err(format!(
        "命令 `{cmd}` 不在白名单内\n\
         [提示] 受限模式仅允许只读命令(git status/log/diff、dir、ls、type、where、grep 等),且 command 只能写程序名、参数必须放进 args;\
         如需执行该命令或使用整行命令(管道/重定向/内建命令),请在 设置 → 授权 → 审批模式 切换为「完全信任」"
    ))
}

/// 执行(已通过校验或为信任模式)。
/// `line_mode = true` 时把 cmd(+args)当作整行命令交给 shell;否则按 argv 直接 exec。
async fn execute(
    cmd: &str,
    args: &[String],
    exec_timeout: Duration,
    line_mode: bool,
    cwd: Option<&Path>,
) -> Result<ShellOutcome, String> {
    use tokio::process::Command;

    let cmd_lower = cmd.to_lowercase();
    let is_builtin = matches!(
        cmd_lower.as_str(),
        "dir" | "type" | "echo" | "cd" | "where" | "set" | "chcp" | "vol" | "ver"
    );
    let line = compose_line(cmd, args);

    let mut command = if line_mode {
        shell_command(&line)
    } else if is_builtin && cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/c").arg(cmd);
        c
    } else {
        Command::new(cmd)
    };
    if !line_mode {
        command.args(args);
    }
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    command
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null());

    let cwd_display = cwd
        .map(display_path)
        .or_else(|| std::env::current_dir().ok().map(|p| display_path(&p)))
        .unwrap_or_else(|| "?".into());
    let dir = crate::paths::data_dir();
    let started = std::time::Instant::now();
    let output = match tokio::time::timeout(exec_timeout, command.output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            crate::mcp::audit(
                &dir,
                "shell_exec",
                "whitelist-shell",
                cmd,
                &format!(
                    "ok=false stage=spawn exit=- ms={} line={line} cwd={cwd_display} err={e}",
                    started.elapsed().as_millis()
                ),
            );
            return Err(format!(
                "启动命令失败: {e}\n[cwd] {cwd_display}\n[cmd] {line}\n\
                 [提示] command 只能是程序名(如 git、dir、where),参数必须放进 args;\
                 需要管道/重定向/内建命令(del/copy/mkdir/set…)时,请在「完全信任」模式下把整行命令写入 command 且 args 留空"
            ));
        }
        Err(_) => {
            let ms = started.elapsed().as_millis();
            crate::mcp::audit(
                &dir,
                "shell_exec",
                "whitelist-shell",
                cmd,
                &format!("ok=false stage=timeout exit=- ms={ms} line={line} cwd={cwd_display}"),
            );
            return Err(format!(
                "命令执行超时({}s),已终止\n[cmd] {line}\n\
                 [提示] 该命令可能在等待交互输入(如 more、确认框),请改用非交互参数;长任务请拆成更短的命令",
                exec_timeout.as_secs()
            ));
        }
    };

    let duration_ms = started.elapsed().as_millis();
    let stdout = decode_output(&output.stdout);
    let stderr = decode_output(&output.stderr);
    let full = match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (true, true) => "(无输出)".to_string(),
        (false, true) => stdout,
        (true, false) => format!("[stderr] {stderr}"),
        (false, false) => format!("{stdout}\n[stderr] {stderr}"),
    };
    let body = truncate_head_tail(&full, MAX_OUTPUT_CHARS);
    let exit = output.status.code();
    let exit_label = exit.map(|c| c.to_string()).unwrap_or_else(|| "?".into());
    let mut text =
        format!("[exit={exit_label} · {duration_ms}ms · cwd={cwd_display}] {line}\n{body}");
    if exit != Some(0) {
        text.push_str(&format!(
            "\n[提示] 退出码 {exit_label} 表示命令失败:先读 stderr 定位原因;\
             同一命令连续失败 3 次请更换方案,不要重复重试"
        ));
    }

    crate::mcp::audit(
        &dir,
        "shell_exec",
        "whitelist-shell",
        cmd,
        &format!(
            "ok={} exit={exit_label} ms={duration_ms} line={line} cwd={cwd_display}",
            exit == Some(0)
        ),
    );
    Ok(ShellOutcome {
        text,
        exit_code: exit,
        timed_out: false,
    })
}

/// 整行命令交给系统 shell:Windows 用 `cmd /c`(raw_arg 避免 Rust 引号与 cmd 解析冲突),
/// 其余平台用 `sh -c`。
#[cfg(windows)]
fn shell_command(line: &str) -> tokio::process::Command {
    let mut c = tokio::process::Command::new("cmd");
    c.raw_arg(format!("/c {line}"));
    c
}

#[cfg(not(windows))]
fn shell_command(line: &str) -> tokio::process::Command {
    let mut c = tokio::process::Command::new("sh");
    c.arg("-c").arg(line);
    c
}

/// 把 cmd + args 拼成可读命令行(用于回显与审计;含空白/元字符的参数加双引号)。
fn compose_line(cmd: &str, args: &[String]) -> String {
    let mut out = cmd.to_string();
    for a in args {
        out.push(' ');
        let needs_quote = a.is_empty()
            || a.chars().any(|c| {
                c.is_whitespace() || matches!(c, '"' | '&' | '|' | '<' | '>' | '^' | '%' | '!')
            });
        if needs_quote {
            out.push('"');
            out.push_str(&a.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(a);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unrestricted_executes_without_whitelist() {
        // 信任模式不校验白名单/解释器,直接执行;用 echo 验证执行通路
        let out = run_unrestricted("echo", &["trust-ok".to_string()], None)
            .await
            .expect("echo 必须执行成功");
        assert!(out.text.contains("trust-ok"), "实际输出:{}", out.text);
        assert!(out.success());
    }

    #[tokio::test]
    async fn unrestricted_still_rejects_empty_command() {
        assert!(run_unrestricted("   ", &[], None).await.is_err());
    }

    #[tokio::test]
    async fn unrestricted_supports_full_line_command() {
        // 整行命令(含空格)→ cmd /c:此前必然 program not found
        let out = run_unrestricted("echo line-mode-ok", &[], None)
            .await
            .expect("整行命令必须可执行");
        assert!(out.text.contains("line-mode-ok"), "实际输出:{}", out.text);
        assert!(out.success());
    }

    #[tokio::test]
    async fn non_zero_exit_is_reported_as_failure() {
        let out = run_unrestricted("cmd /c exit 3", &[], None)
            .await
            .expect("进程本身能启动");
        assert_eq!(out.exit_code, Some(3), "实际:{}", out.text);
        assert!(!out.success(), "非零退出必须判失败");
        assert!(out.text.contains("[exit=3"), "实际输出:{}", out.text);
    }

    #[tokio::test]
    async fn empty_output_is_marked_explicitly() {
        let out = run_unrestricted("cmd /c exit 0", &[], None)
            .await
            .expect("进程本身能启动");
        assert!(out.text.contains("(无输出)"), "实际输出:{}", out.text);
        assert!(out.success());
    }

    #[test]
    fn parse_shell_args_coerces_types() {
        use serde_json::json;
        let (a, w) = parse_shell_args(Some(&json!(["status", "-s"])));
        assert_eq!(a, vec!["status", "-s"]);
        assert!(w.is_none());

        // 字符串形式不再静默丢弃
        let (a, w) = parse_shell_args(Some(&json!("status --short")));
        assert_eq!(a, vec!["status --short"]);
        assert!(w.is_some());

        // 数字按字符串接收
        let (a, _) = parse_shell_args(Some(&json!(["head", 20])));
        assert_eq!(a, vec!["head", "20"]);

        let (a, w) = parse_shell_args(None);
        assert!(a.is_empty() && w.is_none());
    }

    #[test]
    fn cwd_defaults_to_workspace_and_allows_any_existing_dir() {
        let root = crate::fs::paths::workspace_root().expect("工作区根可用");
        // 缺省 → 工作区根(与 fs_* 对齐)
        let dir = resolve_cwd(None, &root).expect("默认 cwd 必须是工作区根");
        assert_eq!(dir, root);

        // 工作区内子目录
        let sub = root.join("cwd-test");
        std::fs::create_dir_all(&sub).expect("创建测试子目录");
        let dir = resolve_cwd(Some("cwd-test"), &root).expect("工作区内子目录应允许");
        assert!(dir.ends_with("cwd-test"));
        std::fs::remove_dir_all(&sub).ok();

        // 工作区外:不再设沙箱,任意已存在目录都允许
        let outside = std::env::temp_dir();
        let outside = outside.to_str().expect("临时目录可转字符串");
        assert!(
            resolve_cwd(Some(outside), &root).is_ok(),
            "无沙箱后工作区外 cwd 应允许"
        );
    }

    #[test]
    fn decode_output_falls_back_to_gbk() {
        // UTF-8 优先:中文 UTF-8 必须原样解出
        assert_eq!(decode_output("中文".as_bytes()), "中文");
        // GBK(CP936):中 = D6 D0,文 = CE C4
        assert_eq!(decode_output(&[0xD6, 0xD0, 0xCE, 0xC4]), "中文");
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn rejects_interpreters_even_without_metachars() {
        for cmd in ["python", "node", "pwsh", "cmd"] {
            let err = validate(cmd, &args(&["-c", "print(1)"]), Path::new("."))
                .expect_err("解释器必须被拒绝");
            assert!(err.contains("解释器"), "实际错误:{err}");
        }
    }

    #[test]
    fn rejects_non_whitelisted() {
        assert!(validate("rm", &args(&["-rf", "/"]), Path::new(".")).is_err());
        assert!(validate("git", &args(&["push"]), Path::new(".")).is_err());
        assert!(validate("git", &args(&["status"]), Path::new(".")).is_ok());
    }

    #[test]
    fn rejects_metachars() {
        assert!(validate("dir", &args(&["&", "calc"]), Path::new(".")).is_err());
        assert!(validate("echo", &args(&["a > b"]), Path::new(".")).is_err());
    }

    #[test]
    fn allows_path_outside_root() {
        let root = std::env::current_dir().unwrap();
        let outside = if cfg!(windows) {
            "C:\\Windows\\win.ini"
        } else {
            "/etc/passwd"
        };
        // 无沙箱:工作区外的路径参数不再被拒绝
        assert!(validate("type", &args(&[outside]), &root).is_ok());
    }

    #[test]
    fn allows_dotenv_and_any_path_inside_root() {
        let root = std::env::current_dir().unwrap();
        // 已取消敏感文件黑名单:`.env` 等文件名不再被拦截
        assert!(validate("type", &args(&[".env"]), &root).is_ok());
    }

    #[test]
    fn allows_relative_path_inside_root() {
        let root = std::env::current_dir().unwrap();
        assert!(validate("ls", &args(&["."]), &root).is_ok());
        assert!(validate("grep", &args(&["TODO", "."]), &root).is_ok());
    }
}
