//! 只读内置工具:不改变磁盘与系统状态,默认下发,免审批。

use serde_json::json;

use super::tool;
use crate::provider::ToolDef;

pub fn readonly_tool_defs() -> Vec<ToolDef> {
    vec![
        tool(
            "get_now",
            "获取当前本地日期、时间、时区与星期。",
            json!({ "type": "object", "properties": {} }),
        ),
        // M2 产物事件:模型显式声明本轮交付的文件,前端据此渲染交付卡
        tool(
            "present",
            "声明本轮交付给用户的文件(报告/表格/图片/代码等)。只对真实存在、且本轮刚写好或用户要的文件调用;path 用工作区相对路径或绝对路径,kind 省略时按扩展名推断。调用后前端会在回答下方展示交付卡,正文里不必再重复完整路径。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "产物路径(工作区相对或绝对)" },
                    "kind": {
                        "type": "string",
                        "enum": ["image", "doc", "sheet", "slides", "pdf", "archive", "text", "html", "file"],
                        "description": "产物类型;省略按扩展名推断"
                    },
                    "name": { "type": "string", "description": "展示名;省略取文件名" },
                    "note": { "type": "string", "description": "一句话说明(可选)" }
                },
                "required": ["path"]
            }),
        ),
        // harness 任务清单:多步任务开始时分解、每完成一步立即更新;UI 实时展示进度
        tool(
            "todo_write",
            "替换式更新本会话任务清单(须提交完整列表)。多步任务开始时分解步骤;之后每完成一步必须立即再次调用本工具,把该项改为 completed 并把下一项改为 in_progress;禁止只在开头建一次清单就不再更新。简单问答不用。",
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
            "读取本机文本文件(默认 3.2 万字符/页,可传 max_chars 放大,不设上限;超大文件分页续读直至读完);path 支持绝对路径。续读优先用上一页截断提示里的 byte_offset(恒定开销),没有时再用 offset(会按字符跳过前缀)。",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "文件路径,支持绝对路径" },
                    "offset": { "type": "number", "description": "跳过前 N 个字符(分页续读;大文件首选 byte_offset)" },
                    "byte_offset": { "type": "number", "description": "从文件第 N 个字节开始读(取上一页提示的 next_byte;续读 O(页大小))" },
                    "max_chars": { "type": "number", "description": "本次读取字符数,默认 3.2 万;不设上限" }
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
        // D1:网页正文抓取(免 API Key)。与联网搜索互补:搜索给候选链接,本工具读原文
        tool(
            "web_fetch",
            "抓取指定网页的正文(免 API Key,只读)。只支持 http/https 公网链接,自动拦截本机/内网地址并遵守 robots.txt;会先去掉导航/侧栏/页脚等模板,再转 Markdown/纯文本。建议把当前要查的问题填入 query,工具只返回最相关段落并排在最前;长页面可用 start_index 续读。依赖登录或纯前端渲染的页面可能抓不到内容。",
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "网页地址(完整 http/https 链接)" },
                    "query": { "type": "string", "description": "检索意图或关键词(建议填当前问题):命中时只返回最相关段落(最相关的在前);省略则按原文顺序返回" },
                    "max_chars": { "type": "number", "description": "返回正文最大字符数,默认 12000,上限 100000" },
                    "start_index": { "type": "number", "description": "从正文第几个字符开始返回(默认 0),用于续读;与 query 同时传时以 query 选段为准" }
                },
                "required": ["url"]
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
