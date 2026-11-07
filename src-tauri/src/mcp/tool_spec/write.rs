//! 写入/执行类内置工具:需「工具总开关 + 内置工具」同时打开才下发。

use serde_json::json;

use super::tool;
use crate::provider::ToolDef;

pub fn write_tool_defs() -> Vec<ToolDef> {
    vec![
        tool(
            "shell_exec",
            "执行本机命令。command 填程序名(git/dir/where),参数放 args 数组;默认仅只读命令白名单,路径不限目录,解释器与写操作被拒绝;系统级操作(环境变量/服务/注册表)需审批模式切到「完全信任」。高危命令(格式化/删根/改注册表等)始终需要人工确认。结果首行 [exit=N ...],N≠0 为失败。",
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
            "写入本机文本文件(自动建目录,覆盖前自动备份);path 支持绝对路径。内容在 2 万字符以内时直接写;更大的文件必须改用 fs_write_begin → fs_write_chunk(多次)→ fs_write_finish 分批写入,否则会被单次输出上限截断。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "目标文件路径,支持绝对路径" },
                    "content": { "type": "string", "description": "文件完整内容" }
                },
                "required": ["path", "content"]
            }),
        ),
        tool(
            "fs_edit",
            "精确替换文件中一段原文(写前自动备份)。old 必须与文件内容逐字一致(含缩进);出现 0 次或存在多处匹配会报错,请加长 old 使其唯一,或设 replace_all=true 全部替换。局部修改优先用它,不要整体重写;path 支持绝对路径。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "目标文件路径,支持绝对路径" },
                    "old": { "type": "string", "description": "待替换的原文片段(须逐字一致)" },
                    "new": { "type": "string", "description": "替换后的新片段(可为空字符串表示删除)" },
                    "replace_all": { "type": "boolean", "description": "true = 替换所有匹配处;默认仅允许唯一匹配" }
                },
                "required": ["path", "old", "new"]
            }),
        ),
        tool(
            "fs_write_begin",
            "开始分批写入大文件:先建临时分片并返回 write_id,目标文件此时不会变动。随后用 fs_write_chunk 逐段追加,最后用 fs_write_finish 落盘。适用于几十万字符以上的长文档/代码,避免单次输出被截断。path 支持绝对路径。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "目标文件路径,支持绝对路径" }
                },
                "required": ["path"]
            }),
        ),
        tool(
            "fs_write_chunk",
            "向 fs_write_begin 创建的临时分片追加一段内容(不设大小上限)。返回已累计字节数,用于核对是否写全;未 finish 前不会产生半截的目标文件。",
            json!({
                "type": "object",
                "properties": {
                    "write_id": { "type": "string", "description": "fs_write_begin 返回的 write_id" },
                    "content": { "type": "string", "description": "本次追加的内容(一页即可)" }
                },
                "required": ["write_id", "content"]
            }),
        ),
        tool(
            "fs_write_finish",
            "结束分批写入:备份原文件后把临时分片写入目标路径,返回最终字节数与备份 id。",
            json!({
                "type": "object",
                "properties": {
                    "write_id": { "type": "string", "description": "fs_write_begin 返回的 write_id" }
                },
                "required": ["write_id"]
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
        tool(
            "fs_to_pdf",
            "把已有文档(docx/xlsx/pptx/odt/html 等)高保真转换为 PDF。调用本机 LibreOffice 无头模式,免费离线、无需 API;未安装时返回可读提示。输出到源文件同目录,同名 PDF 已存在时自动加时间戳,不覆盖用户文件。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "待转换文件路径(工作区相对或绝对)" }
                },
                "required": ["path"]
            }),
        ),
    ]
}
