//! 媒体类内置工具定义:图片生成 / 演示文稿导出 / 图片文字识别。
//!
//! 图片生成与演示文稿属于写入工具(产物落盘),随高频写入工具一起常驻下发;
//! OCR 只读本机图片并返回文本,归入只读工具,默认下发、免审批。

use serde_json::json;

use super::tool;
use crate::provider::ToolDef;

mod extra;

/// 写入类媒体工具(需审批)。
pub fn write_media_tool_defs() -> Vec<ToolDef> {
    vec![
        tool(
            "image_generate",
            "用已配置的图像服务商生成图片并保存到工作区。prompt 描述画面(中文可用,越具体越好);默认保存到 outputs/images/,返回相对路径并自动登记为交付物。可用 format/quality/aspect/max_side 统一交付规格:论文配图建议 png、16:9 或 4:3、max_side 1600;照片类用 jpg + quality 85 控制体积。服务商与模型在「设置 → 能力 → 媒体」中配置。",
            json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "画面描述,不超过 4000 字符" },
                    "size": {
                        "type": "string",
                        "enum": ["auto", "1024x1024", "1536x1024", "1024x1536", "1792x1024", "1024x1792"],
                        "description": "图片尺寸;auto 由服务商决定,默认 auto"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["auto", "png", "jpg"],
                        "description": "输出格式;auto 保持服务商原始格式,默认 auto"
                    },
                    "quality": { "type": "integer", "description": "JPEG 质量 40..100,默认 88;仅对 jpg 生效" },
                    "aspect": {
                        "type": "string",
                        "enum": ["1:1", "4:3", "3:2", "16:9", "3:4", "2:3", "9:16"],
                        "description": "裁切比例(居中裁切,可选);未指定则保持原比例"
                    },
                    "max_side": { "type": "integer", "description": "最长边像素上限(等比缩小,不放大),可选" },
                    "model": { "type": "string", "description": "临时覆盖设置中的图像模型(可选)" },
                    "path": { "type": "string", "description": "保存路径(工作区相对或绝对);省略时写到 outputs/images/<时间戳>.<格式>" }
                },
                "required": ["prompt"]
            }),
        ),
        tool(
            "slides_export",
            "把结构化大纲导出为 .pptx 演示文稿(16:9,自带封面、页眉页脚与主题配色)。先给出大纲再调用。写作要求:每页 4-6 条、每条 20-40 字,首条给结论,配数据/案例/对比;标题用短语不用句子;「- 」前缀的行为二级要点;讲稿展开写进 notes。有趋势或对比数据时用 layout=chart 并填 categories/series;有两三个关键数字时用 layout=kpi。版式:bullets 要点 / two-col 两栏 / image-right 右图 / table 表格 / chart 图表 / kpi 指标卡 / diagram 结构图 / cards 卡片 / quote 引用 / section 章节过渡 / closing 结束页(封面自动生成)。禁止「赋能/助力/全面提升/在当今时代」等套话与排比;数据只写工具结果或用户提供的,没有就写定性结论。表格页用 table 给 headers 与 rows;配图用 image 指定工作区路径。场景模板用 template:group-meeting 组会 / progress 进度 / defense 答辩 / work-summary 工作汇报总结 / data-analysis 数据分析 / lecture 授课 / pitch 路演 / training 培训(仅设默认主题并做结构检查,不改写内容)。结构图用 diagram 给 kind 与 Mermaid 源码;自定义配色可用 theme_pack。默认写到 outputs/slides/,生成失败时回退同名 .md 大纲。",
            json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "演示文稿标题(同时作为默认文件名与封面标题),不超过 120 字符" },
                    "subtitle": { "type": "string", "description": "封面副标题(可选),如汇报人/日期/一句话主张" },
                    "theme": {
                        "type": "string",
                        "enum": ["corporate", "modern", "tech", "vibrant", "nature", "dark", "academic", "ocean", "sunset"],
                        "description": "配色主题,默认 corporate;academic 为黑灰学术风(答辩/组会),ocean/sunset 为低饱和冷暖色"
                    },
                    "theme_pack": {
                        "type": "string",
                        "description": "版本化主题包(可选):内联 JSON 或工作区内 .json 路径;字段 schema_version=1、name、background、text、title、accent、muted(均 #RRGGBB),不得覆盖内置主题名"
                    },
                    "template": {
                        "type": "string",
                        "enum": ["group-meeting", "progress", "defense", "work-summary", "data-analysis", "lecture", "pitch", "training"],
                        "description": "场景模板:group-meeting 组会 / progress 进度 / defense 答辩 / work-summary 工作汇报总结 / data-analysis 数据分析 / lecture 授课 / pitch 路演 / training 培训;决定默认主题与结构检查,不改写内容"
                    },
                    "path": { "type": "string", "description": "目标 .pptx 路径;省略时写到 outputs/slides/<title>.pptx" },
                    "slides": {
                        "type": "array",
                        "description": "幻灯片列表(1..30 页,封面自动生成,此处不要包含封面)",
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string", "description": "本页标题(短语,不超过 40 字符)" },
                                "layout": {
                                    "type": "string",
                                    "enum": ["bullets", "two-col", "image-right", "table", "chart", "kpi", "diagram", "cards", "quote", "section", "closing"],
                                    "description": "版式,默认 bullets;有数据用 chart/kpi,结构/流程用 diagram,读书卡片用 cards,引用页用 quote,章节页用 section,结束页用 closing"
                                },
                                "bullets": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "本页要点,建议 4-6 条,最多 10 条,每条 20-40 字;「- 」前缀的行为二级要点"
                                },
                                "chart": {
                                    "type": "object",
                                    "description": "原生图表(layout=chart 时使用)",
                                    "properties": {
                                        "type": {
                                            "type": "string",
                                            "enum": ["bar", "hbar", "line", "area", "pie", "doughnut", "stacked", "scatter", "heatmap"],
                                            "description": "图表类型,默认 bar;scatter/heatmap 仅 chart_style=raster 支持"
                                        },
                                        "title": { "type": "string", "description": "图表标题(可选)" },
                                        "unit": { "type": "string", "description": "单位脚注(可选),如 万元 / % / 人" },
                                        "source": { "type": "string", "description": "数据来源脚注(可选),须为工具结果或用户提供" },
                                        "categories": {
                                            "type": "array",
                                            "items": { "type": "string" },
                                            "description": "分类/横轴标签(最多 12 个)"
                                        },
                                        "series": {
                                            "type": "array",
                                            "maxItems": 3,
                                            "items": {
                                                "type": "object",
                                                "properties": {
                                                    "name": { "type": "string", "description": "系列名" },
                                                    "values": {
                                                        "type": "array",
                                                        "items": { "type": "number" },
                                                        "description": "数值,与 categories 等长"
                                                    }
                                                },
                                                "required": ["name", "values"]
                                            },
                                            "description": "数据系列(最多 3 条)"
                                        }
                                    },
                                    "required": ["categories", "series"]
                                },
                                "chart_style": {
                                    "type": "string",
                                    "enum": ["native", "raster"],
                                    "description": "图表渲染方式,默认 native(可编辑);raster 用光栅 PNG,需要 layout=chart 或 kpi"
                                },
                                "diagram": {
                                    "type": "object",
                                    "description": "结构图(layout=diagram 或 image-right 时使用);仅支持 flowchart / sequence / mindmap 的 Mermaid 子集",
                                    "properties": {
                                        "kind": { "type": "string", "enum": ["flowchart", "sequence", "mindmap"], "description": "图类型" },
                                        "source": { "type": "string", "description": "Mermaid 源码,首行指令须与 kind 一致;脑图单根且层级不超过 4" }
                                    },
                                    "required": ["kind", "source"]
                                },
                                "cards": {
                                    "type": "array",
                                    "maxItems": 4,
                                    "description": "卡片内容(layout=cards,如读书卡片/概念卡),最多 4 张",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "title": { "type": "string", "description": "卡片标题,不超过 28 字" },
                                            "text": { "type": "string", "description": "卡片正文,不超过 120 字" }
                                        }
                                    }
                                },
                                "quote": {
                                    "type": "object",
                                    "description": "引用页内容(layout=quote)",
                                    "properties": {
                                        "text": { "type": "string", "description": "引用原文,不超过 160 字" },
                                        "source": { "type": "string", "description": "出处/作者,不超过 60 字" }
                                    },
                                    "required": ["text"]
                                },
                                "kpis": {
                                    "type": "array",
                                    "maxItems": 4,
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "value": { "type": "string", "description": "大号数字,如 42%、3.2x、128 万" },
                                            "label": { "type": "string", "description": "一行说明" }
                                        },
                                        "required": ["value", "label"]
                                    },
                                    "description": "指标卡(layout=kpi 时使用),最多 4 张"
                                },
                                "table": {
                                    "type": "object",
                                    "description": "表格数据(layout=table 时使用)",
                                    "properties": {
                                        "headers": {
                                            "type": "array",
                                            "items": { "type": "string" },
                                            "description": "表头(最多 8 列)"
                                        },
                                        "rows": {
                                            "type": "array",
                                            "items": {
                                                "type": "array",
                                                "items": { "type": "string" }
                                            },
                                            "description": "数据行(最多 10 行,单元格不超过 60 字符)"
                                        }
                                    },
                                    "required": ["headers", "rows"]
                                },
                                "notes": { "type": "string", "description": "讲稿备注(可选),可写完整句子与展开说明" },
                                "image": { "type": "string", "description": "配图路径(工作区内;可选),配图时要点建议不超过 4 条" }
                            },
                            "required": ["title"]
                        }
                    }
                },
                "required": ["title", "slides"]
            }),
        ),
        tool(
            "image_compose",
            "确定性合成海报/封面,或给本地截图加标注:不调用图像模型、不上传内容。mode=poster 生成 1200×1600 竖版海报,mode=cover 生成 1600×900 横版封面,两者 title 必填,可选 subtitle/author/date/brand_color/width/height/safe_area;版面按安全区排版,行数超出会明确报错。mode=annotate 对工作区内 PNG/JPEG 加标注:input_path 指定原图,annotations 数组逐项给 type(number 编号 / arrow 箭头 / border 边框 / pixelate 像素打码)与坐标;打码先作用于源像素再缩放,避免原始敏感信息残留。默认写到 outputs/images/,可用 output_path 指定;输出恒为 PNG 并自动登记交付物。",
            json!({
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["poster", "cover", "annotate"],
                        "description": "合成模式:poster 海报 / cover 封面 / annotate 截图标注"
                    },
                    "title": { "type": "string", "description": "主标题(poster/cover 必填),不超过 120 字" },
                    "subtitle": { "type": "string", "description": "副标题(可选),不超过 240 字" },
                    "author": { "type": "string", "description": "作者/单位(可选),不超过 80 字" },
                    "date": { "type": "string", "description": "日期(可选),不超过 40 字" },
                    "brand_color": { "type": "string", "description": "品牌主色 #RRGGBB,默认 #2563EB" },
                    "width": { "type": "integer", "description": "画布宽(可选,默认海报 1200 / 封面 1600)" },
                    "height": { "type": "integer", "description": "画布高(可选,默认海报 1600 / 封面 900)" },
                    "safe_area": { "type": "integer", "description": "安全边距(可选,默认海报 72 / 封面 64),不得超过短边四分之一" },
                    "input_path": { "type": "string", "description": "原图路径(annotate 必填),工作区内 PNG/JPEG,不超过 20 MiB" },
                    "annotations": {
                        "type": "array",
                        "maxItems": 64,
                        "description": "标注列表(annotate 必填,1..64 项),坐标相对原图像素",
                        "items": {
                            "type": "object",
                            "properties": {
                                "type": {
                                    "type": "string",
                                    "enum": ["number", "arrow", "border", "pixelate"],
                                    "description": "标注类型:编号 / 箭头 / 边框 / 像素打码"
                                },
                                "x": { "type": "integer", "description": "number/border/pixelate 的 x 坐标" },
                                "y": { "type": "integer", "description": "number/border/pixelate 的 y 坐标" },
                                "width": { "type": "integer", "description": "border/pixelate 的宽度" },
                                "height": { "type": "integer", "description": "border/pixelate 的高度" },
                                "x1": { "type": "integer", "description": "arrow 起点 x" },
                                "y1": { "type": "integer", "description": "arrow 起点 y" },
                                "x2": { "type": "integer", "description": "arrow 终点 x" },
                                "y2": { "type": "integer", "description": "arrow 终点 y" },
                                "number": { "type": "integer", "description": "编号值 1..99(可选,省略则按顺序)" },
                                "color": { "type": "string", "description": "标注颜色 #RRGGBB,默认 #E11D48" },
                                "block_size": { "type": "integer", "description": "pixelate 的马赛克块大小 2..256,默认 16" }
                            },
                            "required": ["type"]
                        }
                    },
                    "max_side": { "type": "integer", "description": "标注后最长边上限(等比缩小,0 表示保持原尺寸)" },
                    "output_path": { "type": "string", "description": "输出 PNG 路径(工作区内);省略时统一写到 outputs/images/" }
                },
                "required": ["mode"]
            }),
        ),
        extra::diagram_tool_def(),
    ]
}

/// 只读类媒体工具(免审批)。
pub fn readonly_media_tool_defs() -> Vec<ToolDef> {
    vec![tool(
        "ocr_extract",
        "识别本机图片中的文字(Windows 系统 OCR)。支持 png / jpg / jpeg,返回按视觉顺序排列的文本行;识别语言取决于系统已安装的 OCR 语言包。",
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "图片路径(工作区相对或绝对),也支持会话附件路径" },
                "lang": {
                    "type": "string",
                    "enum": ["chs", "eng", "chs_eng"],
                    "description": "识别语言,默认 chs_eng(中英混排)"
                },
                "max_lines": { "type": "integer", "description": "最多返回的行数(1..1000,默认 400)" }
            },
            "required": ["path"]
        }),
    )]
}
