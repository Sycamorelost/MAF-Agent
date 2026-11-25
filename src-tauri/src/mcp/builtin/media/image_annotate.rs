//! 本地截图标注：所有打码先作用于源像素，再缩放并绘制可见标注。

use std::path::Path;

use image::{DynamicImage, GenericImageView, ImageFormat, Rgba, RgbaImage};
use serde_json::Value;

use super::{ComposeOutput, MAX_OUTPUT_BYTES, MAX_SIDE, bad, check_pixels, encode_png, text_arg, u32_arg};

const MAX_INPUT_BYTES: u64 = 20 * 1024 * 1024;
const MAX_MARKS: usize = 64;

#[derive(Clone, Copy)]
struct Rect { x: u32, y: u32, w: u32, h: u32 }

enum Mark {
    Number { x: u32, y: u32, n: u32, color: Rgba<u8> },
    Arrow { from: (u32, u32), to: (u32, u32), color: Rgba<u8> },
    Border { rect: Rect, color: Rgba<u8> },
    Pixelate { rect: Rect, block: u32 },
}

pub(super) fn path_is_input(args: &Value) -> bool {
    !has(args, "input_path") && !has(args, "source_path") && !has(args, "source") && has(args, "path")
}

fn has(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty())
}

fn input_arg(args: &Value) -> Result<String, String> {
    for key in ["input_path", "source_path", "source", "path"] {
        if args.get(key).is_some() {
            let value = text_arg(args, key, 1024, false)?;
            if !value.is_empty() { return Ok(value); }
        }
    }
    Err(bad("input_path", "本地 PNG/JPEG 路径不能为空"))
}

pub(super) fn render(args: &Value, root: &Path) -> Result<ComposeOutput, String> {
    let raw = input_arg(args)?;
    let unresolved = crate::fs::paths::resolve_guarded(root, &raw)?;
    let file = unresolved.canonicalize().map_err(|e| format!("输入图片不可访问 `{raw}`:{e}"))?;
    let ext = file.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    let format = match ext.as_str() {
        "png" => ImageFormat::Png,
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        _ => return Err(bad("input_path", "仅支持本地 PNG / JPEG 图片")),
    };
    let size = std::fs::metadata(&file).map_err(|e| format!("读取图片信息失败:{e}"))?.len();
    if size == 0 || size > MAX_INPUT_BYTES {
        return Err(format!("输入图片体积 {size} 字节不在 1..={MAX_INPUT_BYTES} 范围内"));
    }
    let bytes = std::fs::read(&file).map_err(|e| format!("读取输入图片失败:{e}"))?;
    if bytes.len() as u64 > MAX_INPUT_BYTES { return Err("输入图片超过 20 MiB 上限".into()); }
    let dimensions = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format)
        .into_dimensions().map_err(|e| format!("读取图片尺寸失败:{e}"))?;
    check_pixels(dimensions.0, dimensions.1)?;
    let source = image::load_from_memory_with_format(&bytes, format)
        .map_err(|e| format!("PNG/JPEG 解码失败:{e}"))?;
    let marks = parse_marks(args, dimensions.0, dimensions.1)?;
    let max_side = u32_arg(args, "max_side", 0, 0, MAX_SIDE)?;
    let mut out = compose(source, &marks, max_side)?;
    if out.len() > MAX_OUTPUT_BYTES { return Err("输出图片超过 24 MiB 上限".into()); }
    let name = format!("{}-annotated.png", super::slug(file.file_stem().and_then(|s| s.to_str()).unwrap_or("image")));
    let (width, height) = image::load_from_memory(&out).map_err(|e| format!("输出校验失败:{e}"))?.dimensions();
    check_pixels(width, height)?;
    Ok(ComposeOutput { bytes: std::mem::take(&mut out), width, height, default_name: name, source: Some(file) })
}

fn number(v: &Value, key: &str) -> Result<u32, String> {
    let n = v.get(key).and_then(Value::as_u64).ok_or_else(|| bad(key, "必须是非负整数"))?;
    if n > MAX_SIDE as u64 { return Err(bad(key, &format!("不能超过 {MAX_SIDE}"))); }
    Ok(n as u32)
}

