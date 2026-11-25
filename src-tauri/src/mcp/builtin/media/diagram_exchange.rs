//! Excalidraw JSON 与 drawio mxGraph XML 的受支持子集交换。
//! 子集：矩形/椭圆/菱形、带可选标签的绑定文本、带可选折线点的箭头。

use std::collections::HashSet;

use roxmltree::Document;
use serde_json::{Value, json};

use super::diagram_scene::{
    Canvas, DiagramEdge, DiagramNode, DiagramScene, Geometry, NodeShape, Point, SCENE_VERSION,
    SceneKind, xml_escape,
};

pub(super) fn to_excalidraw(scene: &DiagramScene) -> Result<String, String> {
    scene.validate()?;
    let mut used = scene.nodes.iter().map(|n| n.id.clone())
        .chain(scene.edges.iter().map(|e| e.id.clone())).collect::<HashSet<_>>();
    let mut elements = Vec::new();
    for node in &scene.nodes {
        let text_id = text_id(&node.id, &mut used);
        let mut shape = base(&node.id, shape_name(node.shape), node.geometry);
        let obj = shape.as_object_mut().unwrap();
        obj.insert("roundness".into(), if node.shape == NodeShape::Rectangle { json!({"type":3}) } else { Value::Null });
        obj.insert("boundElements".into(), json!([{"id":text_id,"type":"text"}]));
        elements.push(shape);
        elements.push(text(&text_id, &node.label, node.geometry, &node.id));
    }
    for edge in &scene.edges {
        let a = node(scene, &edge.source).geometry.center();
        let b = node(scene, &edge.target).geometry.center();
        let route = std::iter::once(a).chain(edge.points.iter().copied()).chain(std::iter::once(b)).collect::<Vec<_>>();
        let points = route.iter().map(|p| json!([p.x-a.x,p.y-a.y])).collect::<Vec<_>>();
        let width = route.iter().map(|p| (p.x-a.x).abs()).fold(0.0, f64::max);
        let height = route.iter().map(|p| (p.y-a.y).abs()).fold(0.0, f64::max);
        let g = Geometry { x:a.x, y:a.y, width, height };
        let mut arrow = base(&edge.id, "arrow", g);
        let obj = arrow.as_object_mut().unwrap();
        obj.insert("points".into(), Value::Array(points));
        obj.insert("startBinding".into(), json!({"elementId":edge.source,"focus":0,"gap":0}));
        obj.insert("endBinding".into(), json!({"elementId":edge.target,"focus":0,"gap":0}));
        obj.insert("startArrowhead".into(), Value::Null);
        obj.insert("endArrowhead".into(), json!("arrow"));
        obj.insert("lastCommittedPoint".into(), Value::Null);
        if !edge.label.is_empty() {
            let tid = text_id(&edge.id, &mut used);
            obj.insert("boundElements".into(), json!([{"id":tid,"type":"text"}]));
            let tg = Geometry { x:(a.x+b.x)/2.0-40.0, y:(a.y+b.y)/2.0-20.0, width:80.0, height:24.0 };
            elements.push(arrow);
            elements.push(text(&tid, &edge.label, tg, &edge.id));
        } else {
            elements.push(arrow);
        }
    }
    serde_json::to_string_pretty(&json!({
        "type":"excalidraw", "version":2, "source":"myAgent", "elements":elements,
        "appState":{"viewBackgroundColor":"#ffffff","gridSize":null}, "files":{},
        "customData":{"myAgent":{"sceneVersion":SCENE_VERSION,"kind":scene.kind,"canvas":scene.canvas}}
    })).map_err(|e| format!("Excalidraw 序列化失败:{e}"))
}

