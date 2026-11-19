//! `ocr_extract`:调用 Windows 系统 OCR(Windows.Media.Ocr)识别图片文字。
//!
//! 选用系统 OCR 的原因:无需随包分发 PaddleOCR 模型与 MNN 运行库,
//! 也不引入 C++ 编译依赖;识别语言取决于系统已安装的 OCR 语言包。
//! 识别在 spawn_blocking 中串行执行,避免阻塞异步运行时。

use std::path::Path;

use serde_json::Value;
use windows::core::HSTRING;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapDecoder, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

const MAX_OUTPUT_CHARS: usize = 12000;

pub(crate) async fn exec(args: &Value, session_id: Option<&str>) -> (String, bool) {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if path.is_empty() {
        return ("图片识别失败:path 不能为空".into(), false);
    }
    let max_lines = args
        .get("max_lines")
        .and_then(Value::as_u64)
        .map(|n| n.clamp(1, 1000) as usize)
        .unwrap_or(400);
    let lang = args
        .get("lang")
        .and_then(Value::as_str)
        .unwrap_or("chs_eng")
        .to_string();

    let root = match crate::fs::paths::resolve_workspace_root(session_id) {
        Ok(r) => r,
        Err(e) => return (format!("图片识别失败:{e}"), false),
    };
    let file = match crate::fs::paths::resolve(&root, path, true) {
        Ok(p) => p,
        Err(e) => return (format!("图片识别失败:{e}"), false),
    };
    let ext = file
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg") {
        return (
            format!("图片识别失败:仅支持 png / jpg / jpeg,当前为 .{ext}"),
            false,
        );
    }

    let lang_for_ocr = lang.clone();
    let handle =
        tokio::task::spawn_blocking(move || recognize_blocking(&file, &lang_for_ocr));
    match handle.await {
        Ok(Ok(lines)) => (format_output(path, &lines, max_lines, &lang), true),
        Ok(Err(e)) => (format!("图片识别失败:{e}"), false),
        Err(e) => (format!("图片识别失败:任务中断({e})"), false),
    }
}

struct Line {
    text: String,
    y: f32,
    x: f32,
}

fn werr(e: windows::core::Error) -> String {
    format!("{} (0x{:08X})", e.message(), e.code().0)
}

fn engine_for(lang: &str) -> Result<OcrEngine, String> {
    let tags: &[&str] = match lang {
        "eng" => &["en-US", "en-GB"],
        "chs" => &["zh-Hans-CN", "zh-CN"],
        _ => &["zh-Hans-CN", "zh-CN", "en-US"],
    };
    for tag in tags {
        if let Ok(language) = Language::CreateLanguage(&HSTRING::from(*tag))
            && let Ok(engine) = OcrEngine::TryCreateFromLanguage(&language)
        {
            return Ok(engine);
        }
    }
    OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|_| "系统未安装可用的 OCR 语言包(设置 → 时间和语言 → 语言 → 可选功能中安装「光学字符识别」)".to_string())
}

fn recognize_blocking(path: &Path, lang: &str) -> Result<Vec<Line>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取图片失败: {e}"))?;
    let stream = InMemoryRandomAccessStream::new().map_err(werr)?;
    let writer = DataWriter::CreateDataWriter(&stream).map_err(werr)?;
    writer.WriteBytes(&bytes).map_err(werr)?;
    writer
        .StoreAsync()
        .and_then(|op| op.join())
        .map_err(werr)?;
    writer
        .FlushAsync()
        .and_then(|op| op.join())
        .map_err(werr)?;
    writer.DetachStream().map_err(werr)?;
    stream.Seek(0).map_err(werr)?;

    let decoder = BitmapDecoder::CreateAsync(&stream)
        .and_then(|op| op.join())
        .map_err(werr)?;
    let bitmap = decoder
        .GetSoftwareBitmapAsync()
        .and_then(|op| op.join())
        .map_err(werr)?;
    // OCR 引擎要求 BGRA8;Convert 会自动处理 alpha
    let bitmap =
        SoftwareBitmap::Convert(&bitmap, BitmapPixelFormat::Bgra8).map_err(werr)?;

    let engine = engine_for(lang)?;
    let result = engine
        .RecognizeAsync(&bitmap)
        .and_then(|op| op.join())
        .map_err(werr)?;

    let mut out = Vec::new();
    let lines = result.Lines().map_err(werr)?;
    for line in lines {
        let text = line.Text().map_err(werr)?.to_string();
        let text = text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        // 取本行第一个词的包围盒作为排序依据,保证多栏图片按视觉顺序输出
        let (mut x, mut y) = (0f32, 0f32);
        if let Ok(words) = line.Words()
            && let Some(word) = words.into_iter().next()
            && let Ok(rect) = word.BoundingRect()
        {
            x = rect.X;
            y = rect.Y;
        }
        out.push(Line { text, y, x });
    }
    out.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });
    Ok(out)
}

fn format_output(path: &str, lines: &[Line], max_lines: usize, lang: &str) -> String {
    if lines.is_empty() {
        return format!(
            "未识别到文字:{path}。可尝试提高图片清晰度、裁剪到文字区域,或换用更清晰的截图。"
        );
    }
    let mut body = format!("共 {} 行", lines.len());
    if lang == "eng" {
        body.push_str("(识别语言:英文)");
    }
    body.push('\n');
    let mut chars = body.chars().count();
    for l in lines.iter().take(max_lines) {
        chars += l.text.chars().count() + 1;
        if chars > MAX_OUTPUT_CHARS {
            body.push_str("…(输出过长,已截断)");
            break;
        }
        body.push_str(&l.text);
        body.push('\n');
    }
    body.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_lines_and_respects_max_lines() {
        let lines: Vec<Line> = (0..5)
            .map(|i| Line {
                text: format!("第{i}行"),
                y: i as f32,
                x: 0.0,
            })
            .collect();
        let out = format_output("a.png", &lines, 3, "chs_eng");
        assert!(out.contains("共 5 行"));
        assert!(out.contains("第0行"));
        assert!(!out.contains("第3行"));
    }

    #[test]
    fn empty_result_suggests_adjustments() {
        let out = format_output("a.png", &[], 400, "chs_eng");
        assert!(out.contains("未识别到文字"));
        assert!(out.contains("提高图片清晰度"));
    }

    #[test]
    fn chinese_language_tag_preferred() {
        // 仅验证语言标签映射不 panic;缺语言包时返回 Err 属正常
        let _ = engine_for("chs");
    }
}
