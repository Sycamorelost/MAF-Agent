//! shell_exec 执行分支:复用 `mcp::shell` 的校验与执行内核。
//!
//! 高危命令的人工确认已在审批层完成(见 `super::dangerous_shell_pattern` 与
//! `dispatcher::ask_approval`),本模块不再读取模型的 `confirm` 参数。

use serde_json::Value;

/// 执行 shell_exec。返回 (给模型的内容, 是否成功)。
pub(super) async fn exec(args: &Value, trust_shell: bool) -> (String, bool) {
    let cmd = args
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if cmd.is_empty() {
        return (
            "缺少 command 参数\n[提示] command 填程序名(如 git、ls、where),参数放进 args 数组".into(),
            false,
        );
    }
    // args 容错:字符串/数字不再静默丢弃(见 shell::parse_shell_args)
    let (argv, warn) = crate::mcp::shell::parse_shell_args(args.get("args"));
    let cwd = args
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let r = if trust_shell {
        crate::mcp::shell::run_unrestricted(cmd, &argv, cwd).await
    } else {
        crate::mcp::shell::run(cmd, &argv, cwd).await
    };
    match r {
        Ok(out) => {
            let ok = out.success();
            let mut text = out.text;
            if let Some(w) = warn {
                text.push_str(&format!("\n[warn] {w}"));
            }
            (text, ok)
        }
        Err(e) => (format!("命令被拒绝或执行失败:{e}"), false),
    }
}

#[cfg(test)]
mod tests {
    use super::exec;
    use serde_json::json;

    #[tokio::test]
    async fn shell_exec_rejects_non_whitelisted_command() {
        // 未通过白名单 → 必须返回失败,且不产生任何执行
        let (content, ok) = exec(&json!({ "command": "rm" }), false).await;
        assert!(!ok);
        assert!(content.contains("白名单") || content.contains("拒绝"));
    }

    /// H-1 回归:解释器必须被拒绝(即便参数不含任何元字符)。
    #[tokio::test]
    async fn shell_exec_rejects_interpreters() {
        let (content, ok) = exec(
            &json!({ "command": "python", "args": ["-c", "print(1)"] }),
            false,
        )
        .await;
        assert!(!ok);
        assert!(content.contains("解释器"), "实际内容:{content}");
    }

    #[tokio::test]
    async fn shell_exec_requires_command_argument() {
        let (content, ok) = exec(&json!({}), false).await;
        assert!(!ok);
        assert!(content.contains("command"));
    }

    #[tokio::test]
    async fn shell_exec_denies_git_write_subcommand() {
        // git 只读白名单:commit 必须被拒绝
        let (_content, ok) = exec(
            &json!({ "command": "git", "args": ["commit", "-m", "x"] }),
            false,
        )
        .await;
        assert!(!ok);
    }

    /// 命令行控制:白名单只读命令应真实执行并返回回显。
    #[tokio::test]
    async fn shell_exec_runs_whitelisted_echo() {
        let (content, ok) = exec(&json!({ "command": "echo", "args": ["mcp-ok"] }), false).await;
        assert!(ok, "echo 应执行成功,实际:{content}");
        assert!(content.contains("mcp-ok"), "应包含命令输出,实际:{content}");
    }
}
