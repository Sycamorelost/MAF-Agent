//! 应用设置 commands(迭代 D3.7):提炼专用小模型等,落盘 data/settings.json。
//!
//! Phase B:新增 `save_theme_wallpaper`,把用户选定的图片安全拷贝到
//! `<data_dir>/theme/wallpaper.<ext>`,返回绝对路径供前端持久化。
//! `read_wallpaper` 读取已存储的壁纸并返回 base64 data URL
//! (WebView2 禁止 http 页加载 file:// 资源,故统一走 data URL,与项目绘图/附件一致)。

use std::{fs, path::Path};

use base64::Engine as _;
use tauri_plugin_dialog::DialogExt;

use crate::settings::{self, AppSettings};

/// 读取应用设置(缺失时返回默认值)。
#[tauri::command]
pub fn get_settings() -> AppSettings {
    settings::load()
}

/// 保存应用设置(覆盖写)。
#[tauri::command]
pub fn save_settings(settings: AppSettings) -> Result<(), String> {
    settings::save(&settings)
}

// ---------------- 自定义壁纸(Phase B) ----------------

/// 弹出原生图片选择器;用户取消返回 None。
#[tauri::command]
pub async fn pick_wallpaper_file(app: tauri::AppHandle) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("图片", &["jpg", "jpeg", "png", "webp"])
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.display().to_string())
    })
    .await
    .ok()
    .flatten()
}

/// 壁纸文件大小上限:8 MB。
const WALLPAPER_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// 校验图片魔数并返回扩展名。
fn detect_image_ext(head: &[u8]) -> Option<&'static str> {
    if head.len() >= 2 && head[0] == 0xFF && head[1] == 0xD8 {
        return Some("jpg");
    }
    if head.len() >= 8 && head[0..4] == [0x89, 0x50, 0x4E, 0x47] {
        return Some("png");
    }
    // WebP:前 4 字节 "RIFF",第 8-11 字节 "WEBP"
    if head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        return Some("webp");
    }
    None
}

/// 保存用户壁纸:校验来源 → 拷贝到 data/theme/wallpaper.<ext> → 返回绝对路径。
///
/// 安全要点:
/// - 只接受真实存在的普通文件(拒绝目录/符号链接路径穿越);
/// - 大小 ≤ 8 MB;
/// - 魔数必须是 jpg/png/webp 之一(不依赖扩展名,防止伪装);
/// - 目标文件名固定(`wallpaper.<ext>`),避免用户路径中的空格/Unicode 注入。
#[tauri::command]
pub fn save_theme_wallpaper(source_path: String) -> Result<String, String> {
    let src = Path::new(&source_path);

    // 1. 来源必须是已存在的普通文件
    let meta = fs::metadata(src).map_err(|e| format!("无法读取源文件: {e}"))?;
    if !meta.is_file() {
        return Err("源路径不是普通文件".into());
    }

    // 2. 大小校验
    let len = meta.len();
    if len == 0 {
        return Err("壁纸文件为空".into());
    }
    if len > WALLPAPER_MAX_BYTES {
        return Err(format!(
            "壁纸过大({:.1} MB),上限 8 MB",
            len as f64 / 1024.0 / 1024.0
        ));
    }

    // 3. 魔数校验(读前 12 字节足够覆盖 jpg/png/webp)
    let mut f = fs::File::open(src).map_err(|e| format!("无法打开源文件: {e}"))?;
    let mut head = [0u8; 12];
    use std::io::Read;
    f.read_exact(&mut head).map_err(|e| format!("读取文件头失败: {e}"))?;
    let ext = detect_image_ext(&head).ok_or_else(|| {
        "仅支持 JPG / PNG / WebP 格式(请检查文件是否为真实图片)".to_string()
    })?;

    // 4. 拷贝到固定位置 <data_dir>/theme/wallpaper.<ext>
    let theme_dir = crate::paths::data_dir().join("theme");
    fs::create_dir_all(&theme_dir)
        .map_err(|e| format!("创建壁纸目录失败: {e}"))?;
    let dst = theme_dir.join(format!("wallpaper.{ext}"));
    fs::copy(src, &dst).map_err(|e| format!("拷贝壁纸失败: {e}"))?;

    // 5. 返回供持久化/展示的绝对路径(剥离 Windows \\?\ 前缀,反斜杠转正斜杠,
    //    仅用于 settings.json 存储与设置页展示;实际渲染走 read_wallpaper 的 data URL)。
    let canon = fs::canonicalize(&dst).map_err(|e| format!("规范化壁纸路径失败: {e}"))?;
    let mut s = canon.to_string_lossy().to_string();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        s = stripped.to_string();
    }
    s = s.replace('\\', "/");
    Ok(s)
}

/// 根据魔数推断图片 MIME 类型(仅支持 jpg/png/webp)。
fn mime_from_magic(head: &[u8]) -> &'static str {
    if head.len() >= 2 && head[0] == 0xFF && head[1] == 0xD8 {
        "image/jpeg"
    } else if head.len() >= 8 && head[0..4] == [0x89, 0x50, 0x4E, 0x47] {
        "image/png"
    } else if head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}

/// 读取已存储的壁纸文件,返回 base64 data URL 供前端直接渲染。
/// (WebView2 不允许 http 页加载 file:// 资源,故不返回路径而返回 data URL)
#[tauri::command]
pub fn read_wallpaper(path: String) -> Result<String, String> {
    let p = Path::new(&path);
    let bytes = fs::read(p).map_err(|e| format!("读取壁纸失败: {e}"))?;
    if bytes.is_empty() {
        return Err("壁纸文件为空".into());
    }
    let mime = mime_from_magic(&bytes);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}
