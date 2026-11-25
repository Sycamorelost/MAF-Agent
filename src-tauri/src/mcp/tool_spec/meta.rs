//! 按需工具加载的元工具定义。
//!
//! 默认只下发核心工具,其余工具由模型在需要时调用 `tool_load` 加载;
//! 加载后写入 `session_tools`,本轮后续请求与下次对话都会带上。

use serde_json::json;

use super::tool;
use crate::provider::ToolDef;

pub(super) fn tool_load_def() -> ToolDef {
    tool(
        "tool_load",
        "按需加载更多工具。当前只下发了核心工具;需要其他能力时先调用本工具,加载后本轮即可继续调用。\
         tools 填组名或工具名(可多选):doc 文档检索/回看;docx 生成 Word;sheet 生成 Excel;\
         slides 演示文稿;media 图片生成/海报/OCR/图元;batch 大文件分批写;\
         files 删除、恢复、zip;sys 系统/进程/磁盘/网络;web 网页抓取;agent 子智能体委派;\
         外部 MCP 用 mcp:<server> 加载整台,或 mcp__<server>__<tool> 精确加载。\
         只在确有需要时加载。",
        json!({
            "type": "object",
            "properties": {
                "tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "组名或工具名,如 [\"export\", \"doc_generate\"]"
                }
            },
            "required": ["tools"]
        }),
    )
}