fn rect_of(v: &Value) -> Result<Rect, String> {
    let r = Rect { x: number(v, "x")?, y: number(v, "y")?, w: number(v, "width")?, h: number(v, "height")? };
    if r.w == 0 || r.h == 0 { return Err(bad("width/height", "必须大于 0")); }
    Ok(r)
}

fn color_of(v: &Value) -> Result<Rgba<u8>, String> {
    match v.get("color") {
        None | Some(Value::Null) => super::rgba("#E11D48"),
        Some(Value::String(s)) => super::rgba(s.trim()).map_err(|_| bad("color", "必须是 #RRGGBB 颜色")),
        Some(_) => Err(bad("color", "必须是字符串")),
    }
}

fn parse_marks(args: &Value, width: u32, height: u32) -> Result<Vec<Mark>, String> {
    let items = args.get("annotations").and_then(Value::as_array).ok_or_else(|| bad("annotations", "必须是数组"))?;
    if items.is_empty() || items.len() > MAX_MARKS {
        return Err(bad("annotations", &format!("数量必须在 1..={MAX_MARKS} 之间")));
    }
    let mut marks = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        if !item.is_object() { return Err(bad(&format!("annotations[{i}]"), "必须是对象")); }
        let kind = text_arg(item, "type", 16, true)?;
        let mark = match kind.as_str() {
            "number" => {
                let n = match item.get("number") {
                    None | Some(Value::Null) => 0,
                    Some(Value::Number(n)) => n.as_u64().filter(|n| (1..=99).contains(n)).ok_or_else(|| bad("number", "必须在 1..=99 之间"))? as u32,
                    Some(_) => return Err(bad("number", "必须是整数")),
                };
                Mark::Number { x: number(item, "x")?, y: number(item, "y")?, n, color: color_of(item)? }
            }
            "arrow" => Mark::Arrow { from: (number(item, "x1")?, number(item, "y1")?),
                to: (number(item, "x2")?, number(item, "y2")?), color: color_of(item)? },
            "border" | "box" | "rect" => Mark::Border { rect: rect_of(item)?, color: color_of(item)? },
            "pixelate" | "mosaic" => {
                let block = u32_arg(item, "block_size", 16, 2, 256)?;
                Mark::Pixelate { rect: rect_of(item)?, block }
            }
            _ => return Err(bad(&format!("annotations[{i}].type"), "仅支持 number / arrow / border / pixelate")),
        };
        validate_mark(&mark, width, height).map_err(|e| format!("annotations[{i}]:{e}"))?;
        marks.push(mark);
    }
    Ok(marks)
}

fn validate_mark(mark: &Mark, width: u32, height: u32) -> Result<(), String> {
    let inside = |x, y| x < width && y < height;
    match mark {
        Mark::Number { x, y, .. } if !inside(*x, *y) => Err("编号坐标超出源图".into()),
        Mark::Arrow { from, to, .. } if !inside(from.0, from.1) || !inside(to.0, to.1) => Err("箭头坐标超出源图".into()),
        Mark::Border { rect, .. } | Mark::Pixelate { rect, .. } => {
            if rect.x as u64 + rect.w as u64 > width as u64 || rect.y as u64 + rect.h as u64 > height as u64 {
                Err("矩形超出源图边界".into())
            } else { Ok(()) }
        }
        _ => Ok(()),
    }
}

