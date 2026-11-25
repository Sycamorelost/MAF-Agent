//! 确定性海报、封面与本地截图标注；不调用图像模型，也不上传任何内容。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageFormat};
use serde_json::{Value, json};

#[path = "image_annotate.rs"]
mod annotate;

const MAX_SIDE: u32 = 8192;
const MAX_PIXELS: u64 = 16_000_000;
const MAX_OUTPUT_BYTES: usize = 24 * 1024 * 1024;

pub(super) async fn exec(args: &Value, session_id: Option<&str>) -> (String, bool) {
    match run(args, session_id) {
        Ok(msg) => (msg, true),
        Err(e) => {
            let code = if e.starts_with("参数错误") { "invalid_argument" } else { "compose_failed" };
            (json!({ "ok": false, "error": { "code": code, "message": format!("图片合成失败:{e}") } }).to_string(), false)
        }
    }
}

struct ComposeOutput {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    default_name: String,
    source: Option<PathBuf>,
}

fn run(args: &Value, session_id: Option<&str>) -> Result<String, String> {
    let mode = text_arg(args, "mode", 16, true)?;
    if !matches!(mode.as_str(), "poster" | "cover" | "annotate") {
        return Err(bad("mode", "仅支持 poster / cover / annotate"));
    }
    let root = crate::fs::paths::resolve_workspace_root(session_id)?;
    let out = if mode == "annotate" {
        annotate::render(args, &root)?
    } else {
        Design::parse(args, &mode)?.render()?
    };
    check_pixels(out.width, out.height)?;
    if out.bytes.len() > MAX_OUTPUT_BYTES {
        return Err(format!("输出图片为 {} 字节,超过 24 MiB 上限", out.bytes.len()));
    }
    let explicit = if mode == "annotate" && annotate::path_is_input(args) {
        text_arg(args, "output_path", 1024, false)?
    } else {
        let output = text_arg(args, "output_path", 1024, false)?;
        if output.is_empty() { text_arg(args, "path", 1024, false)? } else { output }
    };
    let target = output_path(&root, &explicit, &out.default_name)?;
    if out.source.as_ref().is_some_and(|p| same_path(p, &target)) {
        return Err(bad("output_path", "输出不能覆盖输入原图"));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建输出目录失败:{e}"))?;
    }
    std::fs::write(&target, &out.bytes).map_err(|e| format!("写入图片失败:{e}"))?;
    let rel = crate::fs::paths::rel_of(&root, &target)
        .unwrap_or_else(|_| target.display().to_string().replace('\\', "/"));
    let (note, registered) = crate::deliverables::present(session_id, &json!({
        "path": rel, "kind": "image", "name": target.file_name().and_then(|s| s.to_str())
    }));
    if !registered {
        log::warn!("登记合成图片失败:{note}");
    }
    Ok(format!("已合成图片:{rel}({}×{},{} KB){}", out.width, out.height,
        out.bytes.len() / 1024, if registered { ",已登记产物" } else { "" }))
}

fn output_path(root: &Path, explicit: &str, default_name: &str) -> Result<PathBuf, String> {
    if explicit.trim().is_empty() {
        return Ok(root.join("outputs").join("images").join(default_name));
    }
    let mut p = crate::fs::paths::resolve_guarded(root, explicit)?;
    p.set_extension("png");
    Ok(p)
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.canonicalize().unwrap_or_else(|_| a.to_path_buf())
        == b.canonicalize().unwrap_or_else(|_| b.to_path_buf())
}

fn bad(field: &str, message: &str) -> String {
    format!("参数错误 {field}:{message}")
}

fn text_arg(args: &Value, key: &str, max: usize, required: bool) -> Result<String, String> {
    let value = match args.get(key) {
        None | Some(Value::Null) => "",
        Some(Value::String(s)) => s.trim(),
        Some(_) => return Err(bad(key, "必须是字符串")),
    };
    if required && value.is_empty() {
        return Err(bad(key, "不能为空"));
    }
    if value.chars().count() > max {
        return Err(bad(key, &format!("超过 {max} 字符上限")));
    }
    Ok(value.to_string())
}

