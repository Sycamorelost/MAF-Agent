use image::ImageFormat;

/// 输出编码格式;auto = 保持服务商原始格式(有后处理时回退 png)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutFormat {
    Auto,
    Png,
    Jpeg,
}

pub(super) fn parse_format(raw: Option<&str>) -> OutFormat {
    match raw.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "png" => OutFormat::Png,
        "jpg" | "jpeg" => OutFormat::Jpeg,
        _ => OutFormat::Auto,
    }
}

/// 解析 "16:9"/"4:3"/"1:1" 为宽高比;非法值忽略。
pub(super) fn parse_aspect(raw: Option<&str>) -> Option<f64> {
    let s = raw?.trim();
    let (w, h) = s.split_once(':')?;
    let w: f64 = w.trim().parse().ok()?;
    let h: f64 = h.trim().parse().ok()?;
    (w > 0.0 && h > 0.0).then_some(w / h)
}

/// 按文件头判断扩展名,避免服务商固定声明 png 但实际返回 jpeg。
fn sniff_ext(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "jpg"
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "webp"
    } else if bytes.starts_with(b"GIF8") {
        "gif"
    } else {
        "png"
    }
}

/// 按需裁切比例、限制最长边并重新编码。无任何后处理时原样返回。
pub(super) fn postprocess(
    bytes: Vec<u8>,
    format: OutFormat,
    quality: u8,
    aspect: Option<f64>,
    max_side: u32,
) -> Result<(Vec<u8>, &'static str), String> {
    let src_ext = sniff_ext(&bytes);
    let needs_work = format != OutFormat::Auto || aspect.is_some() || max_side > 0;
    if !needs_work {
        return Ok((bytes, src_ext));
    }
    let mut img =
        image::load_from_memory(&bytes).map_err(|e| format!("图片解码失败(格式不支持):{e}"))?;

    if let Some(ratio) = aspect {
        let (w, h) = (img.width() as f64, img.height() as f64);
        if (w / h - ratio).abs() > 0.01 {
            let (nw, nh) = if w / h > ratio {
                ((h * ratio).round() as u32, img.height())
            } else {
                (img.width(), (w / ratio).round() as u32)
            };
            let x = (img.width() - nw) / 2;
            let y = (img.height() - nh) / 2;
            img = img.crop_imm(x, y, nw.max(1), nh.max(1));
        }
    }
    if max_side > 0 && (img.width() > max_side || img.height() > max_side) {
        img = img.resize(max_side, max_side, image::imageops::FilterType::Lanczos3);
    }

    let out = match format {
        OutFormat::Png | OutFormat::Auto => {
            let mut buf = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
                .map_err(|e| format!("PNG 编码失败:{e}"))?;
            (buf, "png")
        }
        OutFormat::Jpeg => {
            // JPEG 无透明通道,先铺白底避免黑块
            let rgb = img.to_rgb8();
            let mut buf = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality)
                .encode_image(&rgb)
                .map_err(|e| format!("JPEG 编码失败:{e}"))?;
            (buf, "jpg")
        }
    };
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_extension_from_magic() {
        assert_eq!(sniff_ext(&[0x89, b'P', b'N', b'G', 0]), "png");
        assert_eq!(sniff_ext(&[0xFF, 0xD8, 0xFF, 0]), "jpg");
        assert_eq!(sniff_ext(b"RIFF0000WEBPVP8 "), "webp");
        assert_eq!(sniff_ext(b"GIF89a"), "gif");
    }

    #[test]
    fn parses_aspect_ratios() {
        assert_eq!(parse_aspect(Some("16:9")), Some(16.0 / 9.0));
        assert_eq!(parse_aspect(Some("1:1")), Some(1.0));
        assert_eq!(parse_aspect(Some("bad")), None);
        assert_eq!(parse_aspect(Some("0:1")), None);
    }

    #[test]
    fn postprocess_crops_aspect_and_reencodes() {
        use image::GenericImageView;
        let img = image::RgbImage::from_fn(4, 2, |x, _| image::Rgb([x as u8, 0, 0]));
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();
        let (out, ext) = postprocess(png, OutFormat::Jpeg, 80, Some(1.0), 0).unwrap();
        assert_eq!(ext, "jpg");
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.dimensions(), (2, 2));
    }

    #[test]
    fn postprocess_keeps_bytes_when_no_transform() {
        let raw = vec![0x89, b'P', b'N', b'G', 1, 2, 3];
        let (out, ext) = postprocess(raw.clone(), OutFormat::Auto, 88, None, 0).unwrap();
        assert_eq!(out, raw);
        assert_eq!(ext, "png");
    }
}
