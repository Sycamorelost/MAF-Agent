//! 媒体类内置工具入口:图片生成 / 演示文稿导出 / 图片 OCR。

mod image;
mod ocr;
mod slides;
mod slides_parse;
mod slides_render;
mod slides_templates;

pub(crate) fn handles(name: &str) -> bool {
    matches!(name, "image_generate" | "slides_export" | "ocr_extract")
}

pub(crate) async fn exec(
    name: &str,
    args: &serde_json::Value,
    session_id: Option<&str>,
) -> (String, bool) {
    match name {
        "image_generate" => image::exec(args, session_id).await,
        "slides_export" => slides::exec(args, session_id).await,
        "ocr_extract" => ocr::exec(args, session_id).await,
        _ => ("未知媒体工具".into(), false),
    }
}
