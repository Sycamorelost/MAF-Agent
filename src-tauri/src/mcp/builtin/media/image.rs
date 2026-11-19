//! `image_generate`:调用 OpenAI 兼容的 `/images/generations` 生成图片并落盘。
//!
//! 失败语义:参数错误、未配置服务商/Key 时立即返回,不发请求;
//! 网络错误与 5xx 重试一次,4xx 直接返回服务商错误信息。

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::ImageFormat;
use serde_json::{json, Value};

const MAX_PROMPT_CHARS: usize = 4000;
const MAX_DOWNLOAD_BYTES: usize = 20 * 1024 * 1024;

/// 输出编码格式;auto = 保持服务商原始格式(有后处理时回退 png)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutFormat {
    Auto,
    Png,
    Jpeg,
}

fn parse_format(raw: Option<&str>) -> OutFormat {
    match raw.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "png" => OutFormat::Png,
        "jpg" | "jpeg" => OutFormat::Jpeg,
        _ => OutFormat::Auto,
    }
}

/// 解析 "16:9"/"4:3"/"1:1" 为宽高比;非法值忽略。
fn parse_aspect(raw: Option<&str>) -> Option<f64> {
    let s = raw?.trim();
    let (w, h) = s.split_once(':')?;
    let w: f64 = w.trim().parse().ok()?;
    let h: f64 = h.trim().parse().ok()?;
    (w > 0.0 && h > 0.0).then_some(w / h)
}

/// 按需裁切比例、限制最长边并重新编码。无任何后处理时原样返回。
fn postprocess(
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

pub(crate) async fn exec(args: &Value, session_id: Option<&str>) -> (String, bool) {
    match run(args, session_id).await {
        Ok(msg) => (msg, true),
        Err(e) => (
            format!(
                "图片生成失败:{e}。请确认模型是否为图像模型\
                 (OpenAI:gpt-image-1 / dall-e-3;智谱:cogview-3-flash / cogview-4),\
                 且服务商支持 /images/generations 接口。"
            ),
            false,
        ),
    }
}

/// 服务商返回的图片载荷:优先 base64,部分服务商只给 URL。
#[derive(Debug, PartialEq)]
enum Payload {
    B64(String),
    Url(String),
}

fn parse_response(v: &Value) -> Result<Payload, String> {
    let item = v
        .get("data")
        .and_then(|d| d.get(0))
        .ok_or_else(|| "响应缺少 data[0]".to_string())?;
    if let Some(b) = item.get("b64_json").and_then(Value::as_str)
        && !b.trim().is_empty()
    {
        return Ok(Payload::B64(b.trim().to_string()));
    }
    if let Some(u) = item
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|u| !u.is_empty())
    {
        return Ok(Payload::Url(u.to_string()));
    }
    Err("响应中既没有 b64_json 也没有 url".into())
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

/// 文件名片段:保留中英文与数字,其余字符折叠为 `-`。
fn slug_of(prompt: &str) -> String {
    let mut s = String::new();
    for ch in prompt.chars().take(24) {
        if ch.is_alphanumeric() || ch == '-' || ch == '_' {
            s.push(ch);
        } else if !s.ends_with('-') {
            s.push('-');
        }
    }
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "image".into() } else { s }
}

fn default_name(prompt: &str, ext: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{millis}-{}.{ext}", slug_of(prompt))
}

/// 解析保存路径:显式 path 优先(缺扩展名时补),否则写到 outputs/images/。
fn target_path(
    root: &Path,
    explicit: &str,
    prompt: &str,
    ext: &str,
) -> Result<PathBuf, String> {
    if explicit.trim().is_empty() {
        let dir = root.join("outputs").join("images");
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {e}"))?;
        return Ok(dir.join(default_name(prompt, ext)));
    }
    let mut p = crate::fs::paths::resolve(root, explicit, false)?;
    // 输出扩展名跟随最终编码格式,避免 .png 里装 JPEG 字节
    p.set_extension(ext);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    Ok(p)
}

async fn fetch_url(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(format!("不支持的图片地址:{url}"));
    }
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载图片失败:{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载图片失败:HTTP {}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取图片数据失败:{e}"))?;
    if bytes.len() > MAX_DOWNLOAD_BYTES {
        return Err("图片超过 20MB 上限".into());
    }
    Ok(bytes.to_vec())
}