fn u32_arg(args: &Value, key: &str, default: u32, min: u32, max: u32) -> Result<u32, String> {
    let n: u64 = match args.get(key) {
        None | Some(Value::Null) => default as u64,
        Some(Value::Number(n)) => n.as_u64().ok_or_else(|| bad(key, "必须是非负整数"))?,
        Some(_) => return Err(bad(key, "必须是整数")),
    };
    if n < min as u64 || n > max as u64 {
        return Err(bad(key, &format!("必须在 {min}..={max} 之间")));
    }
    Ok(n as u32)
}

fn check_pixels(width: u32, height: u32) -> Result<(), String> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!("图片尺寸 {width}×{height} 超出 1..={MAX_SIDE} 限制"));
    }
    if width as u64 * height as u64 > MAX_PIXELS {
        return Err(format!("图片像素 {} 超过 {MAX_PIXELS} 上限", width as u64 * height as u64));
    }
    Ok(())
}

fn rgba(hex: &str) -> Result<image::Rgba<u8>, String> {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad("brand_color", "必须是 #RRGGBB 颜色"));
    }
    let channel = |i| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0);
    Ok(image::Rgba([channel(0), channel(2), channel(4), 255]))
}

struct Design {
    mode: String,
    title: String,
    subtitle: String,
    author: String,
    date: String,
    brand: String,
    width: u32,
    height: u32,
    safe: u32,
}

impl Design {
    fn parse(args: &Value, mode: &str) -> Result<Self, String> {
        let (dw, dh) = if mode == "poster" { (1200, 1600) } else { (1600, 900) };
        let width = u32_arg(args, "width", dw, 64, MAX_SIDE)?;
        let height = u32_arg(args, "height", dh, 64, MAX_SIDE)?;
        check_pixels(width, height)?;
        let brand = match args.get("brand_color") {
            None | Some(Value::Null) => "#2563EB".to_string(),
            Some(Value::String(s)) => s.trim().to_string(),
            Some(_) => return Err(bad("brand_color", "必须是字符串")),
        };
        rgba(&brand)?;
        let safe = u32_arg(args, "safe_area", if mode == "poster" { 72 } else { 64 }, 8, 1024)?;
        if safe > width.min(height) / 4 {
            return Err(bad("safe_area", "不能大于短边的四分之一"));
        }
        Ok(Self { mode: mode.into(), title: text_arg(args, "title", 120, true)?,
            subtitle: text_arg(args, "subtitle", 240, false)?, author: text_arg(args, "author", 80, false)?,
            date: text_arg(args, "date", 40, false)?, brand, width, height, safe })
    }

