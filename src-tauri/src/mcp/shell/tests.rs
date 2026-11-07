use std::path::Path;

use super::*;

#[tokio::test]
async fn unrestricted_executes_without_whitelist() {
    // 信任模式不校验白名单/解释器,直接执行;用 echo 验证执行通路
    let out = run_unrestricted("echo", &["trust-ok".to_string()], None, None)
        .await
        .expect("echo 必须执行成功");
    assert!(out.text.contains("trust-ok"), "实际输出:{}", out.text);
    assert!(out.success());
}

#[tokio::test]
async fn unrestricted_still_rejects_empty_command() {
    assert!(run_unrestricted("   ", &[], None, None).await.is_err());
}

#[tokio::test]
async fn unrestricted_supports_full_line_command() {
    // 整行命令(含空格)→ cmd /c:此前必然 program not found
    let out = run_unrestricted("echo line-mode-ok", &[], None, None)
        .await
        .expect("整行命令必须可执行");
    assert!(out.text.contains("line-mode-ok"), "实际输出:{}", out.text);
    assert!(out.success());
}

#[tokio::test]
async fn non_zero_exit_is_reported_as_failure() {
    let out = run_unrestricted("cmd /c exit 3", &[], None, None)
        .await
        .expect("进程本身能启动");
    assert_eq!(out.exit_code, Some(3), "实际:{}", out.text);
    assert!(!out.success(), "非零退出必须判失败");
    assert!(out.text.contains("[exit=3"), "实际输出:{}", out.text);
}

#[tokio::test]
async fn empty_output_is_marked_explicitly() {
    let out = run_unrestricted("cmd /c exit 0", &[], None, None)
        .await
        .expect("进程本身能启动");
    assert!(out.text.contains("(无输出)"), "实际输出:{}", out.text);
    assert!(out.success());
}

#[tokio::test]
async fn huge_output_keeps_head_and_tail() {
    // 约 80KB 输出:必须保留头尾(尾部的错误/结果不能丢),中间标注省略
    let out = run_unrestricted(
        "cmd /c for /L %i in (1,1,8000) do @echo line%i",
        &[],
        None,
        None,
    )
    .await
    .expect("批量输出命令必须可执行");
    assert!(out.success(), "实际:{}", out.text);
    assert!(
        out.text.contains("line1"),
        "头部丢失:{}",
        out.text.chars().take(40).collect::<String>()
    );
    assert!(
        out.text.contains("line8000"),
        "尾部丢失(只保留头部会看不到最终结果)"
    );
    assert!(out.text.contains("省略中间"), "超限输出应显式标注省略");
    // P9:完整输出落盘,提示中给出可续读路径
    assert!(out.text.contains("[完整输出已保存]"), "实际:{}", out.text);
    let saved = out
        .text
        .lines()
        .find(|l| l.contains("[完整输出已保存]"))
        .and_then(|l| l.split("] ").nth(1))
        .map(|s| s.split('(').next().unwrap_or("").trim().to_string());
    if let Some(p) = saved {
        assert!(std::path::Path::new(&p).exists(), "落盘文件不存在: {p}");
    }
    // 正文本体受 MAX_OUTPUT_CHARS 约束;额外允许落盘路径提示(绝对路径长度)
    assert!(
        out.text.chars().count() <= MAX_OUTPUT_CHARS + 400,
        "输出必须被截断到上限附近,实际 {} 字符",
        out.text.chars().count()
    );
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
    let root = crate::fs::paths::default_workspace_root().expect("工作区根可用");
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