fn compose(source: DynamicImage, marks: &[Mark], max_side: u32) -> Result<Vec<u8>, String> {
    let (sw, sh) = source.dimensions();
    let mut base = source.to_rgba8();
    // 第一遍只打码，后续任何绘制都不会接触原始敏感像素。
    for mark in marks {
        if let Mark::Pixelate { rect, block } = mark { pixelate(&mut base, *rect, *block); }
    }
    let longest = sw.max(sh);
    let (ow, oh) = if max_side > 0 && longest > max_side {
        (((sw as u64 * max_side as u64 + longest as u64 / 2) / longest as u64).max(1) as u32,
         ((sh as u64 * max_side as u64 + longest as u64 / 2) / longest as u64).max(1) as u32)
    } else { (sw, sh) };
    check_pixels(ow, oh)?;
    let mut out = if (ow, oh) == (sw, sh) { base } else {
        image::imageops::resize(&base, ow, oh, image::imageops::FilterType::Lanczos3)
    };
    let sx = ow as f32 / sw as f32;
    let sy = oh as f32 / sh as f32;
    let thickness = (3.0 * sx.min(sy)).round().max(1.0) as i32;
    let mut sequence = 0;
    for mark in marks {
        match mark {
            Mark::Pixelate { .. } => {}
            Mark::Border { rect, color } => border(&mut out, map_rect(*rect, sx, sy), *color, thickness),
            Mark::Arrow { from, to, color } => arrow(&mut out, (scale(from.0, sx), scale(from.1, sy)),
                (scale(to.0, sx), scale(to.1, sy)), *color, thickness, sx.min(sy)),
            Mark::Number { x, y, n, color } => {
                sequence += 1;
                let n = if *n == 0 { sequence } else { *n };
                number_badge(&mut out, scale(*x, sx), scale(*y, sy), n, *color, sx.min(sy));
            }
        }
    }
    encode_png(DynamicImage::ImageRgba8(out))
}

fn pixelate(img: &mut RgbaImage, r: Rect, block: u32) {
    for by in (r.y..r.y + r.h).step_by(block as usize) {
        for bx in (r.x..r.x + r.w).step_by(block as usize) {
            let xe = (bx + block).min(r.x + r.w);
            let ye = (by + block).min(r.y + r.h);
            let mut sum = [0u64; 4];
            for y in by..ye { for x in bx..xe {
                for c in 0..4 { sum[c] += img.get_pixel(x, y).0[c] as u64; }
            }}
            let count = (xe - bx) as u64 * (ye - by) as u64;
            let avg = Rgba([ (sum[0]/count) as u8, (sum[1]/count) as u8, (sum[2]/count) as u8, (sum[3]/count) as u8 ]);
            for y in by..ye { for x in bx..xe { img.put_pixel(x, y, avg); }}
        }
    }
}

fn scale(v: u32, factor: f32) -> i32 { (v as f32 * factor).round() as i32 }
fn map_rect(r: Rect, sx: f32, sy: f32) -> Rect {
    let x = scale(r.x, sx).max(0) as u32;
    let y = scale(r.y, sy).max(0) as u32;
    Rect { x, y, w: (scale(r.x + r.w, sx).max(0) as u32 - x).max(1),
        h: (scale(r.y + r.h, sy).max(0) as u32 - y).max(1) }
}

fn put(img: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>) {
    if x >= 0 && y >= 0 && x < img.width() as i32 && y < img.height() as i32 { img.put_pixel(x as u32, y as u32, color); }
}

fn dot(img: &mut RgbaImage, x: i32, y: i32, radius: i32, color: Rgba<u8>) {
    for dy in -radius..=radius { for dx in -radius..=radius {
        if dx * dx + dy * dy <= radius * radius { put(img, x + dx, y + dy, color); }
    }}
}

fn line(img: &mut RgbaImage, mut a: (i32, i32), b: (i32, i32), color: Rgba<u8>, thickness: i32) {
    let dx = (b.0 - a.0).abs();
    let dy = -(b.1 - a.1).abs();
    let sx = if a.0 < b.0 { 1 } else { -1 };
    let sy = if a.1 < b.1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        dot(img, a.0, a.1, (thickness - 1) / 2, color);
        if a == b { break; }
        let e2 = err * 2;
        if e2 >= dy { err += dy; a.0 += sx; }
        if e2 <= dx { err += dx; a.1 += sy; }
    }
}