pub(super) fn from_excalidraw(source: &str) -> Result<DiagramScene, String> {
    let root: Value = serde_json::from_str(source).map_err(|e| format!("Excalidraw JSON 无效:{e}"))?;
    let elements = root.get("elements").and_then(Value::as_array).ok_or("Excalidraw 缺少 elements")?;
    let meta = root.pointer("/customData/myAgent");
    let kind = parse_kind(meta.and_then(|m| m.get("kind")).and_then(Value::as_str))?;
    if let Some(v) = meta.and_then(|m| m.get("sceneVersion")).and_then(Value::as_u64)
        && v != SCENE_VERSION as u64 { return Err("Excalidraw 场景版本不受支持".into()); }
    let mut canvas = meta.and_then(|m| m.get("canvas")).map(|c| -> Result<Canvas,String> {
        Ok(Canvas { width:number(c.get("width"), "canvas.width")?, height:number(c.get("height"), "canvas.height")? })
    }).transpose()?;
    let (mut nodes, mut edges, mut labels) = (Vec::new(), Vec::new(), Vec::new());
    let mut ids = HashSet::new();
    for element in elements {
        if element.get("isDeleted").and_then(Value::as_bool) == Some(true) { continue; }
        let id = text_of(element.get("id"), "element.id")?.to_string();
        if !ids.insert(id.clone()) { return Err(format!("Excalidraw 重复元素 ID「{id}」")); }
        match text_of(element.get("type"), "element.type")? {
            kind @ ("rectangle" | "ellipse" | "diamond") => nodes.push(DiagramNode {
                id, label:String::new(), geometry:geometry(element)?,
                shape:match kind { "ellipse"=>NodeShape::Ellipse, "diamond"=>NodeShape::Diamond, _=>NodeShape::Rectangle },
            }),
            "text" => labels.push((id,
                text_of(element.get("containerId"), "text.containerId")?.to_string(),
                element.get("text").and_then(Value::as_str).unwrap_or("").to_string())),
            "arrow" => {
                let x = number(element.get("x"), "arrow.x")?;
                let y = number(element.get("y"), "arrow.y")?;
                let raw = element.get("points").and_then(Value::as_array).ok_or("arrow 缺少 points")?;
                if raw.len() < 2 { return Err(format!("箭头「{id}」至少需要两个点")); }
                let mut points = Vec::new();
                for p in &raw[1..raw.len()-1] {
                    let pair = p.as_array().filter(|a| a.len()==2).ok_or("arrow.points 必须为二元数组")?;
                    points.push(Point { x:x+number(pair.first(),"point.x")?, y:y+number(pair.get(1),"point.y")? });
                }
                edges.push(DiagramEdge {
                    id, source:text_of(element.pointer("/startBinding/elementId"), "arrow.startBinding")?.into(),
                    target:text_of(element.pointer("/endBinding/elementId"), "arrow.endBinding")?.into(),
                    label:String::new(), points,
                });
            }
            other => return Err(format!("Excalidraw 元素类型「{other}」不在受支持子集内")),
        }
    }
    attach_labels(&mut nodes, &mut edges, labels)?;
    if canvas.is_none() { canvas = Some(infer_canvas(&nodes, &edges)); }
    let scene = DiagramScene { version:SCENE_VERSION, kind, canvas:canvas.unwrap(), nodes, edges };
    scene.validate()?;
    Ok(scene)
}

pub(super) fn to_drawio(scene: &DiagramScene) -> Result<String, String> {
    scene.validate()?;
    let kind = match scene.kind { SceneKind::Flowchart=>"flowchart", SceneKind::Mindmap=>"mindmap" };
    let mut out = format!("<mxfile><diagram name=\"Page-1\"><mxGraphModel dx=\"0\" dy=\"0\" grid=\"0\" pageWidth=\"{}\" pageHeight=\"{}\" myAgentVersion=\"1\" myAgentKind=\"{kind}\"><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/>", scene.canvas.width, scene.canvas.height);
    for n in &scene.nodes {
        let g = n.geometry;
        let shape = match n.shape { NodeShape::Ellipse=>"ellipse;", NodeShape::Diamond=>"rhombus;", NodeShape::Rectangle=>"rounded=0;" };
        out.push_str(&format!("<mxCell id=\"{}\" value=\"{}\" style=\"{shape}whiteSpace=wrap;html=1;\" vertex=\"1\" parent=\"1\"><mxGeometry x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" as=\"geometry\"/></mxCell>", xml_escape(&n.id), xml_escape(&n.label), g.x,g.y,g.width,g.height));
    }
    for e in &scene.edges {
        out.push_str(&format!("<mxCell id=\"{}\" value=\"{}\" style=\"edgeStyle=orthogonalEdgeStyle;html=1;endArrow=block;\" edge=\"1\" parent=\"1\" source=\"{}\" target=\"{}\"><mxGeometry relative=\"1\" as=\"geometry\">", xml_escape(&e.id), xml_escape(&e.label), xml_escape(&e.source), xml_escape(&e.target)));
        if !e.points.is_empty() {
            out.push_str("<Array as=\"points\">");
            for p in &e.points { out.push_str(&format!("<mxPoint x=\"{}\" y=\"{}\"/>",p.x,p.y)); }
            out.push_str("</Array>");
        }
        out.push_str("</mxGeometry></mxCell>");
    }
    out.push_str("</root></mxGraphModel></diagram></mxfile>");
    Ok(out)
}