async fn run(args: &Value, session_id: Option<&str>) -> Result<String, String> {
    let prompt = args
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if prompt.is_empty() {
        return Err("prompt 不能为空".into());
    }
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(format!("prompt 超过 {MAX_PROMPT_CHARS} 字符上限"));
    }
    let size = args
        .get("size")
        .and_then(Value::as_str)
        .unwrap_or("auto")
        .trim()
        .to_string();

    let settings = crate::settings::load();
    let provider = settings
        .image_provider_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("未配置图像服务商")?
        .to_string();
    let model = args
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            settings
                .image_model
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "gpt-image-1".into());

    let spec = crate::commands::provider::resolve_spec(&provider, &model, None, None)?;
    let slots = crate::provider::keychain::list_slots(&provider)?;
    let slot = *slots.first().ok_or("该服务商未配置 API Key")?;
    let api_key = crate::provider::keychain::load_slot(&provider, slot)?
        .filter(|k| !k.trim().is_empty())
        .ok_or("该服务商未配置 API Key")?;

    let mut body = json!({ "model": model, "prompt": prompt, "n": 1 });
    let is_gpt_image = model.starts_with("gpt-image");
    if size != "auto" || is_gpt_image {
        body["size"] = json!(size);
    }
    // 不传 response_format:gpt-image 不接受它,而多数 OpenAI 兼容端点(如智谱)
    // 默认返回 url;解析层已同时兼容 b64_json 与 url。

    let url = format!("{}/images/generations", spec.base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败:{e}"))?;

    let mut attempt = 0;
    let resp = loop {
        attempt += 1;
        match client
            .post(&url)
            .bearer_auth(&api_key)
            .json(&body)
            .send()
            .await
        {
            Ok(r) => break r,
            Err(e) if attempt < 2 => {
                log::warn!("图片生成请求失败,重试一次: {e}");
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
            Err(e) => return Err(format!("网络请求失败:{e}")),
        }
    };

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let brief: String = text.chars().take(400).collect();
        return Err(format!("服务商返回 HTTP {status}:{brief}"));
    }
    let json: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败:{e}"))?;
    let payload = parse_response(&json)?;

    let bytes = match payload {
        Payload::B64(b) => STANDARD
            .decode(b.as_bytes())
            .map_err(|e| format!("base64 解码失败:{e}"))?,
        Payload::Url(u) => fetch_url(&client, &u).await?,
    };
    if bytes.is_empty() {
        return Err("服务商返回了空图片".into());
    }
    let format = parse_format(args.get("format").and_then(Value::as_str));
    let quality = args
        .get("quality")
        .and_then(Value::as_u64)
        .unwrap_or(88)
        .clamp(40, 100) as u8;
    let aspect = parse_aspect(args.get("aspect").and_then(Value::as_str));
    let max_side = args
        .get("max_side")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(8192) as u32;
    let (bytes, ext) = postprocess(bytes, format, quality, aspect, max_side)?;
    let root = crate::fs::paths::resolve_workspace_root(session_id)?;
    let explicit = args.get("path").and_then(Value::as_str).unwrap_or("");
    let target = target_path(&root, explicit, prompt, ext)?;
    std::fs::write(&target, &bytes).map_err(|e| format!("写入图片失败: {e}"))?;

    let rel = crate::fs::paths::rel_of(&root, &target)
        .unwrap_or_else(|_| target.display().to_string().replace('\\', "/"));
    let (note, ok) = crate::deliverables::present(
        session_id,
        &json!({ "path": rel, "kind": "image", "name": target.file_name().and_then(|s| s.to_str()) }),
    );
    if !ok {
        log::warn!("登记图片产物失败: {note}");
    }
    Ok(format!(
        "已生成图片:{rel}({} KB){}",
        bytes.len() / 1024,
        if ok { ",已登记产物" } else { "" }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_b64_over_url() {
        let v = json!({ "data": [{ "b64_json": "QUJD", "url": "https://x/y.png" }] });
        assert_eq!(parse_response(&v).unwrap(), Payload::B64("QUJD".into()));
    }

    #[test]
    fn falls_back_to_url() {
        let v = json!({ "data": [{ "url": "https://x/y.png" }] });
        assert_eq!(
            parse_response(&v).unwrap(),
            Payload::Url("https://x/y.png".into())
        );
    }

    #[test]
    fn reports_missing_payload() {
        assert!(parse_response(&json!({ "data": [] })).is_err());
        assert!(parse_response(&json!({})).is_err());
    }

    #[test]
    fn detects_extension_from_magic() {
        assert_eq!(sniff_ext(&[0x89, b'P', b'N', b'G', 0]), "png");
        assert_eq!(sniff_ext(&[0xFF, 0xD8, 0xFF, 0]), "jpg");
        assert_eq!(sniff_ext(b"RIFF0000WEBPVP8 "), "webp");
        assert_eq!(sniff_ext(b"GIF89a"), "gif");
    }

    #[test]
    fn slug_keeps_chinese_and_strips_symbols() {
        assert_eq!(slug_of("赛博朋克 城市/夜景"), "赛博朋克-城市-夜景");
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
        let (out, ext) =
            postprocess(png, OutFormat::Jpeg, 80, Some(1.0), 0).unwrap();
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
