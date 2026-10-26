//! 内置工具(get_now / shell_exec / 工作区文件工具)的 function-calling 定义。
//!
//! 默认不暴露 —— 由设置页开关控制,避免向不支持 tools 字段的中转站发送请求导致 400。

use crate::provider::{ToolDef, ToolFunctionDef};
use serde_json::json;

/// 全部内置工具定义(仅当 `builtin_tools` 开关打开时随对话下发)。
pub fn builtin_tool_defs() -> Vec<ToolDef> {
    vec![
        tool(
            "get_now",
            "获取当前本地日期与时间(含时区、星期)。需要「现在几点/今天几号」时直接调用。",
            json!({ "type": "object", "properties": {} }),
        ),
        // harness 任务清单:多步任务开始时分解、每完成一步立即更新;UI 实时展示进度
        tool(
            "todo_write",
            "维护本会话的任务清单(替换式,提交完整列表)。多步任务必须在开始时调用它分解步骤,\
             每完成一步立即把该项 status 改为 completed、把下一步改为 in_progress。\
             简单单轮问答不要使用。",
            json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "description": "完整任务列表(整体替换旧清单)",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string", "description": "任务 id,可省略(按序自动编号)" },
                                "content": { "type": "string", "description": "任务内容(一句话)" },
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
            "读取当前会话的任务清单,核对哪些步骤已完成、接下来做什么。",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "delegate",
            "把一个独立子任务委派给专项智能体执行并等待其结果返回。\
             适合:需要不同专业视角的子问题(如调研、翻译、代码审查、文案起草)。\
             子智能体没有工具、不能再委派;需要读文件/执行命令的子任务不要委派,直接自己做。",
            json!({
                "type": "object",
                "properties": {
                    "agent": { "type": "string", "description": "智能体名称或 id(如「翻译专家」)" },
                    "task": { "type": "string", "description": "子任务完整描述(子智能体看不到当前对话,需自包含)" }
                },
                "required": ["agent", "task"]
            }),
        ),
        // G5 系统状态只读快照:概览 / 进程 / 磁盘 / 网络,免审批、只读、落审计
        tool(
            "sys_overview",
            "获取本机系统概览:OS/内核/主机名、CPU 核数与占用率、内存与 swap 用量、开机时长。\
             需要「电脑配置/内存还剩多少/开机多久」时调用。",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "sys_processes",
            "获取进程快照(pid/名称/内存/CPU),按内存占用降序。\
             需要「哪些程序占内存/某个进程还在不在」时调用。",
            json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "number", "description": "返回条数,默认 30,最大 100" }
                }
            }),
        ),
        tool(
            "sys_disks",
            "获取磁盘分区快照:挂载点/文件系统/总容量/可用空间。需要「磁盘还剩多少空间」时调用。",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "sys_networks",
            "获取网络接口快照:接口名/IP 地址/累计收发流量。",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "shell_exec",
            "执行本机命令。command 填「程序名」(如 git、dir、where),参数放进 args 数组。\
             需要管道、重定向、内建命令(del/copy/mkdir/set…)或一整行命令时,把整行写入 command 并把 args 留空\
             (仅「完全信任 MCP」模式可用)。默认模式只允许工作区内只读命令,解释器与写操作会被拒绝。\
             默认工作目录与文件工具一致(工作区根);需要其他目录时传 cwd(受限模式须在工作区内,完全信任可为任意已存在目录)。\
             结果首行是 [exit=N · 耗时 · cwd]:N≠0 即失败,先读 stderr 再决定是否重试。",
            json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "程序名(如 git、ls、where);或一整行命令(仅完全信任模式)"
                    },
                    "args": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "命令参数(必须是字符串数组;不要传整行)"
                    },
                    "cwd": {
                        "type": "string",
                        "description": "工作目录(可选);缺省为工作区根,可传工作区相对路径或绝对路径"
                    }
                },
                "required": ["command"]
            }),
        ),
        // 工作区文件工具:相对路径 + 自动备份,详见 fs 模块
        tool(
            "fs_list",
            "列出工作区目录内容(路径为工作区相对路径,留空表示根目录)。",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "相对路径,可留空" } }
            }),
        ),
        tool(
            "fs_read",
            "读取工作区内的文本文件(默认最多 20 万字符)。",
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
            "写入文本文件到工作区(自动建目录;覆盖前自动备份,可用备份 id 回滚)。",
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
            "删除工作区内的文件(删除前自动备份)。",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        ),
        tool(
            "fs_export_docx",
            "把标题与正文导出为 .docx 到工作区(正文按行分段)。",
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
            "把二维数据导出为 .xlsx 到工作区(支持多个工作表)。",
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
            "按备份 id 恢复工作区文件(恢复前会对当前内容再备份,可撤销)。",
            json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "fs_write / fs_delete 返回的 backup_id" } },
                "required": ["id"]
            }),
        ),
        tool(
            "file_extract",
            "提取工作区文件正文:docx / xlsx / pdf / pptx / epub / html / 文本。",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "工作区相对路径" } },
                "required": ["path"]
            }),
        ),
        tool(
            "zip_list",
            "列出工作区内 zip 压缩包的文件清单。",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        ),
        tool(
            "zip_extract",
            "把工作区内的 zip 解包到工作区目录(自动跳过越界条目与符号链接)。",
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
