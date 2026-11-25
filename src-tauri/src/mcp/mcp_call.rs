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
pub fn handle_response(resp: CallToolResponse, data_dir: &std::path::Path) -> McpToolResult {
    match resp {
        CallToolResponse::Complete(result) => handle_complete(result, data_dir),
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

fn handle_complete(result: CallToolResult, data_dir: &std::path::Path) -> McpToolResult {
    let mut parts: Vec<String> = Vec::new();
    let mut budget = MAX_TOOL_TOTAL_CHARS;
    for c in result.content {
        let piece = match c {
            rmcp::model::ContentBlock::Text(t) => t.text,
            // P1-5:非文本块不再静默丢弃 —— 文本资源直接内联,二进制落盘并给出路径说明
            rmcp::model::ContentBlock::Resource(res) => match res.resource {
                rmcp::model::ResourceContents::TextResourceContents { uri, text, .. } => {
                    format!("[资源 {uri}]\n{text}")
                }
                rmcp::model::ResourceContents::BlobResourceContents {
                    uri,
                    mime_type,
                    blob,
                    ..
                } => {
                    let mime = mime_type.unwrap_or_else(|| "application/octet-stream".into());
                    spill_notice(data_dir, &blob, &mime, "资源", Some(&uri))
                }
                _ => "[资源(未知类型,未内联)]".to_string(),
            },
            rmcp::model::ContentBlock::Image(img) => {
                spill_notice(data_dir, &img.data, &img.mime_type, "图片", None)
            }
            rmcp::model::ContentBlock::Audio(a) => {
                spill_notice(data_dir, &a.data, &a.mime_type, "音频", None)
            }
            rmcp::model::ContentBlock::ResourceLink(r) => {
                let size = r.size.map(|s| format!(", {s} 字节")).unwrap_or_default();
                let mime = r.mime_type.unwrap_or_default();
                format!("[资源链接 {} <{}> {mime}{size}]", r.name, r.uri)
            }
            _ => "[不支持的 MCP 内容块(已跳过)]".to_string(),
        };
        if budget == 0 {
            parts.push("[…其余内容因超出单次上限已省略]".into());
            break;
        }
        let capped = crate::mcp::safety::truncate_chars(&piece, MAX_TOOL_TEXT_CHARS.min(budget));
        budget = budget.saturating_sub(capped.chars().count());
        parts.push(capped);
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

/// 二进制块(base64)落盘到 `mcp-results/`,返回给模型一条可解释的说明。
/// 解码失败也不静默:至少报告类型与体积。
fn spill_notice(
    data_dir: &std::path::Path,
    b64: &str,
    mime: &str,
    kind: &str,
    uri: Option<&str>,
) -> String {
    use base64::Engine as _;
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) else {
        return format!(
            "[{kind}结果 {mime} base64 ≈{} 字符,解码失败,未内联]",
            b64.len()
        );
    };
    let ext = mime.split('/').nth(1).unwrap_or("bin");
    let ext: String = ext
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect();
    let name = format!("{}.{ext}", uuid::Uuid::new_v4());
    let dir = data_dir.join("mcp-results");
    if std::fs::create_dir_all(&dir).is_err() || std::fs::write(dir.join(&name), &bytes).is_err() {
        return format!(
            "[{kind}结果 {mime} {} 字节,落盘失败,未内联]",
            bytes.len()
        );
    }
    let path = dir.join(&name);
    let src = uri.map(|u| format!(",来源 {u}")).unwrap_or_default();
    format!(
        "[{kind}结果 {mime} {} 字节已保存:{}{src}]\n\
         注意:模型无法直接查看二进制内容;需要细节时请让用户打开该文件,或改用能返回文本的工具。",
        bytes.len(),
        path.display()
    )
}
