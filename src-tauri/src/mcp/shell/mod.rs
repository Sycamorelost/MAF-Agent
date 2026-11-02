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
//!
//! 职责拆分:
//! - `policy.rs`:白名单/元字符/危险模式常量与 `validate`、args 解析;
//! - `exec.rs`:进程执行、超时、输出解码/截断/落盘与审计。

mod exec;
mod policy;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use exec::{ShellOutcome, run, run_unrestricted};
#[allow(unused_imports)]
pub use policy::{
    EXEC_TIMEOUT, FORBIDDEN_CMDS, GIT_READ_ONLY, PATH_CMDS, READ_ONLY_CMDS, TRUST_EXEC_TIMEOUT,
    dangerous_pattern, parse_shell_args, validate,
};
// 测试复用的内部 helper(不扩大为 pub)。
#[allow(unused_imports)]
pub(crate) use exec::{decode_output, resolve_cwd};
#[allow(unused_imports)]
pub(crate) use policy::MAX_OUTPUT_CHARS;
