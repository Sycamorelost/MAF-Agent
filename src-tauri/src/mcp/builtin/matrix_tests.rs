//! 内置工具矩阵测试:注册表、审批矩阵、D3 工作区外读取、P1 高危 shell、GUI 合并。
//!
//! 与 `mod.rs` 的路由实现一一对应;新增工具时请同步更新此处清单。

use super::{
    builtin_needs_approval, dangerous_shell_pattern, execute_builtin, is_builtin_tool,
    reads_outside_workspace,
};
use serde_json::json;

/// R4 工具注册表单源:schema 定义的每个工具都必须有执行分支。
/// 新增工具时请同步更新此处清单与 `execute_builtin` 的路由。
#[test]
fn every_schema_tool_has_an_execution_handler() {
    // `execute_builtin` 的兜底返回「未知内置工具」;
    // 逐个用空参数调用,只要不命中兜底即说明有对应分支。
    for name in crate::mcp::tool_spec::ALL_BUILTIN_TOOL_NAMES.iter() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (content, _ok) = rt.block_on(execute_builtin(name, &json!({}), false, None, None));
        assert!(
            !content.contains("未知内置工具"),
            "工具 `{name}` 已在 schema 中定义,但 `execute_builtin` 缺少对应执行分支"
        );
    }
}

#[test]
fn builtin_names_are_recognized() {
    assert!(is_builtin_tool("get_now"));
    assert!(is_builtin_tool("shell_exec"));
    assert!(is_builtin_tool("fs_write"));
    assert!(is_builtin_tool("fs_export_xlsx"));
    assert!(is_builtin_tool("file_extract"));
    assert!(!is_builtin_tool("read_file"));
}

/// P1:高危 shell 命令必须被识别为「需人工确认」,且 `confirm=false/true` 都不影响判定。
#[test]
fn dangerous_shell_is_flagged_regardless_of_model_confirm() {
    // 命中危险片段:格式化 / 删根 / 关机 / 注册表
    for args in [
        json!({ "command": "format D:" }),
        json!({ "command": "shutdown", "args": ["/s"] }),
        json!({ "command": "reg", "args": ["delete", "HKLM\\x"] }),
        json!({ "command": "rm", "args": ["-rf", "/"] }),
    ] {
        assert!(
            dangerous_shell_pattern("builtin", "shell_exec", &args).is_some(),
            "高危命令未命中: {args}"
        );
    }
    // 模型的 confirm 参数不参与判定,也不应改变结果
    let with_confirm = json!({
        "command": "format D:",
        "confirm": true
    });
    assert!(dangerous_shell_pattern("builtin", "shell_exec", &with_confirm).is_some());
    // 普通命令不触发强制确认
    for args in [
        json!({ "command": "git", "args": ["status"] }),
        json!({ "command": "dir" }),
    ] {
        assert!(dangerous_shell_pattern("builtin", "shell_exec", &args).is_none());
    }
    // 非 shell 内置工具与外部 server 的同名工具不触发该规则(外部工具另有审批门控)
    assert!(dangerous_shell_pattern("builtin", "fs_read", &json!({})).is_none());
    assert!(
        dangerous_shell_pattern("mcp", "shell_exec", &json!({ "command": "shutdown" })).is_none()
    );
}

#[test]
fn gui_tools_join_the_builtin_matrix() {
    assert!(is_builtin_tool("ui_list_windows"));
    assert!(is_builtin_tool("ui_type_text"));
    // GUI 已并入完全信任总开关,执行期统一拒绝/放行,不再单独走审批
    assert!(!builtin_needs_approval("ui_invoke"));
    assert!(!builtin_needs_approval("ui_list_windows"));
}

#[test]
fn builtin_approval_matrix_is_conservative() {
    for name in [
        "shell_exec",
        "fs_write",
        "fs_edit",
        "fs_write_begin",
        "fs_write_chunk",
        "fs_write_finish",
        "fs_delete",
        "fs_restore",
        "fs_export_docx",
        "fs_export_xlsx",
        "zip_extract",
    ] {
        assert!(builtin_needs_approval(name), "`{name}` 必须审批");
    }
    for name in [
        "get_now",
        "fs_list",
        "fs_read",
        "file_extract",
        "zip_list",
        "web_fetch",
    ] {
        assert!(!builtin_needs_approval(name), "`{name}` 为只读,不应打扰");
    }
}

/// D3:只读工具指向工作区外的本机文件时才需要审批;相对路径与缺省路径仍免打扰。
#[test]
fn outside_workspace_reads_require_approval() {
    // 缺省 / 相对路径 = 工作区内,不强制审批
    assert!(!reads_outside_workspace("fs_list", &json!({}), None));
    assert!(!reads_outside_workspace(
        "fs_read",
        &json!({ "path": "notes/a.txt" }),
        None
    ));
    // 临时目录的绝对路径通常不在工作区 → 需要审批
    let outside = std::env::temp_dir().join(format!("maf-d3-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&outside).unwrap();
    assert!(reads_outside_workspace(
        "fs_read",
        &json!({ "path": outside.to_str().unwrap() }),
        None
    ));
    assert!(reads_outside_workspace(
        "fs_grep",
        &json!({ "pattern": "x", "path": outside.to_str().unwrap() }),
        None
    ));
    // 与文件无关的只读工具不参与该判定
    assert!(!reads_outside_workspace(
        "web_fetch",
        &json!({ "url": "https://example.com" }),
        None
    ));
    assert!(!reads_outside_workspace(
        "get_now",
        &json!({ "path": outside.to_str().unwrap() }),
        None
    ));
    std::fs::remove_dir_all(&outside).ok();
}
