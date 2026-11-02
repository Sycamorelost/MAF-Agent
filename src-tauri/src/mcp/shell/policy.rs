//! shell 安全策略:白名单、元字符、危险模式与命令校验。

use std::path::Path;
use std::time::Duration;

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
/// 完全信任模式走 `run_unrestricted`,不经过本表校验(见模块头注释)。
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
pub(crate) const MAX_OUTPUT_CHARS: usize = 32_000;

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
