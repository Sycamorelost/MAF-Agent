//! 绘图生成 / 图片下载安全校验 / 绘图历史。

use base64::Engine as _;
use serde::Deserialize;

use crate::db::usage::{self, UsageRecord};
use crate::provider::{rotation::{self, RotationState}, ImageRequest, ImageResponse};

use super::failover::with_key_rotation;
use super::keys::{build_openai, own_candidates, resolve_spec, rotation_group};
use super::pricing::{cost_nano_units, pricing_for};

// ---------- generate_image ----------

#[derive(Debug, Deserialize)]
pub struct GenerateImageInput {
    pub provider_id: String,
    pub model: String,
    pub prompt: String,
    /// 负面提示(合并进 prompt:`. Avoid: ...`)
    pub negative: Option<String>,
    /// 风格词(由前端风格 chip 给出英文关键词,合并进 prompt)
    pub style_prompt: Option<String>,
    /// 风格标签名(仅记录 meta)
    pub style: Option<String>,
    /// 预留:种子(当前内置 Provider 均不支持,仅记录 meta)
    pub seed: Option<i64>,
    /// 预留:Guidance(同上)
    pub guidance: Option<f32>,
    pub size: Option<String>,
    pub n: Option<u32>,
    pub custom_base_url: Option<String>,
    pub custom_display_name: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct ImageResultItem {
    /// 落盘绝对路径(项目 data/images/ 下)
    pub path: String,
    /// base64 data URI,前端直接渲染(未启用 asset protocol,避免路径协议配置)
    pub data_url: String,
    pub revised_prompt: Option<String>,
}

/// 图片落盘目录:`<data_dir>/images`
fn images_dir() -> std::path::PathBuf {
    crate::paths::data_dir().join("images")
}

/// 生成参数留档(D5.4):与 png 同名同目录的 meta.json。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageMeta {
    pub prompt: String,
    pub negative: Option<String>,
    pub style: Option<String>,
    pub size: String,
    pub model: String,
    pub provider_id: String,
    pub seed: Option<i64>,
    pub guidance: Option<f32>,
    pub created: String,
}

/// 生成图像。n 张图逐张调用 Provider(单张生成,职责单一),全部落盘后返回。
/// Phase 3:多 Key 时按轮换顺序尝试,遇 429/5xx 冷却该 Key 并换下一把(每张图独立选 Key)。
/// 兼容 b64_json(解码写入)与 url(下载后写入)两种 Provider 返回。
#[tauri::command]
pub async fn generate_image(
    state: tauri::State<'_, RotationState>,
    input: GenerateImageInput,
) -> Result<Vec<ImageResultItem>, String> {
    let spec = resolve_spec(
        &input.provider_id,
        &input.model,
        input.custom_base_url.as_deref(),
        input.custom_display_name.as_deref(),
    )?;

    let cfg = rotation::config_for(&input.provider_id);
    let group = rotation_group(&input.provider_id, &spec.base_url);
    let cands = own_candidates(&input.provider_id, &cfg);
    if cands.is_empty() {
        return Err(format!("provider {} 未配置 API Key", input.provider_id));
    }

    let n = input.n.unwrap_or(1).clamp(1, 4);
    let size = input.size.clone().unwrap_or_else(|| "1024x1024".into());

    // 风格词与负面提示合并进 prompt(对所有 OpenAI 兼容 Provider 生效)
    let mut final_prompt = input.prompt.clone();
    if let Some(sp) = input.style_prompt.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        final_prompt = format!("{final_prompt}, {sp}");
    }
    if let Some(neg) = input.negative.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        final_prompt = format!("{final_prompt}. Avoid: {neg}");
    }

    let meta = ImageMeta {
        prompt: input.prompt.clone(),
        negative: input.negative.clone(),
        style: input.style.clone(),
        size: size.clone(),
        model: input.model.clone(),
        provider_id: input.provider_id.clone(),
        seed: input.seed,
        guidance: input.guidance,
        created: chrono::Local::now().to_rfc3339(),
    };

    let mut results = Vec::with_capacity(n as usize);
    for i in 0..n {
        let resp = with_key_rotation(&group, cands.clone(), &state, &cfg, |_cand, key| {
            let provider = build_openai(&spec, &key, false);
            let req = ImageRequest {
                model: input.model.clone(),
                prompt: final_prompt.clone(),
                size: Some(size.clone()),
                n: Some(1),
                style: None,
            };
            async move { provider.generate_image(req).await }
        })
        .await?;

        results.push(save_image_response(resp, Some(&meta)).await?);

        // 生图用量(V17):按「张」计价落一行,避免生图成本恒为 0。
        // with_key_rotation 内部选槽,这里不冒充具体槽位:固定 0 并标注「生图」,
        // 保证 Key 健康表不会被错误归因。
        let pricing = pricing_for(&input.provider_id, &input.model);
        let cost_nano = pricing.as_ref().and_then(|p| cost_nano_units(p, 1, 0));
        let (pricing_snapshot, cost_source) = match (&pricing, cost_nano) {
            (Some(p), Some(_)) => (Some(p.snapshot.clone()), Some(p.source.clone())),
            (Some(p), None) => (Some(p.snapshot.clone()), Some("unpriced".into())),
            (None, _) => (None, Some("none".into())),
        };
        usage::record(&UsageRecord {
            request_id: format!("img-{}-{i}", chrono::Utc::now().timestamp_millis()),
            provider_id: input.provider_id.clone(),
            provider_name: Some(spec.display_name.clone()),
            model_id: input.model.clone(),
            slot_id: 0,
            slot_label: Some("生图".into()),
            status: "success".into(),
            cost_nano,
            pricing_snapshot,
            cost_source,
            ..Default::default()
        });
    }
    Ok(results)
}

