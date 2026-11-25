//! 图元导出工具定义(从 `media.rs` 拆出以控制单文件体积)。

use serde_json::json;

use super::tool;
use crate::provider::ToolDef;

pub(super) fn diagram_tool_def() -> ToolDef {
    tool(
        "diagram_export",
        "把独立图元模型导出为 png/svg,或在 Excalidraw/drawio 与图元模型之间互转。直接用 scene 提供 version=1、kind(flowchart/mindmap)、canvas、nodes、edges;节点形状 rectangle/ellipse/diamond,边可带折线点与标签;也可用 import_path 读取本地 .excalidraw/.json 或 .drawio/.xml 后再导出。输出格式用 format(png/svg/excalidraw/drawio);默认写到 outputs/diagrams/diagram.<ext>。只处理本地文件与结构化参数,不访问网络、不执行脚本;超出支持子集的元素会明确报错而不是静默丢弃。",
        json!({
            "type": "object",
            "properties": {
                "scene": {
                    "type": "object",
                    "description": "独立图元场景(与 import_path 二选一)",
                    "properties": {
                        "version": { "type": "integer", "description": "固定为 1" },
                        "kind": { "type": "string", "enum": ["flowchart", "mindmap"], "description": "图类型" },
                        "canvas": {
                            "type": "object",
                            "properties": {
                                "width": { "type": "number", "description": "画布宽,>0 且不超过 10000" },
                                "height": { "type": "number", "description": "画布高,>0 且不超过 10000" }
                            },
                            "required": ["width", "height"]
                        },
                        "nodes": {
                            "type": "array",
                            "description": "节点(1..256),ID 唯一且几何在画布内",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string", "description": "节点 ID(唯一)" },
                                    "label": { "type": "string", "description": "节点文字,不超过 1000 字" },
                                    "shape": { "type": "string", "enum": ["rectangle", "ellipse", "diamond"], "description": "形状,默认 rectangle" },
                                    "geometry": {
                                        "type": "object",
                                        "properties": {
                                            "x": { "type": "number" }, "y": { "type": "number" },
                                            "width": { "type": "number" }, "height": { "type": "number" }
                                        },
                                        "required": ["x", "y", "width", "height"]
                                    }
                                },
                                "required": ["id", "label", "geometry"]
                            }
                        },
                        "edges": {
                            "type": "array",
                            "description": "边(0..512),source/target 必须指向已存在节点",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string", "description": "边 ID(唯一)" },
                                    "source": { "type": "string", "description": "起点节点 ID" },
                                    "target": { "type": "string", "description": "终点节点 ID" },
                                    "label": { "type": "string", "description": "边标签(可选)" },
                                    "points": {
                                        "type": "array",
                                        "maxItems": 64,
                                        "description": "折线路由点(可选)",
                                        "items": {
                                            "type": "object",
                                            "properties": { "x": { "type": "number" }, "y": { "type": "number" } },
                                            "required": ["x", "y"]
                                        }
                                    }
                                },
                                "required": ["id", "source", "target"]
                            }
                        }
                    },
                    "required": ["version", "kind", "canvas", "nodes"]
                },
                "import_path": { "type": "string", "description": "本地交换文件路径(与 scene 二选一),支持 .excalidraw/.json/.drawio/.xml,不超过 8 MiB" },
                "import_format": { "type": "string", "enum": ["excalidraw", "drawio"], "description": "导入格式;省略时按扩展名判断" },
                "format": { "type": "string", "enum": ["png", "svg", "excalidraw", "drawio"], "description": "输出格式,默认 png" },
                "path": { "type": "string", "description": "输出路径;省略时写到 outputs/diagrams/diagram.<ext>" }
            },
            "required": ["format"]
        }),
    )
}
