//! Windows 下的启动命令解析回归测试(由 `runtime.rs` 的 `mod win_tests` 引入)。

/// Windows 上 `npx` 只有 .cmd,必须能被解析并经 cmd.exe 成功执行。
/// 本机没装 Node 时跳过。
#[tokio::test]
async fn npx_resolves_and_runs() {
    let mut cmd = match super::build_command("npx", &["--version".to_string()]) {
        Ok(c) => c,
        Err(_) => return, // 未安装 npx
    };
    let out = cmd.output().await.expect("spawn npx");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// 命令行必须整条再包一层引号,否则带空格的参数会让 cmd 误判。
#[test]
fn cmd_line_wraps_whole_command_in_quotes() {
    let line = super::build_cmd_line(
        "C:\\Program Files\\nodejs\\npx.cmd",
        &["-y".into(), "@scope/pkg".into(), "D:\\a b\\c".into()],
    );
    assert_eq!(
        line,
        "\"\"C:\\Program Files\\nodejs\\npx.cmd\" \"-y\" \"@scope/pkg\" \"D:\\a b\\c\"\""
    );
}

/// 脚本路径和参数都带空格时,真实启动仍要能原样传参。
#[tokio::test]
async fn cmd_script_keeps_spaced_args() {
    let dir = std::env::temp_dir().join("maf mcp quote test");
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("echo args.cmd");
    std::fs::write(&script, "@echo off\r\necho %1\r\n").unwrap();

    let mut cmd = super::build_command(
        script.to_string_lossy().as_ref(),
        &["D:\\a b\\c".to_string()],
    )
    .expect("build");
    let out = cmd.output().await.expect("spawn");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("D:\\a b\\c"), "stdout={stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// cmd.exe 路径下必须拒绝 shell 元字符。
#[test]
fn rejects_metachars_for_cmd_script() {
    let err = super::build_command("npx", &["a&calc".to_string()]);
    assert!(err.is_err(), "元字符参数应被拒绝");
}