/// 把 ImageResponse 落盘到 data/images/,同时写同名 meta.json;返回路径 + data URI。
async fn save_image_response(
    resp: ImageResponse,
    meta: Option<&ImageMeta>,
) -> Result<ImageResultItem, String> {
    let dir = images_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 images 目录失败: {e}"))?;

    // 统一拿到图片字节
    let bytes: Vec<u8> = if let Some(b64) = resp.url.strip_prefix("data:image/png;base64,") {
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("base64 解码失败: {e}"))?
    } else if resp.url.starts_with("http://") || resp.url.starts_with("https://") {
        download_image_checked(&resp.url).await?
    } else {
        return Err(format!("未知的图片返回格式: {}", &resp.url[..resp.url.len().min(64)]));
    };

    // 毫秒时间戳,避免同秒多图撞名
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S%3f");
    let path = dir.join(format!("{ts}.png"));
    std::fs::write(&path, &bytes).map_err(|e| format!("写入图片失败: {e}"))?;

    // 参数留档 meta.json(失败不影响出图)
    if let Some(m) = meta {
        if let Ok(json) = serde_json::to_string_pretty(m) {
            let _ = std::fs::write(dir.join(format!("{ts}.meta.json")), json);
        }
    }

    let data_url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    );
    Ok(ImageResultItem {
        path: path.to_string_lossy().to_string(),
        data_url,
        revised_prompt: resp.revised_prompt,
    })
}

// ---------- 图片 URL 下载安全校验(M2,SSRF 防护) ----------

/// 远程图片大小上限:20MB。
const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

/// 校验 IP 是否为公网可达地址(白名单制)。
/// 覆盖回环/私网/链路本地/ULA/未指定/保留/文档/CGNAT/组播等全部非公网段;
/// IPv4-mapped/compatible IPv6 还原为 v4 再判,防止映射形式绕过(CWE-918,M-8)。
/// 不使用 std 的 is_global(在当前工具链对 IpAddr 仍为 unstable)。
fn assert_ip_allowed(ip: std::net::IpAddr) -> Result<(), String> {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let o = v4.octets();
            let non_public = v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                // 保留段 240.0.0.0/4(is_reserved 在当前工具链为 unstable,手动判定)
                || o[0] >= 240
                // 0.0.0.0/8「本网段」保留地址(也兜住 IPv4-compatible ::a.b.c.d 还原后绕过)
                || o[0] == 0
                // CGNAT 100.64.0.0/10(is_shared 在部分工具链版本不可用,手动判定)
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                // 基准测试 198.18.0.0/15
                || (o[0] == 198 && (18..=19).contains(&o[1]));
            if non_public {
                return Err(format!("图片 URL 指向非公网地址 {v4},已拒绝"));
            }
        }
        std::net::IpAddr::V6(v6) => {
            // 注意顺序:::1 / :: 也会被 to_ipv4() 解释成 0.0.0.1 / 0.0.0.0,
            // 必须先挡回环与未指定,再做内嵌 v4 还原。
            if v6.is_loopback() || v6.is_unspecified() {
                return Err(format!("图片 URL 指向非公网地址 {v6},已拒绝"));
            }
            // ::ffff:a.b.c.d(mapped)与 ::a.b.c.d(compatible)按内嵌 v4 规则判定
            if let Some(v4) = v6.to_ipv4() {
                return assert_ip_allowed(std::net::IpAddr::V4(v4));
            }
            let s = v6.segments();
            let non_public = (s[0] & 0xffc0 == 0xfe80) // 链路本地 fe80::/10
                || (s[0] & 0xfe00 == 0xfc00) // 唯一本地 ULA fc00::/7
                || s[0] == 0xff00 // 组播 ff00::/8
                || (s[0] == 0x2001 && s[1] == 0x0db8) // 文档 2001:db8::/32
                || s[0] == 0x0100; // 黑洞 100::/64(覆盖 100::/64 等丢弃前缀)
            if non_public {
                return Err(format!("图片 URL 指向非公网地址 {v6},已拒绝"));
            }
        }
    }
    Ok(())
}