pub(super) fn from_drawio(source: &str) -> Result<DiagramScene, String> {
    let doc = Document::parse(source).map_err(|e| format!("drawio XML 无效:{e}"))?;
    let model = doc.descendants().find(|n| n.has_tag_name("mxGraphModel"))
        .ok_or("仅支持未压缩且包含 mxGraphModel 的 drawio 文件")?;
    if let Some(v) = model.attribute("myAgentVersion") && v != "1" { return Err("drawio 场景版本不受支持".into()); }
    let kind = parse_kind(model.attribute("myAgentKind"))?;
    let mut canvas = match (model.attribute("pageWidth"),model.attribute("pageHeight")) {
        (Some(w),Some(h)) => Some(Canvas { width:parse_num(w,"pageWidth")?, height:parse_num(h,"pageHeight")? }),
        (None,None) => None, _ => return Err("drawio 画布宽高必须同时提供".into()),
    };
    let root = model.children().find(|n| n.has_tag_name("root")).ok_or("drawio 缺少 root")?;
    let (mut nodes,mut edges,mut ids) = (Vec::new(),Vec::new(),HashSet::new());
    for cell in root.children().filter(|n| n.has_tag_name("mxCell")) {
        let id = cell.attribute("id").ok_or("mxCell 缺少 id")?;
        if !ids.insert(id.to_string()) { return Err(format!("drawio 重复元素 ID「{id}」")); }
        let (vertex,edge) = (cell.attribute("vertex")==Some("1"),cell.attribute("edge")==Some("1"));
        if vertex && edge { return Err(format!("mxCell「{id}」不能同时是节点和边")); }
        if !vertex && !edge { continue; }
        let g = cell.children().find(|n| n.has_tag_name("mxGeometry")).ok_or_else(||format!("mxCell「{id}」缺少几何"))?;
        if vertex {
            let style = cell.attribute("style").unwrap_or("");
            if style.split(';').any(|s|s.starts_with("shape=")) { return Err(format!("节点「{id}」形状不受支持")); }
            let shape = if style.split(';').any(|s|s=="ellipse") { NodeShape::Ellipse }
                else if style.split(';').any(|s|s=="rhombus") { NodeShape::Diamond } else { NodeShape::Rectangle };
            nodes.push(DiagramNode { id:id.into(), label:cell.attribute("value").unwrap_or("").into(), shape,
                geometry:Geometry { x:xml_num(g,"x")?,y:xml_num(g,"y")?,width:xml_num(g,"width")?,height:xml_num(g,"height")? } });
        } else {
            let mut points = Vec::new();
            if let Some(array) = g.children().find(|n|n.has_tag_name("Array")&&n.attribute("as")==Some("points")) {
                for p in array.children().filter(|n|n.has_tag_name("mxPoint")) {
                    points.push(Point { x:xml_num(p,"x")?,y:xml_num(p,"y")? });
                }
            }
            edges.push(DiagramEdge { id:id.into(),source:cell.attribute("source").ok_or("边缺少 source")?.into(),
                target:cell.attribute("target").ok_or("边缺少 target")?.into(),label:cell.attribute("value").unwrap_or("").into(),points });
        }
    }
    if canvas.is_none() { canvas=Some(infer_canvas(&nodes,&edges)); }
    let scene=DiagramScene { version:SCENE_VERSION,kind,canvas:canvas.unwrap(),nodes,edges };
    scene.validate()?;
    Ok(scene)
}

fn base(id:&str,kind:&str,g:Geometry)->Value { json!({
    "id":id,"type":kind,"x":g.x,"y":g.y,"width":g.width,"height":g.height,"angle":0,
    "strokeColor":"#3568b8","backgroundColor":"#f5f8ff","fillStyle":"solid","strokeWidth":2,
    "strokeStyle":"solid","roughness":0,"opacity":100,"groupIds":[],"frameId":null,
    "seed":1,"version":1,"versionNonce":0,"isDeleted":false,"boundElements":[],
    "updated":0,"link":null,"locked":false
}) }

fn text(id:&str,label:&str,g:Geometry,container:&str)->Value {
    let mut v=base(id,"text",g); let o=v.as_object_mut().unwrap();
    o.insert("text".into(),json!(label)); o.insert("originalText".into(),json!(label));
    o.insert("fontSize".into(),json!(18)); o.insert("fontFamily".into(),json!(1));
    o.insert("textAlign".into(),json!("center")); o.insert("verticalAlign".into(),json!("middle"));
    o.insert("containerId".into(),json!(container)); o.insert("lineHeight".into(),json!(1.25));
    o.insert("autoResize".into(),json!(false)); v
}