    fn render(&self) -> Result<ComposeOutput, String> {
        let poster = self.mode == "poster";
        let title_size = if poster { 64 } else { 56 };
        let available = self.width - self.safe * 2;
        let titles = wrap_lines(&self.title, (available as usize * 2 / title_size).max(4), if poster { 5 } else { 3 })?;
        let subtitles = wrap_lines(&self.subtitle, (available as usize * 2 / 28).max(4), 3)?;
        let (background, foreground) = if poster { ("#F8FAFC", "#0F172A") } else { (self.brand.as_str(), "#FFFFFF") };
        let mut svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"><rect width=\"100%\" height=\"100%\" fill=\"{background}\"/>", self.width, self.height, self.width, self.height);
        if poster {
            let _ = write!(svg, "<rect width=\"{}\" height=\"20\" fill=\"{}\"/><rect x=\"{}\" y=\"{}\" width=\"96\" height=\"8\" fill=\"{}\"/>", self.width, self.brand, self.safe, self.safe, self.brand);
        } else {
            let _ = write!(svg, "<circle cx=\"{}\" cy=\"0\" r=\"{}\" fill=\"#FFFFFF\" opacity=\"0.10\"/>", self.width, self.height / 2);
        }
        let title_y = if poster { self.safe + 150 } else { self.height * 35 / 100 };
        let mut y = title_y;
        for line in &titles {
            let _ = write!(svg, "<text x=\"{}\" y=\"{y}\" fill=\"{foreground}\" font-family=\"Microsoft YaHei,PingFang SC,Arial,sans-serif\" font-size=\"{title_size}\" font-weight=\"700\">{}</text>", self.safe, xml(line));
            y += title_size as u32 * 6 / 5;
        }
        y += 20;
        for line in &subtitles {
            let _ = write!(svg, "<text x=\"{}\" y=\"{y}\" fill=\"{foreground}\" opacity=\"0.78\" font-family=\"Microsoft YaHei,PingFang SC,Arial,sans-serif\" font-size=\"28\">{}</text>", self.safe, xml(line));
            y += 40;
        }
        let bottom = self.height - self.safe;
        if !self.author.is_empty() {
            let _ = write!(svg, "<text x=\"{}\" y=\"{bottom}\" fill=\"{foreground}\" font-family=\"Microsoft YaHei,PingFang SC,Arial,sans-serif\" font-size=\"24\">{}</text>", self.safe, xml(&self.author));
        }
        if !self.date.is_empty() {
            let _ = write!(svg, "<text x=\"{}\" y=\"{bottom}\" text-anchor=\"end\" fill=\"{foreground}\" opacity=\"0.75\" font-family=\"Arial,sans-serif\" font-size=\"22\">{}</text>", self.width - self.safe, xml(&self.date));
        }
        svg.push_str("</svg>");
        let bytes = charts_rs::svg_to_png_with_size(&svg, Some(self.width), Some(self.height))
            .map_err(|e| format!("SVG 光栅化失败:{e}"))?;
        Ok(ComposeOutput { bytes, width: self.width, height: self.height,
            default_name: format!("{}-{}.png", self.mode, slug(&self.title)), source: None })
    }
}

fn wrap_lines(text: &str, units: usize, max_lines: usize) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    for ch in text.chars() {
        if ch == '\n' {
            out.push(std::mem::take(&mut line));
            used = 0;
            continue;
        }
        let w = if ch.is_ascii() { 1 } else { 2 };
        if used + w > units && !line.is_empty() {
            out.push(std::mem::take(&mut line));
            used = 0;
        }
        line.push(ch);
        used += w;
    }
    if !line.is_empty() || text.is_empty() { out.push(line); }
    if out.len() > max_lines { return Err(bad("title/subtitle", &format!("按当前安全区排版需要 {} 行,最多 {max_lines} 行", out.len()))); }
    Ok(out)
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars().take(24) {
        if ch.is_alphanumeric() || ch == '-' || ch == '_' { out.push(ch); }
        else if !out.ends_with('-') { out.push('-'); }
    }
    let out = out.trim_matches('-');
    if out.is_empty() { "image".into() } else { out.into() }
}

fn encode_png(img: DynamicImage) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(|e| format!("PNG 编码失败:{e}"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poster_output_is_deterministic() {
        let args = json!({"title":"季度发布","subtitle":"产品与技术","author":"张三","date":"2026-11-23","brand_color":"#123ABC"});
        let a = Design::parse(&args, "poster").unwrap().render().unwrap();
        let b = Design::parse(&args, "poster").unwrap().render().unwrap();
        assert_eq!(a.bytes, b.bytes);
        assert!(a.bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn escapes_svg_and_rejects_bad_layout() {
        assert_eq!(xml("A<&\""), "A&lt;&amp;&quot;");
        assert!(wrap_lines("一二三四五六", 2, 2).is_err());
        assert!(Design::parse(&json!({"title":"x","brand_color":"red"}), "cover").is_err());
    }
}
