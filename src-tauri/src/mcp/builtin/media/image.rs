//! `image_generate`:调用 OpenAI 兼容的 `/images/generations` 生成图片并落盘。
//!
//! 失败语义:参数错误、未配置服务商/Key 时立即返回,不发请求;
//! 网络错误与 5xx 重试一次,4xx 直接返回服务商错误信息。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::image_postprocess::{parse_aspect, parse_format, postprocess};
use super::image_provider;

const MAX_PROMPT_CHARS: usize = 4000;

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
fn target_path(root: &Path, explicit: &str, prompt: &str, ext: &str) -> Result<PathBuf, String> {
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

    let bytes = image_provider::generate(&spec.base_url, &api_key, &model, prompt, &size).await?;
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
    fn slug_keeps_chinese_and_strips_symbols() {
        assert_eq!(slug_of("赛博朋克 城市/夜景"), "赛博朋克-城市-夜景");
    }
}