/// 校验图片 URL:仅 http/https;host 为 IP 字面量时要求是公网地址,
/// 防止生成结果里的 URL 被用作内网探测跳板(CWE-918)。
/// 域名形式的解析后校验在下载前由 `assert_domain_resolves_public` 完成。
fn validate_image_url(url: &str) -> Result<(), String> {
    let u = reqwest::Url::parse(url).map_err(|e| format!("图片 URL 无法解析: {e}"))?;
    match u.scheme() {
        "http" | "https" => {}
        s => return Err(format!("图片 URL 协议不支持: {s}")),
    }
    let host = u.host_str().ok_or_else(|| "图片 URL 缺少 host".to_string())?;
    // url crate 对 IPv6 字面量返回带方括号形式([::1]),统一去括号后再判定
    let h = host.trim_start_matches('[').trim_end_matches(']');
    if h.eq_ignore_ascii_case("localhost") {
        return Err("图片 URL 指向本机回环,已拒绝".into());
    }
    if let Ok(ip) = h.parse::<std::net::IpAddr>() {
        assert_ip_allowed(ip)?;
    }
    Ok(())
}

/// 域名形式的 host:DNS 解析后逐一校验所有结果地址均为公网(M-8)。
/// 若域名同时返回公网与内网 A 记录(如 DNS rebinding 预备态),只要有一个内网地址即拒绝。
/// 注:校验与 reqwest 实际连接之间仍存在极小的解析-连接 TOCTOU 窗口,
/// 彻底消除需自定义连接器做 IP pinning;当前在可用性与安全间取平衡。
async fn assert_domain_resolves_public(url: &str) -> Result<(), String> {
    let u = reqwest::Url::parse(url).map_err(|e| format!("图片 URL 无法解析: {e}"))?;
    let host = u.host_str().ok_or_else(|| "图片 URL 缺少 host".to_string())?;
    // 去 IPv6 方括号;能 parse 成 IP 的已由 validate_image_url 同步校验,此处只处理域名
    let h = host.trim_start_matches('[').trim_end_matches(']');
    if h.parse::<std::net::IpAddr>().is_ok() || h.eq_ignore_ascii_case("localhost") {
        return Ok(());
    }
    let port = u.port_or_known_default().unwrap_or(443);
    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((h, port))
        .await
        .map_err(|e| format!("图片域名解析失败 {h}: {e}"))?
        .collect();
    if addrs.is_empty() {
        return Err(format!("图片域名 {h} 无可用解析地址"));
    }
    for sa in &addrs {
        assert_ip_allowed(sa.ip())?;
    }
    Ok(())
}

/// 带校验的图片下载:禁用自动重定向后手动逐跳校验(防 SSRF),再验 Content-Type,全程 20MB 上限。
pub(crate) async fn download_image_checked(url: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("构造图片下载客户端失败: {e}"))?;

    let mut current = url.to_string();
    // 最多跟随 5 跳重定向,每跳都过 SSRF 校验
    for _ in 0..5 {
        validate_image_url(&current)?;
        assert_domain_resolves_public(&current).await?;
        let mut resp = client
            .get(&current)
            .send()
            .await
            .map_err(|e| format!("下载图片失败: {e}"))?;

        // 3xx 重定向:取 Location 手动跟随
        if resp.status().is_redirection() {
            if let Some(loc) = resp.headers().get(reqwest::header::LOCATION) {
                let loc_str = loc.to_str().unwrap_or("").to_string();
                let new_url = if loc_str.starts_with("http://") || loc_str.starts_with("https://") {
                    loc_str
                } else {
                    let base = reqwest::Url::parse(&current)
                        .map_err(|e| format!("重定向基准解析失败: {e}"))?;
                    base.join(&loc_str)
                        .map_err(|e| format!("重定向相对路径拼接失败: {e}"))?
                        .to_string()
                };
                current = new_url;
                continue;
            }
            return Err("重定向响应缺少 Location,已拒绝".into());
        }

        // 最终响应:校验 Content-Type + 尺寸上限
        let ct = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_lowercase();
        if !ct.starts_with("image/") {
            return Err(format!("图片响应 Content-Type 非法: {ct}"));
        }
        if let Some(len) = resp.content_length() {
            if len as usize > MAX_IMAGE_BYTES {
                return Err(format!("图片超过 20MB 上限({len} 字节)"));
            }
        }

        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| format!("读取图片 body 失败: {e}"))?
        {
            if buf.len() + chunk.len() > MAX_IMAGE_BYTES {
                return Err("图片超过 20MB 上限".into());
            }
            buf.extend_from_slice(&chunk);
        }
        return Ok(buf);
    }
    Err("图片下载重定向超过 5 跳,已拒绝".into())
}

