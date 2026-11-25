//! 版本化独立图元场景：只描述 flowchart / mindmap，不依赖 Mermaid 或外部运行时。

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(super) const SCENE_VERSION: u32 = 1;
const MAX_CANVAS: f64 = 10_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum SceneKind {
    Flowchart,
    Mindmap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub(super) enum NodeShape {
    #[default]
    Rectangle,
    Ellipse,
    Diamond,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Canvas {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Geometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Geometry {
    pub(super) fn center(&self) -> Point {
        Point {
            x: self.x + self.width / 2.0,
            y: self.y + self.height / 2.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DiagramNode {
    pub id: String,
    pub label: String,
    pub geometry: Geometry,
    #[serde(default)]
    pub shape: NodeShape,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DiagramEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub label: String,
    /// 可选折线路由点；首尾端点始终取源、目标节点中心。
    #[serde(default)]
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DiagramScene {
    pub version: u32,
    pub kind: SceneKind,
    pub canvas: Canvas,
    pub nodes: Vec<DiagramNode>,
    pub edges: Vec<DiagramEdge>,
}

impl DiagramScene {
    pub(super) fn from_json(value: &Value) -> Result<Self, String> {
        let scene: Self = serde_json::from_value(value.clone())
            .map_err(|e| format!("scene 结构错误:{e}"))?;
        scene.validate()?;
        Ok(scene)
    }

    /// 全量校验，导入和导出共用；任何非法元素都不静默丢弃。
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.version != SCENE_VERSION {
            return Err(format!(
                "scene.version={} 不受支持，当前仅支持 {SCENE_VERSION}",
                self.version
            ));
        }
        check_extent(self.canvas.width, self.canvas.height, "canvas")?;
        if self.nodes.is_empty() {
            return Err("scene.nodes 不能为空".into());
        }
        if self.nodes.len() > 256 || self.edges.len() > 512 {
            return Err("图元数量超过上限（节点 256，边 512）".into());
        }
        let mut ids = HashSet::new();
        let mut node_ids = HashSet::new();
        for node in &self.nodes {
            check_id(&node.id)?;
            if !ids.insert(node.id.as_str()) {
                return Err(format!("重复图元 ID「{}」", node.id));
            }
            node_ids.insert(node.id.as_str());
            check_label(&node.label)?;
            check_geometry(&node.geometry, &self.canvas, &node.id)?;
        }
        for edge in &self.edges {
            check_id(&edge.id)?;
            if !ids.insert(edge.id.as_str()) {
                return Err(format!("重复图元 ID「{}」", edge.id));
            }
            if !node_ids.contains(edge.source.as_str()) {
                return Err(format!("边「{}」的 source 悬空：{}", edge.id, edge.source));
            }
            if !node_ids.contains(edge.target.as_str()) {
                return Err(format!("边「{}」的 target 悬空：{}", edge.id, edge.target));
            }
            check_label(&edge.label)?;
            if edge.points.len() > 64 {
                return Err(format!("边「{}」路由点超过 64 个", edge.id));
            }
            for point in &edge.points {
                check_point(point, &self.canvas, &edge.id)?;
            }
        }
        Ok(())
    }

    /// 自绘确定性 SVG：输入不变则字节不变，无脚本、外链或运行时依赖。
    pub(super) fn render_svg(&self) -> Result<String, String> {
        self.validate()?;
        let (w, h) = (self.canvas.width, self.canvas.height);
        let mut out = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
            n(w), n(h), n(w), n(h)
        );
        out.push_str("<defs><marker id=\"arrow\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"7\" markerHeight=\"7\" orient=\"auto-start-reverse\"><path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"#526079\"/></marker></defs>");
        out.push_str(&format!("<rect width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>", n(w), n(h)));
        for edge in &self.edges {
            let a = self.nodes.iter().find(|n| n.id == edge.source).unwrap().geometry.center();
            let b = self.nodes.iter().find(|n| n.id == edge.target).unwrap().geometry.center();
            let points = std::iter::once(a).chain(edge.points.iter().copied()).chain(std::iter::once(b))
                .map(|p| format!("{},{}", n(p.x), n(p.y))).collect::<Vec<_>>().join(" ");
            out.push_str(&format!("<polyline id=\"{}\" points=\"{}\" fill=\"none\" stroke=\"#526079\" stroke-width=\"2\" marker-end=\"url(#arrow)\"/>", xml_escape(&edge.id), points));
            if !edge.label.is_empty() {
                push_text(&mut out, (a.x + b.x) / 2.0, (a.y + b.y) / 2.0 - 8.0, &edge.label, 14, "#526079");
            }
        }
        for node in &self.nodes {
            let g = node.geometry;
            let id = xml_escape(&node.id);
            let common = format!("id=\"{id}\" fill=\"#f5f8ff\" stroke=\"#3568b8\" stroke-width=\"2\"");
            match node.shape {
                NodeShape::Rectangle => out.push_str(&format!("<rect {common} x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"6\"/>", n(g.x), n(g.y), n(g.width), n(g.height))),
                NodeShape::Ellipse => out.push_str(&format!("<ellipse {common} cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"/>", n(g.x + g.width / 2.0), n(g.y + g.height / 2.0), n(g.width / 2.0), n(g.height / 2.0))),
                NodeShape::Diamond => out.push_str(&format!("<polygon {common} points=\"{},{} {},{} {},{} {},{}\"/>", n(g.x + g.width / 2.0), n(g.y), n(g.x + g.width), n(g.y + g.height / 2.0), n(g.x + g.width / 2.0), n(g.y + g.height), n(g.x), n(g.y + g.height / 2.0))),
            }
            push_text(&mut out, g.x + g.width / 2.0, g.y + g.height / 2.0, &node.label, 18, "#17233b");
        }
        out.push_str("</svg>");
        Ok(out)
    }

    pub(super) fn render_png(&self) -> Result<Vec<u8>, String> {
        let svg = self.render_svg()?;
        charts_rs::svg_to_png_with_size(&svg, Some(1200), None)
            .map_err(|e| format!("结构图光栅化失败:{e}"))
    }
}

fn check_extent(width: f64, height: f64, name: &str) -> Result<(), String> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0
        || width > MAX_CANVAS || height > MAX_CANVAS {
        return Err(format!("{name} 尺寸必须有限、为正且不超过 {MAX_CANVAS}"));
    }
    Ok(())
}

fn check_geometry(g: &Geometry, c: &Canvas, id: &str) -> Result<(), String> {
    check_extent(g.width, g.height, &format!("节点 {id}"))?;
    check_point(&Point { x: g.x, y: g.y }, c, id)?;
    if g.x + g.width > c.width || g.y + g.height > c.height {
        return Err(format!("节点「{id}」几何超出画布"));
    }
    Ok(())
}

fn check_point(p: &Point, c: &Canvas, id: &str) -> Result<(), String> {
    if !p.x.is_finite() || !p.y.is_finite() || p.x < 0.0 || p.y < 0.0 || p.x > c.width || p.y > c.height {
        return Err(format!("图元「{id}」坐标出界或非有限值"));
    }
    Ok(())
}

fn check_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.trim() != id || id.chars().count() > 128 || id.chars().any(char::is_control) {
        return Err(format!("非法图元 ID「{id}」"));
    }
    Ok(())
}

fn check_label(label: &str) -> Result<(), String> {
    if label.chars().count() > 1000 || label.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')) {
        return Err("标签为空格以外的非法控制字符，或超过 1000 字".into());
    }
    Ok(())
}

fn push_text(out: &mut String, x: f64, y: f64, label: &str, size: u32, color: &str) {
    if label.is_empty() { return; }
    let lines = label.lines().collect::<Vec<_>>();
    let start = y - (lines.len().saturating_sub(1) as f64) * size as f64 * 0.6;
    out.push_str(&format!("<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" dominant-baseline=\"middle\" font-family=\"Arial,Microsoft YaHei,sans-serif\" font-size=\"{size}\" fill=\"{color}\">", n(x), n(start)));
    for (i, line) in lines.iter().enumerate() {
        out.push_str(&format!("<tspan x=\"{}\" dy=\"{}\">{}</tspan>", n(x), if i == 0 { "0".into() } else { format!("{}", size as f64 * 1.2) }, xml_escape(line)));
    }
    out.push_str("</text>");
}

pub(super) fn xml_escape(raw: &str) -> String {
    raw.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .replace('"', "&quot;").replace('\'', "&apos;")
}

fn n(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

#[cfg(test)]
pub(super) fn fixture() -> DiagramScene {
    serde_json::from_value(serde_json::json!({
        "version": 1, "kind": "flowchart", "canvas": {"width": 640.0, "height": 360.0},
        "nodes": [
            {"id":"a", "label":"开始 & A", "shape":"rectangle", "geometry":{"x":40.0,"y":80.0,"width":140.0,"height":70.0}},
            {"id":"b", "label":"结束", "shape":"diamond", "geometry":{"x":350.0,"y":70.0,"width":150.0,"height":90.0}}
        ],
        "edges": [{"id":"e", "source":"a", "target":"b", "label":"继续", "points":[{"x":270.0,"y":115.0}]}]
    })).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_renders_deterministic_svg_and_png() {
        let scene = fixture();
        scene.validate().unwrap();
        assert_eq!(scene.render_svg().unwrap(), scene.render_svg().unwrap());
        assert!(scene.render_png().unwrap().starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn rejects_duplicate_dangling_out_of_bounds_and_version() {
        let mut scene = fixture();
        scene.nodes[1].id = "a".into();
        assert!(scene.validate().unwrap_err().contains("重复"));
        let mut scene = fixture();
        scene.edges[0].target = "missing".into();
        assert!(scene.validate().unwrap_err().contains("悬空"));
        let mut scene = fixture();
        scene.nodes[0].geometry.x = 600.0;
        assert!(scene.validate().unwrap_err().contains("超出画布"));
        let mut scene = fixture();
        scene.version = 2;
        assert!(scene.validate().unwrap_err().contains("version"));
        assert!(DiagramScene::from_json(&serde_json::json!({"version":1,"kind":"sequence"})).is_err());
    }
}
