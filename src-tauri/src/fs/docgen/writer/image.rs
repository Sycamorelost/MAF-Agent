//! 图片定位、缩放与段落封装。

use std::path::{Path, PathBuf};

use docx_rs::*;
use image::GenericImageView;

/// 图片最大宽度(EMU,约 14.7cm),超出等比缩放。
const IMAGE_MAX_W: u64 = 5_300_000;
/// 图片最大高度(EMU,约 18cm)。
const IMAGE_MAX_H: u64 = 6_500_000;
/// 图片体积上限,防止把超大图塞进文档。
const IMAGE_MAX_BYTES: u64 = 20 * 1024 * 1024;

pub(super) fn image_para(base: &Path, src: &str, brk: bool) -> Option<Paragraph> {
    let path = resolve_image(base, src)?;
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() > IMAGE_MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).ok()?;

    let mut ew = w as u64 * 9525;
    let mut eh = h as u64 * 9525;
    if ew > IMAGE_MAX_W {
        eh = eh * IMAGE_MAX_W / ew;
        ew = IMAGE_MAX_W;
    }
    if eh > IMAGE_MAX_H {
        ew = ew * IMAGE_MAX_H / eh;
        eh = IMAGE_MAX_H;
    }
    let pic = Pic::new_with_dimensions(png.into_inner(), w, h).size(ew as u32, eh as u32);
    Some(
        Paragraph::new()
            .align(AlignmentType::Center)
            .page_break_before(brk)
            .add_run(Run::new().add_image(pic)),
    )
}

fn resolve_image(base: &Path, src: &str) -> Option<PathBuf> {
    let raw = src.trim().trim_matches(|c| c == '"' || c == '\'');
    if raw.is_empty() || raw.starts_with("http://") || raw.starts_with("https://") {
        return None;
    }
    let ext = Path::new(raw)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())?;
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg") {
        return None;
    }
    let path = Path::new(raw);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    path.is_file().then_some(path)
}
