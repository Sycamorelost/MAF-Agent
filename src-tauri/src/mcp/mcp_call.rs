//! MCP 工具调用辅助函数。
//! 从 runtime.rs 抽出来,避免超过体积守卫 700 行上限。

use rmcp::model::{CallToolResponse, CallToolResult, Tool};
use serde::{Deserialize, Serialize};

/// 工具描述(返回给前端或 ChatStream 的轻量结构)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    /// rmcp Tool 的 input_schema(serde_json::Value 即 JSON Schema)。
    pub input_schema: serde_json::Value,
}

/// 工具调用请求(从 ChatStream 路由过来时的结构)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCall {
    pub server_name: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// 工具调用结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    pub ok: bool,
    pub content: String,
    pub error: Option<String>,
}

// === 长度上限常量 —— 工具调用返回正文超长时截断 ===
/// 单次工具调用输出总字符数上限;超过部分丢弃(MCP server 偶尔返回长报告)。
/// 值与 runtime.rs 原版保持一致,不要改。
pub const MAX_TOOL_TOTAL_CHARS: usize = 64_000;
/// 单段文本块的字符截断上限;配合 TOTAL 预算渐进截断。
pub const MAX_TOOL_TEXT_CHARS: usize = 32_000;

/// Tool → McpToolInfo 转换。
pub fn tool_to_info(t: Tool) -> McpToolInfo {
    McpToolInfo {
        name: t.name.into_owned(),
        description: t.description.map(|c| c.into_owned()).unwrap_or_default(),
        input_schema: serde_json::Value::Object(t.input_schema.as_ref().clone()),
    }
}

/// 把 rmcp CallToolResponse 转成稳定的 McpToolResult。
/// 使用项目实际 rmcp 版本的 variant(Complete / InputRequired / Task),
/// 不同 rmcp 版本的枚举形态不同,改版本时必须同步核对这里。
pub fn handle_response(resp: CallToolResponse) -> McpToolResult {
    match resp {
        CallToolResponse::Complete(result) => handle_complete(result),
        CallToolResponse::InputRequired(_) => McpToolResult {
            ok: false,
            content: String::new(),
            error: Some("server 需要额外输入(MRTR 扩展,暂不支持)".into()),
        },
        CallToolResponse::Task(_) => McpToolResult {
            ok: false,
            content: String::new(),
            error: Some("server 把调用 materialize 为 task,暂不支持".into()),
        },
        _ => McpToolResult {
            ok: false,
            content: String::new(),
            error: Some("未知 response 类型".into()),
        },
    }
}

fn handle_complete(result: CallToolResult) -> McpToolResult {
    let mut parts: Vec<String> = Vec::new();
    let mut budget = MAX_TOOL_TOTAL_CHARS;
    for c in result.content {
        if let rmcp::model::ContentBlock::Text(t) = c {
            if budget == 0 {
                break;
            }
            let capped =
                crate::mcp::safety::truncate_chars(&t.text, MAX_TOOL_TEXT_CHARS.min(budget));
            budget = budget.saturating_sub(capped.chars().count());
            parts.push(capped);
        }
    }
    let text = parts.join("\n");

    let is_err = result.is_error.unwrap_or(false);
    McpToolResult {
        ok: !is_err,
        content: text,
        error: if is_err {
            Some("server reported error".into())
        } else {
            None
        },
    }
}
