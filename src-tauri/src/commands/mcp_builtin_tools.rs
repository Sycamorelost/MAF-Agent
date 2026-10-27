//! 内置工具(get_now / shell_exec / 工作区文件工具)的 function-calling 定义。
//!
//! 默认不暴露 —— 由设置页开关控制,避免向不支持 tools 字段的中转站发送请求导致 400。
//! 描述保持精简:工具定义每轮都会随请求下发,冗长描述会持续消耗 token 并拉高延迟。

use crate::provider::{ToolDef, ToolFunctionDef};
use serde_json::json;

/// 全部内置工具定义(仅当 `builtin_tools` 开关打开时随对话下发)。
pub fn builtin_tool_defs() -> Vec<ToolDef> {
    vec![
        tool(
            "get_now",
            "获取当前本地日期、时间、时区与星期。",
            json!({ "type": "object", "properties": {} }),
        ),
        // harness 任务清单:多步任务开始时分解、每完成一步立即更新;UI 实时展示进度
        tool(
            "todo_write",
            "替换式更新本会话任务清单(须提交完整列表)。多步任务开始时分解步骤,完成一步即改为 completed 并推进下一步;简单问答不用。",
            json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "description": "完整任务列表(整体替换)",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string", "description": "可省略,按序编号" },
                                "content": { "type": "string", "description": "任务内容" },
                                "status": { "type": "string", "enum": ["pending", "in_progress", "completed", "cancelled"] },
                                "priority": { "type": "string", "enum": ["high", "medium", "low"] }
                            },
                            "required": ["content"]
                        }
                    }
                },
                "required": ["items"]
            }),
        ),
        tool(
            "todo_read",
            "读取本会话任务清单。",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "delegate",
            "把独立子任务委派给专项智能体并等待结果。子智能体无工具、不可再委派;需读写文件或执行命令的子任务请自己做。",
            json!({
                "type": "object",
                "properties": {
                    "agent": { "type": "string", "description": "智能体名称或 id" },
                    "task": { "type": "string", "description": "子任务描述(子智能体看不到当前对话)" }
                },
                "required": ["agent", "task"]
            }),
        ),
        // G5 系统状态只读快照:概览 / 进程 / 磁盘 / 网络,免审批、只读、落审计
        tool(
            "sys_overview",
            "本机系统概览:OS、CPU 占用、内存/swap、开机时长。",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "sys_processes",
            "进程快照(pid/名称/内存/CPU),按内存降序。",
            json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "number", "description": "条数,默认 30,最大 100" }
                }
            }),
        ),
        tool(
            "sys_disks",
            "磁盘分区:挂载点、文件系统、总容量、可用空间。",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "sys_networks",
            "网络接口:名称、IP、累计收发流量。",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "shell_exec",
            "执行本机命令。command 填程序名(git/dir/where),参数放 args 数组;默认仅工作区内只读命令,解释器与写操作被拒绝。结果首行 [exit=N ...],N≠0 为失败。",
            json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "程序名;完全信任模式可传整行命令"
                    },
                    "args": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "参数(字符串数组)"
                    },
                    "cwd": {
                        "type": "string",
                        "description": "工作目录,缺省为工作区根"
                    }
                },
                "required": ["command"]
            }),
        ),
        // 工作区文件工具:相对路径 + 自动备份,详见 fs 模块
        tool(
            "fs_list",
            "列出工作区目录(相对路径,留空为根)。",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "相对路径,可留空" } }
            }),
        ),
        tool(
            "fs_read",
            "读取工作区文本文件(默认上限 20 万字符)。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "工作区相对路径" },
                    "max_chars": { "type": "number", "description": "读取上限(字符)" }
                },
                "required": ["path"]
            }),
        ),
        tool(
            "fs_write",
            "写入工作区文本文件(自动建目录,覆盖前自动备份)。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "工作区相对路径" },
                    "content": { "type": "string", "description": "文件内容" }
                },
                "required": ["path", "content"]
            }),
        ),
        tool(
            "fs_delete",
            "删除工作区文件(删除前自动备份)。",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        ),
        tool(
            "fs_export_docx",
            "导出 .docx 到工作区(正文按行分段)。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "目标相对路径,如 reports/a.docx" },
                    "title": { "type": "string" },
                    "body": { "type": "string" }
                },
                "required": ["path", "body"]
            }),
        ),
        tool(
            "fs_export_xlsx",
            "导出 .xlsx 到工作区(支持多工作表)。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "目标相对路径,如 tables/a.xlsx" },
                    "sheets": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "rows": { "type": "array", "items": { "type": "array", "items": { "type": "string" } } }
                            },
                            "required": ["name", "rows"]
                        }
                    }
                },
                "required": ["path", "sheets"]
            }),
        ),
        tool(
            "fs_restore",
            "按备份 id 恢复工作区文件(恢复前再备份,可撤销)。",
            json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "备份 id" } },
                "required": ["id"]
            }),
        ),
        tool(
            "file_extract",
            "提取工作区文件正文:docx/xlsx/pdf/pptx/epub/html/文本。",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "工作区相对路径" } },
                "required": ["path"]
            }),
        ),
        tool(
            "zip_list",
            "列出工作区 zip 压缩包内文件。",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        ),
        tool(
            "zip_extract",
            "把工作区 zip 解包到工作区目录(自动跳过越界条目与符号链接)。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "zip 相对路径" },
                    "dest": { "type": "string", "description": "目标目录相对路径" }
                },
                "required": ["path", "dest"]
            }),
        ),
    ]
}

fn tool(name: &str, desc: &str, params: serde_json::Value) -> ToolDef {
    ToolDef {
        r#type: "function".into(),
        function: ToolFunctionDef {
            name: name.into(),
            description: Some(desc.into()),
            parameters: Some(params),
        },
    }
}
