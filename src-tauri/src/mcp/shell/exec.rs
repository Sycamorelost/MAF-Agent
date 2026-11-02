//! shell 执行:进程启动、超时、输出解码/截断/落盘与审计。

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::mcp::safety::truncate_head_tail;

use super::policy::{EXEC_TIMEOUT, MAX_OUTPUT_CHARS, TRUST_EXEC_TIMEOUT, validate};

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
pub(crate) fn resolve_cwd(cwd: Option<&str>, root: &Path) -> Result<PathBuf, String> {
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
pub(crate) fn decode_output(bytes: &[u8]) -> String {
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
    // P9:超长输出完整落盘到 data/tmp/cmd,正文仍保留首尾;模型可再用 fs_read 续读全文。
    let total_chars = full.chars().count();
    let body = if total_chars > MAX_OUTPUT_CHARS {
        let mut b = truncate_head_tail(&full, MAX_OUTPUT_CHARS);
        match save_full_output(&dir, &full) {
            Some(p) => b.push_str(&format!(
                "\n[完整输出已保存] {}({total_chars} 字符;可用 fs_read 按 offset 分页读取)",
                display_path(&p)
            )),
            None => b.push_str(&format!(
                "\n[完整输出落盘失败] 原始 {total_chars} 字符,此处仅保留首尾"
            )),
        }
        b
    } else {
        full
    };
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

/// P9:把完整命令输出写到 `data/tmp/cmd/<时间>-<随机>.log`,返回落盘路径。
/// 同一目录下超过 24 小时的旧日志在写入时顺手清理,避免无限堆积。
fn save_full_output(dir: &Path, text: &str) -> Option<PathBuf> {
    let sub = dir.join("tmp").join("cmd");
    std::fs::create_dir_all(&sub).ok()?;
    cleanup_cmd_logs(&sub);
    let name = format!(
        "{}-{}.log",
        chrono::Local::now().format("%Y%m%d-%H%M%S"),
        &uuid::Uuid::new_v4().to_string()[..8]
    );
    let path = sub.join(name);
    std::fs::write(&path, text).ok()?;
    Some(path)
}

fn cleanup_cmd_logs(sub: &Path) {
    let Ok(read) = std::fs::read_dir(sub) else {
        return;
    };
    let cutoff = std::time::SystemTime::now() - Duration::from_secs(24 * 3600);
    for entry in read.flatten() {
        let path = entry.path();
        if path.extension().and_then(|x| x.to_str()) != Some("log") {
            continue;
        }
        let expired = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| t < cutoff)
            .unwrap_or(false);
        if expired {
            let _ = std::fs::remove_file(&path);
        }
    }
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
