//! 结构图:用 charts-rs 的 Mermaid 能力渲染 flowchart / sequence / mindmap。
//!
//! 只接受这三种图;其它语法或无效源码会带行号报错,不做静默降级。

use serde_json::Value;

/// 支持的 Mermaid 图类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DiagramKind {
    Flowchart,
    Sequence,
    Mindmap,
}

impl DiagramKind {
    pub(super) fn parse(raw: &str) -> Option<DiagramKind> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "flowchart" | "flow" | "graph" => Some(DiagramKind::Flowchart),
            "sequence" | "sequencediagram" | "seq" => Some(DiagramKind::Sequence),
            "mindmap" | "mind" => Some(DiagramKind::Mindmap),
            _ => None,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            DiagramKind::Flowchart => "flowchart",
            DiagramKind::Sequence => "sequenceDiagram",
            DiagramKind::Mindmap => "mindmap",
        }
    }

    fn accepts(&self, directive: &str) -> bool {
        let d = directive.to_ascii_lowercase();
        match self {
            DiagramKind::Flowchart => d.starts_with("flowchart") || d.starts_with("graph"),
            DiagramKind::Sequence => d.starts_with("sequencediagram"),
            DiagramKind::Mindmap => d.starts_with("mindmap"),
        }
    }
}

/// 幻灯片里的结构图声明。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DiagramSpec {
    pub kind: DiagramKind,
    pub source: String,
}

impl DiagramSpec {
    /// 解析并校验:类型白名单 + 首行指令匹配 + Mermaid 语法校验。
    pub(super) fn new(kind: &str, source: &str) -> Result<DiagramSpec, String> {
        let kind = DiagramKind::parse(kind).ok_or_else(|| {
            format!("diagram.kind「{kind}」不支持,仅支持 flowchart / sequence / mindmap")
        })?;
        let source = source.trim().to_string();
        if source.is_empty() {
            return Err("diagram.source 不能为空".into());
        }
        let (line_no, directive) = first_directive(&source)
            .ok_or_else(|| "第 1 行:缺少 Mermaid 图类型指令".to_string())?;
        if !kind.accepts(directive) {
            return Err(format!(
                "第 {line_no} 行:声明为 {} 但源码指令是「{}」",
                kind.name(),
                directive
            ));
        }
        charts_rs::mermaid_validate(&source).map_err(|e| format!("Mermaid 语法错误:{e}"))?;
        if kind == DiagramKind::Mindmap {
            validate_mindmap(&source)?;
        }
        Ok(DiagramSpec { kind, source })
    }

    /// 解析工具入参中的 `diagram` 对象。
    pub(super) fn from_json(v: &Value) -> Result<DiagramSpec, String> {
        let kind = v
            .get("kind")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or("diagram.kind 不能为空")?;
        let source = v
            .get("source")
            .and_then(Value::as_str)
            .ok_or("diagram.source 不能为空")?;
        DiagramSpec::new(kind, source)
    }

    /// 渲染为 PNG(1200 宽,等比高度)。
    pub(super) fn render_png(&self) -> Result<Vec<u8>, String> {
        let mut chart = charts_rs::MermaidChart::new(self.source.clone());
        // 不输出 <a> 链接,避免把外部 URL 写进演示文稿
        chart.allow_links = false;
        let svg = chart.svg().map_err(|e| format!("结构图渲染失败:{e}"))?;
        charts_rs::svg_to_png_with_size(&svg, Some(1200), None)
            .map_err(|e| format!("结构图光栅化失败:{e}"))
    }
}

/// 脑图结构校验:单一根节点、缩进不倒退、层级不超过 4。
fn validate_mindmap(source: &str) -> Result<(), String> {
    let mut seen_directive = false;
    let mut root_indent: Option<usize> = None;
    let mut stack: Vec<usize> = Vec::new();
    for (i, raw) in source.lines().enumerate() {
        let line = raw.trim_end();
        let text = line.trim();
        if text.is_empty() || text.starts_with("%%") || text == "---" {
            continue;
        }
        if !seen_directive {
            seen_directive = text.to_ascii_lowercase().starts_with("mindmap");
            continue;
        }
        let indent = line.chars().take_while(|c| *c == ' ').count();
        match root_indent {
            None => {
                root_indent = Some(indent);
                stack.push(indent);
            }
            Some(root) => {
                if indent < root {
                    return Err(format!("第 {} 行:缩进小于根节点", i + 1));
                }
                if indent == root {
                    return Err(format!(
                        "第 {} 行:检测到第二个根节点,脑图必须有且只有一个根",
                        i + 1
                    ));
                }
                while stack.len() > 1 && indent <= *stack.last().unwrap_or(&root) {
                    stack.pop();
                }
                if indent > *stack.last().unwrap_or(&root) {
                    stack.push(indent);
                }
                if stack.len() > 4 {
                    return Err(format!(
                        "第 {} 行:脑图层级超过 4 级,请收敛分支结构",
                        i + 1
                    ));
                }
            }
        }
    }
    if root_indent.is_none() {
        return Err("脑图缺少根节点".into());
    }
    Ok(())
}

/// 返回首个有效指令行:(行号, 指令文本)。
fn first_directive(source: &str) -> Option<(usize, &str)> {
    let mut in_frontmatter = false;
    for (i, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if line == "---" {
            in_frontmatter = !in_frontmatter;
            continue;
        }
        if in_frontmatter || line.is_empty() || line.starts_with("%%") {
            continue;
        }
        return Some((i + 1, line));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_supported_diagrams() {
        for (kind, src) in [
            ("flowchart", "flowchart LR\n  A[开始] --> B[结束]"),
            ("sequence", "sequenceDiagram\n  A->>B: 请求\n  B-->>A: 返回"),
            ("mindmap", "mindmap\n  root((主题))\n    子项一\n    子项二"),
        ] {
            let spec = DiagramSpec::new(kind, src).unwrap();
            let png = spec.render_png().unwrap();
            assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        }
    }

    #[test]
    fn rejects_unsupported_and_invalid_mermaid() {
        let e = DiagramSpec::new("class", "classDiagram\n  A <|-- B").unwrap_err();
        assert!(e.contains("不支持"), "错误信息:{e}");

        let e = DiagramSpec::new("flowchart", "gantt\n  title A").unwrap_err();
        assert!(e.contains("第 1 行"), "错误信息:{e}");

        let e = DiagramSpec::new("flowchart", "flowchart LR\n  A[未闭合 --> B").unwrap_err();
        assert!(e.contains("Mermaid"), "错误信息:{e}");
    }

    #[test]
    fn mindmap_checks_roots_and_depth() {
        let e = DiagramSpec::new("mindmap", "mindmap\n  root((主题))\n  另一个根").unwrap_err();
        assert!(
            e.contains("第二个根节点") || e.contains("single root"),
            "错误信息:{e}"
        );

        let deep = "mindmap\n  r((根))\n    a\n      b\n        c\n          d\n            e";
        let e = DiagramSpec::new("mindmap", deep).unwrap_err();
        assert!(e.contains("层级超过 4"), "错误信息:{e}");

        assert!(DiagramSpec::new("mindmap", "mindmap\n  r((根))\n    a\n      b").is_ok());
    }
}