fn arrow(img: &mut RgbaImage, from: (i32, i32), to: (i32, i32), color: Rgba<u8>, thickness: i32, factor: f32) {
    line(img, from, to, color, thickness);
    let angle = (to.1 - from.1) as f32;
    let angle = angle.atan2((to.0 - from.0) as f32);
    let length = (18.0 * factor).max(8.0);
    for offset in [-0.55, 0.55] {
        let end = (to.0 - (length * (angle + offset).cos()) as i32, to.1 - (length * (angle + offset).sin()) as i32);
        line(img, to, end, color, thickness);
    }
}

fn border(img: &mut RgbaImage, r: Rect, color: Rgba<u8>, thickness: i32) {
    let (x0, y0) = (r.x as i32, r.y as i32);
    let (x1, y1) = (x0 + r.w as i32 - 1, y0 + r.h as i32 - 1);
    line(img, (x0, y0), (x1, y0), color, thickness);
    line(img, (x1, y0), (x1, y1), color, thickness);
    line(img, (x1, y1), (x0, y1), color, thickness);
    line(img, (x0, y1), (x0, y0), color, thickness);
}

fn number_badge(img: &mut RgbaImage, x: i32, y: i32, n: u32, color: Rgba<u8>, factor: f32) {
    let radius = (18.0 * factor).max(8.0) as i32;
    dot(img, x, y, radius, color);
    let digits = n.to_string();
    let unit = (radius / 6).max(1);
    let total = (digits.len() as i32 * 4 - 1) * unit;
    let start = (x - total / 2, y - 5 * unit / 2);
    for (di, digit) in digits.bytes().enumerate() {
        let rows = digit_rows(digit);
        for (row, bits) in rows.iter().enumerate() { for col in 0..3 {
            if bits & (1 << (2 - col)) != 0 {
                for dy in 0..unit { for dx in 0..unit {
                    put(img, start.0 + (di as i32 * 4 + col) * unit + dx, start.1 + row as i32 * unit + dy, Rgba([255, 255, 255, 255]));
                }}
            }
        }}
    }
}

fn digit_rows(digit: u8) -> [u8; 5] {
    match digit {
        b'0' => [7, 5, 5, 5, 7], b'1' => [2, 6, 2, 2, 7], b'2' => [7, 1, 7, 4, 7],
        b'3' => [7, 1, 7, 1, 7], b'4' => [5, 5, 7, 1, 1], b'5' => [7, 4, 7, 1, 7],
        b'6' => [7, 4, 7, 5, 7], b'7' => [7, 1, 1, 1, 1], b'8' => [7, 5, 7, 5, 7],
        _ => [7, 5, 7, 5, 7],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_maps_border_coordinates() {
        let source = DynamicImage::ImageRgba8(RgbaImage::from_pixel(20, 10, Rgba([255, 255, 255, 255])));
        let red = Rgba([255, 0, 0, 255]);
        let png = compose(source, &[Mark::Border { rect: Rect { x: 2, y: 2, w: 4, h: 2 }, color: red }], 10).unwrap();
        let out = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(out.dimensions(), (10, 5));
        assert_eq!(*out.get_pixel(1, 1), red);
        assert_eq!(*out.get_pixel(9, 4), Rgba([255, 255, 255, 255]));
    }

    #[test]
    fn pixelation_erases_source_pixels_before_output() {
        let raw = RgbaImage::from_fn(8, 8, |x, y| Rgba([(x * 20) as u8, (y * 20) as u8, (x + y * 8) as u8, 255]));
        let original = *raw.get_pixel(1, 1);
        let png = compose(DynamicImage::ImageRgba8(raw), &[Mark::Pixelate { rect: Rect { x: 1, y: 1, w: 4, h: 4 }, block: 2 }], 0).unwrap();
        let out = image::load_from_memory(&png).unwrap().to_rgba8();
        let average = *out.get_pixel(1, 1);
        assert_ne!(average, original);
        assert_eq!(average, Rgba([30, 30, 13, 255]));
        for (x, y) in [(2, 1), (1, 2), (2, 2)] { assert_eq!(*out.get_pixel(x, y), average); }
        assert_eq!(*out.get_pixel(0, 0), Rgba([0, 0, 0, 255]));
    }
}