fn text_id(owner:&str,used:&mut HashSet<String>)->String {
    let base=format!("{owner}--text"); let mut id=base.clone(); let mut i=2;
    while used.contains(&id) { id=format!("{base}-{i}"); i+=1; }
    used.insert(id.clone()); id
}
fn node<'a>(scene:&'a DiagramScene,id:&str)->&'a DiagramNode { scene.nodes.iter().find(|n|n.id==id).unwrap() }
fn shape_name(shape:NodeShape)->&'static str { match shape { NodeShape::Rectangle=>"rectangle",NodeShape::Ellipse=>"ellipse",NodeShape::Diamond=>"diamond" } }
fn text_of<'a>(v:Option<&'a Value>,name:&str)->Result<&'a str,String> { v.and_then(Value::as_str).filter(|s|!s.is_empty()).ok_or_else(||format!("缺少有效 {name}")) }
fn number(v:Option<&Value>,name:&str)->Result<f64,String> { v.and_then(Value::as_f64).filter(|n|n.is_finite()).ok_or_else(||format!("{name} 必须为有限数值")) }
fn geometry(v:&Value)->Result<Geometry,String> { Ok(Geometry { x:number(v.get("x"),"x")?,y:number(v.get("y"),"y")?,width:number(v.get("width"),"width")?,height:number(v.get("height"),"height")? }) }
fn parse_num(raw:&str,name:&str)->Result<f64,String> { raw.parse::<f64>().ok().filter(|n|n.is_finite()).ok_or_else(||format!("{name} 必须为有限数值")) }
fn xml_num(node:roxmltree::Node<'_, '_>,name:&str)->Result<f64,String> { parse_num(node.attribute(name).ok_or_else(||format!("缺少几何属性 {name}"))?,name) }
fn parse_kind(raw:Option<&str>)->Result<SceneKind,String> { match raw.unwrap_or("flowchart") { "flowchart"=>Ok(SceneKind::Flowchart),"mindmap"=>Ok(SceneKind::Mindmap),other=>Err(format!("图类型「{other}」不受支持")) } }

fn attach_labels(nodes:&mut [DiagramNode],edges:&mut [DiagramEdge],labels:Vec<(String,String,String)>)->Result<(),String> {
    let mut attached=HashSet::new();
    for (tid,cid,label) in labels {
        if !attached.insert(cid.clone()) { return Err(format!("容器「{cid}」绑定了多个文本")); }
        if let Some(n)=nodes.iter_mut().find(|n|n.id==cid) { n.label=label; }
        else if let Some(e)=edges.iter_mut().find(|e|e.id==cid) { e.label=label; }
        else { return Err(format!("文本「{tid}」的容器「{cid}」不存在")); }
    }
    Ok(())
}

fn infer_canvas(nodes:&[DiagramNode],edges:&[DiagramEdge])->Canvas {
    let mut w=640.0_f64; let mut h=360.0_f64;
    for n in nodes { w=w.max(n.geometry.x+n.geometry.width+40.0); h=h.max(n.geometry.y+n.geometry.height+40.0); }
    for p in edges.iter().flat_map(|e|e.points.iter()) { w=w.max(p.x+40.0); h=h.max(p.y+40.0); }
    Canvas { width:w,height:h }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::builtin::media::diagram_scene::fixture;

    #[test]
    fn supported_formats_roundtrip_without_losing_scene() {
        let scene=fixture();
        let json=to_excalidraw(&scene).unwrap();
        assert_eq!(from_excalidraw(&json).unwrap(),scene);
        let xml=to_drawio(&scene).unwrap();
        assert_eq!(from_drawio(&xml).unwrap(),scene);
    }

    #[test]
    fn rejects_unsupported_and_dangling_exchange_input() {
        assert!(from_excalidraw(r#"{"elements":[{"id":"x","type":"freedraw"}]}"#).unwrap_err().contains("子集"));
        let mut bad=fixture(); bad.edges[0].target="ghost".into();
        assert!(to_drawio(&bad).is_err());
        let xml=r#"<mxfile><diagram><mxGraphModel><root><mxCell id="0"/><mxCell id="a" vertex="1" value="A"><mxGeometry x="0" y="0" width="80" height="40" as="geometry"/></mxCell><mxCell id="e" edge="1" source="a" target="b"><mxGeometry as="geometry"/></mxCell></root></mxGraphModel></diagram></mxfile>"#;
        assert!(from_drawio(xml).unwrap_err().contains("悬空"));
    }
}
