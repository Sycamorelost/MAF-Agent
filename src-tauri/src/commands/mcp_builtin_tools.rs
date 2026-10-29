//! 内置工具(get_now / shell_exec / 文件工具)的 function-calling 定义。
//!
//! R4/D2:拆成「只读」与「写入」两组。只读工具默认随对话下发(开箱可用,
//! 长文档的 doc_search/doc_read 不再依赖工具总开关);写入/执行类工具需同时打开
//! 「工具总开关」与「内置工具」开关,避免默认就把 shell 暴露给模型。
//! 描述保持精简:工具定义每轮都会随请求下发,冗长描述会持续消耗 token 并拉高延迟。

use std::collections::HashSet;
use std::sync::LazyLock;

use crate::provider::{ToolDef, ToolFunctionDef};
use serde_json::json;

/// 全部内置工具名(只读 + 写入)。从 schema 定义单源派生,
/// 避免 `mcp/builtin.rs` 再维护一份硬编码名单。
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

/// 只读内置工具:不改变磁盘与系统状态,默认下发,免审批。
pub fn readonly_tool_defs() -> Vec<ToolDef> {
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
        // 文件工具:path 支持绝对路径(本机任意位置),相对路径以工作区为基准;详见 fs 模块
        tool(
            "fs_list",
            "列出目录内容。path 省略时列默认工作区根;相对路径按工作区解析,也支持绝对路径如 D:\\资料。",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "目录路径,可留空;支持绝对路径" } }
            }),
        ),
        tool(
            "fs_read",
            "读取本机文本文件(默认 3.2 万字符,硬上限 5 万);path 支持绝对路径;大文件用 offset 续读。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "文件路径,支持绝对路径" },
                    "offset": { "type": "number", "description": "跳过前 N 个字符(分页续读)" },
                    "max_chars": { "type": "number", "description": "本次读取上限(字符,最大 5 万)" }
                },
                "required": ["path"]
            }),
        ),
        tool(
            "fs_grep",
            "在本机文本文件中检索关键词,返回「文件:行号:内容」;path 可为绝对路径或工作区相对路径(目录或单文件)。",
            json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "关键词(不区分大小写)" },
                    "path": { "type": "string", "description": "检索目录或文件,可留空;支持绝对路径" },
                    "limit": { "type": "number", "description": "最多返回条数,默认 50" }
                },
                "required": ["pattern"]
            }),
        ),
        tool(
            "file_extract",
            "提取本机文件正文:docx/xlsx/pdf/pptx/epub/html/文本;path 支持绝对路径。",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "文件路径,支持绝对路径" } },
                "required": ["path"]
            }),
        ),
        // 文档驻留层(R3 C1/C2):会话内已上传文档按块检索与回看,长文档不必全量注入
        tool(
            "doc_search",
            "在本会话已上传的文档中按块检索片段,返回块序号、页码、章节与命中片段;追问长文档细节时优先用它。",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "检索关键词" },
                    "doc": { "type": "string", "description": "限定文档名或 doc_id,可省略" },
                    "limit": { "type": "number", "description": "返回块数,默认 6,最大 20" }
                },
                "required": ["query"]
            }),
        ),
        tool(
            "doc_read",
            "按块序号读取本会话文档原文(带页码/章节);doc 省略且会话仅一份文档时自动选中。",
            json!({
                "type": "object",
                "properties": {
                    "doc": { "type": "string", "description": "文档名或 doc_id,可省略" },
                    "start": { "type": "number", "description": "起始块序号(0 起),默认 0" },
                    "count": { "type": "number", "description": "读取块数,默认 3" }
                }
            }),
        ),
        tool(
            "zip_list",
            "列出本机 zip 压缩包内文件;path 支持绝对路径。",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "zip 文件路径,支持绝对路径" } },
                "required": ["path"]
            }),
        ),
    ]
}

/// 写入/执行类内置工具:需「工具总开关 + 内置工具」同时打开才下发。
pub fn write_tool_defs() -> Vec<ToolDef> {
    vec![
        tool(
            "shell_exec",
            "执行本机命令。command 填程序名(git/dir/where),参数放 args 数组;默认仅只读命令白名单,路径不限目录,解释器与写操作被拒绝;系统级操作(环境变量/服务/注册表)需审批模式切到「完全信任」。结果首行 [exit=N ...],N≠0 为失败。",
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
        tool(
            "fs_write",
            "写入本机文本文件(自动建目录,覆盖前自动备份);path 支持绝对路径。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "目标文件路径,支持绝对路径" },
                    "content": { "type": "string", "description": "文件内容" }
                },
                "required": ["path", "content"]
            }),
        ),
        tool(
            "fs_delete",
            "删除本机文件(删除前自动备份);path 支持绝对路径,仅支持文件。",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "文件路径,支持绝对路径" } },
                "required": ["path"]
            }),
        ),
        tool(
            "fs_export_docx",
            "导出 .docx,自动套用学术/通用版式:Heading 大纲级别、正文首行缩进与 1.5 倍行距、三线表、封面、目录、页眉页码。正文用 Markdown:#/##/### 标题、- 或 1. 列表、| 表格 |、``` 代码块、**粗体**、`行内代码`、> 引用、![图注](图片路径);表题写在表格上一行(如「表 1 …」),图注写在图片下一行。实验报告/论文建议用 ## 一、… 分章,meta 填作者/课程以生成封面。path 支持绝对路径,自动建目录。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "目标路径,如 D:\\报告\\a.docx" },
                    "title": { "type": "string", "description": "文档标题" },
                    "body": { "type": "string", "description": "正文,按 Markdown 解析;表格用管道语法,表题紧邻表格上一行" },
                    "profile": {
                        "type": "string",
                        "enum": ["experiment", "paper", "practice", "general"],
                        "description": "用途模板:experiment 实验报告 / paper 论文 / practice 实践报告 / general 通用文档;省略则自动识别"
                    },
                    "toc": { "type": "boolean", "description": "是否生成自动目录;省略时学术类默认开启" },
                    "meta": {
                        "type": "object",
                        "description": "封面信息;任一项非空即生成封面",
                        "properties": {
                            "author": { "type": "string", "description": "作者/姓名" },
                            "org": { "type": "string", "description": "单位/学院" },
                            "course": { "type": "string", "description": "课程/项目名称" },
                            "date": { "type": "string", "description": "日期" }
                        }
                    }
                },
                "required": ["path", "body"]
            }),
        ),
        tool(
            "fs_export_xlsx",
            "导出 .xlsx(支持多工作表);path 支持绝对路径,自动建目录。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "目标路径,如 D:\\表格\\a.xlsx" },
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
            "按备份 id 恢复文件(恢复前再备份,可撤销)。",
            json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "备份 id" } },
                "required": ["id"]
            }),
        ),
        tool(
            "zip_extract",
            "把 zip 解包到目标目录(自动跳过越界条目与符号链接);path / dest 支持绝对路径。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "zip 文件路径,支持绝对路径" },
                    "dest": { "type": "string", "description": "目标目录,支持绝对路径" }
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
