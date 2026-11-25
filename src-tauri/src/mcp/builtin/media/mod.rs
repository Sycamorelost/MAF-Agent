//! 媒体类内置工具入口:图片生成 / 演示文稿导出 / 图片 OCR。

mod chart;
mod diagram;
mod diagram_exchange;
mod diagram_export;
mod diagram_scene;
mod formula;
#[cfg(test)]
mod formula_omml;
mod formula_symbols;
mod image;
mod image_compose;
mod image_postprocess;
mod image_provider;
mod ocr;
mod slides;
mod slides_parse;
mod slides_preflight;
mod slides_render;
mod slides_templates;
mod slides_theme;
mod slides_chrome;
mod slides_validate;

pub(crate) fn handles(name: &str) -> bool {
    matches!(
        name,
        "image_generate" | "image_compose" | "slides_export" | "diagram_export" | "ocr_extract"
    )
}

pub(crate) async fn exec(
    name: &str,
    args: &serde_json::Value,
    session_id: Option<&str>,
) -> (String, bool) {
    match name {
        "image_generate" => image::exec(args, session_id).await,
        "image_compose" => image_compose::exec(args, session_id).await,
        "slides_export" => slides::exec(args, session_id).await,
        "diagram_export" => diagram_export::exec(args, session_id).await,
        "ocr_extract" => ocr::exec(args, session_id).await,
        _ => ("未知媒体工具".into(), false),
    }
}
