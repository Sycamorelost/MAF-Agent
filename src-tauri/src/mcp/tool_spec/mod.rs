//! 内置工具的 function-calling 定义(单源)。
//!
//! 从 `commands::mcp_builtin_tools` 上提到 `mcp` 层:工具名单由 schema 定义派生,
//! `mcp::builtin`(执行/审批)与 `commands`(IPC 下发)共同依赖本模块,
//! 避免执行层反向依赖命令层。
//!
//! R4/D2:拆成「只读」与「写入」两组。只读工具默认随对话下发(开箱可用,
//! 长文档的 doc_search/doc_read 不再依赖工具总开关);写入/执行类工具需同时打开
//! 「工具总开关」与「内置工具」开关,避免默认就把 shell 暴露给模型。
//! 描述保持精简:工具定义每轮都会随请求下发,冗长描述会持续消耗 token 并拉高延迟。

use std::collections::HashSet;
use std::sync::LazyLock;

use crate::provider::{ToolDef, ToolFunctionDef};

mod readonly;
mod write;

pub use readonly::readonly_tool_defs;
pub use write::write_tool_defs;

/// 全部内置工具名(只读 + 写入)。从 schema 定义单源派生,
/// 避免 `mcp::builtin` 再维护一份硬编码名单。
pub(crate) static ALL_BUILTIN_TOOL_NAMES: LazyLock<HashSet<String>> = LazyLock::new(|| {
    readonly_tool_defs()
        .into_iter()
        .chain(write_tool_defs())
        .map(|t| t.function.name)
        .collect()
});

/// 写入/执行类工具名(需审批)。从 `write_tool_defs` 派生。
pub(crate) static WRITE_TOOL_NAMES: LazyLock<HashSet<String>> = LazyLock::new(|| {
    write_tool_defs()
        .into_iter()
        .map(|t| t.function.name)
        .collect()
});

/// 全部内置工具定义(只读 + 写入)。供设置页展示与审计使用。
#[allow(dead_code)]
pub fn builtin_tool_defs() -> Vec<ToolDef> {
    let mut out = readonly_tool_defs();
    out.extend(write_tool_defs());
    out
}

/// 构造一个 OpenAI function-calling 工具定义。
pub(super) fn tool(name: &str, desc: &str, params: serde_json::Value) -> ToolDef {
    ToolDef {
        r#type: "function".into(),
        function: ToolFunctionDef {
            name: name.into(),
            description: Some(desc.into()),
            parameters: Some(params),
        },
    }
}