// ---------- 绘画历史(D5.4) ----------
/// 历史列表条目:缩略图 data URL + 参数 meta。
#[derive(Debug, serde::Serialize)]
pub struct ImageListItem {
    pub path: String,
    /// 256px JPEG 缩略图 data URL
    pub thumb: String,
    pub meta: Option<ImageMeta>,
}

/// 校验路径必须位于 images 目录内(防越权读盘)。
fn ensure_in_images_dir(p: &std::path::Path) -> Result<(), String> {
    let dir = images_dir().canonicalize().map_err(|e| e.to_string())?;
    let canon = p.canonicalize().map_err(|e| format!("路径无效: {e}"))?;
    if canon.starts_with(&dir) {
        Ok(())
    } else {
        Err("路径不在 images 目录内".into())
    }
}

/// 列出全部历史作品(按文件名时间戳倒序),附缩略图与 meta。
#[tauri::command]
pub fn list_images() -> Result<Vec<ImageListItem>, String> {
    let dir = images_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names: Vec<String> = Vec::new();
    let rd = std::fs::read_dir(&dir).map_err(|e| format!("读取 images 目录失败: {e}"))?;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.ends_with(".png") {
            names.push(name);
        }
    }
    names.sort_by(|a, b| b.cmp(a)); // 时间戳命名,倒序即最新在前

    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let path = dir.join(&name);
        // 缩略图:按字节内容嗅探格式解码(部分渠道实际返回 JPEG 但沿用 .png 命名,
        // image::open 会按扩展名误选 PNG 解码器导致整张被跳过)→ 256 等比缩 → jpeg q70 → data URL
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(img) = image::load_from_memory(&bytes) else { continue };
        let thumb_img = img.thumbnail(256, 256);
        let mut buf = std::io::Cursor::new(Vec::new());
        if thumb_img
            .write_to(&mut buf, image::ImageFormat::Jpeg)
            .is_err()
        {
            continue;
        }
        let thumb = format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
        );
        let meta_path = dir.join(name.replace(".png", ".meta.json"));
        let meta = std::fs::read_to_string(&meta_path)
            .ok()
            .and_then(|t| serde_json::from_str::<ImageMeta>(&t).ok());
        out.push(ImageListItem {
            path: path.to_string_lossy().to_string(),
            thumb,
            meta,
        });
    }
    Ok(out)
}

/// 读取单张图片的完整 data URL(历史列表点击放大时用)。
#[tauri::command]
pub fn read_image(path: String) -> Result<String, String> {
    let p = std::path::PathBuf::from(&path);
    ensure_in_images_dir(&p)?;
    let bytes = std::fs::read(&p).map_err(|e| format!("读取图片失败: {e}"))?;
    // 文件统一 .png 命名,但部分渠道返回的是 JPEG;按字节内容标注真实 MIME。
    let mime = match image::guess_format(&bytes) {
        Ok(image::ImageFormat::Jpeg) => "image/jpeg",
        Ok(image::ImageFormat::WebP) => "image/webp",
        Ok(image::ImageFormat::Gif) => "image/gif",
        _ => "image/png",
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// 「上传到对话」:把已生成图片复制为指定会话的图片附件,返回附件元数据。
#[tauri::command]
pub async fn image_to_session(
    image_path: String,
    session_id: String,
) -> Result<crate::commands::attachment::AttachmentMeta, String> {
    let src = std::path::PathBuf::from(&image_path);
    ensure_in_images_dir(&src)?;
    let bytes = std::fs::read(&src).map_err(|e| format!("读取图片失败: {e}"))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let name = src
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image.png".into());
    crate::commands::attachment::save_attachment(session_id, name, b64).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M2 回归:私网/回环图片 URL 必须被拒绝(SSRF)。
    #[test]
    fn image_url_rejects_private_ip() {
        assert!(validate_image_url("http://192.168.1.1/x.png").is_err());
        assert!(validate_image_url("http://10.0.0.5/x.png").is_err());
        assert!(validate_image_url("http://172.16.0.9/x.png").is_err());
        assert!(validate_image_url("http://127.0.0.1/x.png").is_err());
        assert!(validate_image_url("http://169.254.1.1/x.png").is_err());
        assert!(validate_image_url("http://localhost/x.png").is_err());
        assert!(validate_image_url("http://[::1]/x.png").is_err());
        assert!(validate_image_url("file:///C:/x.png").is_err());
    }

    /// M2 回归:公网 http/https 图片 URL 放行。
    #[test]
    fn image_url_allows_public_host() {
        assert!(validate_image_url("https://oaidalleapiprodscus.blob.core.windows.net/x.png").is_ok());
        assert!(validate_image_url("http://8.8.8.8/x.png").is_ok());
    }
}
