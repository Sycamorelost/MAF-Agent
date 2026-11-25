//! 独立图元导出：结构化 scene 或本地交换文件 → png/svg/excalidraw/drawio。

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::diagram_exchange::{from_drawio, from_excalidraw, to_drawio, to_excalidraw};
use super::diagram_scene::DiagramScene;

const MAX_IMPORT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Png,
    Svg,
    Excalidraw,
    Drawio,
}

impl OutputFormat {
    fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "png" => Ok(Self::Png),
            "svg" => Ok(Self::Svg),
            "excalidraw" | "json" => Ok(Self::Excalidraw),
            "drawio" | "xml" => Ok(Self::Drawio),
            _ => Err("format 仅支持 png / svg / excalidraw / drawio".into()),
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Svg => "svg",
            Self::Excalidraw => "excalidraw",
            Self::Drawio => "drawio",
        }
    }
}

/// 与现有媒体工具一致：返回（模型可读回执，是否成功）。
pub(super) async fn exec(args: &Value, session_id: Option<&str>) -> (String, bool) {
    match run(args, session_id) {
        Ok(message) => (message, true),
        Err(error) => (format!("结构图导出失败:{error}"), false),
    }
}

fn run(args: &Value, session_id: Option<&str>) -> Result<String, String> {
    let root = crate::fs::paths::resolve_workspace_root(session_id)?;
    run_with_root(args, session_id, &root)
}

fn run_with_root(args: &Value, session_id: Option<&str>, root: &Path) -> Result<String, String> {
    let format = OutputFormat::parse(args.get("format").and_then(Value::as_str).unwrap_or("png"))?;
    let scene = load_scene(args, root)?;
    let bytes = export_bytes(&scene, format)?;
    let target = target_path(args, root, format)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建输出目录失败:{e}"))?;
    }
    std::fs::write(&target, &bytes).map_err(|e| format!("写入结构图失败:{e}"))?;
    let path = crate::fs::paths::rel_of(root, &target)
        .unwrap_or_else(|_| target.display().to_string().replace('\\', "/"));
    let kind = if matches!(format, OutputFormat::Png | OutputFormat::Svg) { "image" } else { "file" };
    let name = target.file_name().and_then(|s| s.to_str()).unwrap_or("diagram");
    let (note, registered) = crate::deliverables::present(
        session_id,
        &json!({"path":path,"kind":kind,"name":name,"note":"独立图元导出"}),
    );
    if !registered && session_id.is_some() {
        log::warn!("登记结构图产物失败:{note}");
    }
    Ok(format!(
        "已导出结构图:{path}({} KB){}",
        bytes.len() / 1024,
        if registered { ",已登记产物" } else { "" }
    ))
}

fn load_scene(args: &Value, root: &Path) -> Result<DiagramScene, String> {
    let import_path = args.get("import_path").and_then(Value::as_str).unwrap_or("").trim();
    match (args.get("scene").filter(|v| !v.is_null()), import_path.is_empty()) {
        (Some(_), false) => Err("scene 与 import_path 只能二选一".into()),
        (Some(scene), true) => DiagramScene::from_json(scene),
        (None, true) => Err("必须提供结构化 scene 或本地 import_path".into()),
        (None, false) => {
            if import_path.contains("://") {
                return Err("import_path 仅支持本地文件，不支持 URL".into());
            }
            let path = crate::fs::paths::resolve_guarded(root, import_path)?;
            let size = std::fs::metadata(&path).map_err(|e| format!("无法读取导入文件:{e}"))?.len();
            if size > MAX_IMPORT_BYTES {
                return Err("导入文件超过 8 MiB 上限".into());
            }
            let source = std::fs::read_to_string(&path).map_err(|e| format!("导入文件须为 UTF-8 文本:{e}"))?;
            let declared = args.get("import_format").and_then(Value::as_str).unwrap_or("");
            let inferred = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            let format = if declared.trim().is_empty() { inferred } else { declared };
            match format.trim().to_ascii_lowercase().as_str() {
                "excalidraw" | "json" => from_excalidraw(&source),
                "drawio" | "xml" => from_drawio(&source),
                _ => Err("无法识别导入格式，请使用 .excalidraw/.json 或 .drawio/.xml，或指定 import_format".into()),
            }
        }
    }
}

fn target_path(args: &Value, root: &Path, format: OutputFormat) -> Result<PathBuf, String> {
    let explicit = args.get("path").and_then(Value::as_str).unwrap_or("").trim();
    let mut target = if explicit.is_empty() {
        root.join("outputs").join("diagrams").join(format!("diagram.{}", format.extension()))
    } else {
        crate::fs::paths::resolve_guarded(root, explicit)?
    };
    target.set_extension(format.extension());
    Ok(target)
}

fn export_bytes(scene: &DiagramScene, format: OutputFormat) -> Result<Vec<u8>, String> {
    scene.validate()?;
    match format {
        OutputFormat::Png => scene.render_png(),
        OutputFormat::Svg => scene.render_svg().map(String::into_bytes),
        OutputFormat::Excalidraw => to_excalidraw(scene).map(String::into_bytes),
        OutputFormat::Drawio => to_drawio(scene).map(String::into_bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::builtin::media::diagram_scene::fixture;

    #[test]
    fn exports_scene_and_imports_local_exchange_file() {
        let root = std::env::temp_dir().join(format!("maf-diagram-export-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let scene = serde_json::to_value(fixture()).unwrap();
        let svg = root.join("scene.svg");
        let args = json!({"scene":scene,"format":"svg","path":svg});
        assert!(run_with_root(&args, None, &root).unwrap().contains("scene.svg"));
        assert!(std::fs::read_to_string(&svg).unwrap().contains("<svg"));

        let exchange = root.join("scene.drawio");
        let drawio = json!({"scene":serde_json::to_value(fixture()).unwrap(),"format":"drawio","path":exchange});
        run_with_root(&drawio, None, &root).unwrap();
        let imported = json!({"import_path":exchange,"format":"svg","path":root.join("imported.svg")});
        run_with_root(&imported, None, &root).unwrap();
        assert!(root.join("imported.svg").is_file());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rejects_ambiguous_missing_and_remote_input() {
        let root = std::env::temp_dir();
        assert!(load_scene(&json!({}), &root).is_err());
        assert!(load_scene(&json!({"scene":{},"import_path":"a.drawio"}), &root).unwrap_err().contains("二选一"));
        assert!(load_scene(&json!({"import_path":"https://example.com/a.drawio"}), &root).unwrap_err().contains("本地"));
        assert!(OutputFormat::parse("sequence").is_err());
    }
}
