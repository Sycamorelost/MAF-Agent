//! 终稿 AI 概念示意草稿与图示落盘。

use crate::provider::ImageRequest;

use super::EngineParams;
use super::skills;
use super::store;

/// 科研增强:终稿追加 AI 概念示意草稿(默认关)。任何一步失败都静默降级,不影响报告交付。
pub(crate) async fn attach_image_draft(p: &EngineParams, report: String) -> String {
    if !p.params.skill_enhance || !p.params.skill_image_draft {
        return report;
    }
    let (Some(provider_id), Some(model)) = (
        p.params.skill_image_provider.as_deref(),
        p.params.skill_image_model.as_deref(),
    ) else {
        return report;
    };
    if provider_id.is_empty() || model.is_empty() {
        return report;
    }
    // 从报告 TL;DR 提炼草稿主题(缺 TL;DR 时用主题句)
    let tldr = extract_tldr_section(&report).unwrap_or_else(|| p.topic.clone());
    let req = ImageRequest {
        model: model.to_string(),
        prompt: skills::image_draft_prompt(&p.topic, &tldr),
        size: Some("1024x1024".into()),
        n: Some(1),
        style: None,
    };
    let resp = match p.provider.generate_image(req).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[research] AI 草稿生成失败(静默降级): {e}");
            return report;
        }
    };
    let fname = match save_figure(p, &resp.url).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[research] AI 草稿落盘失败(静默降级): {e}");
            return report;
        }
    };
    // 追加草稿章节:图注明确标注 AI 生成、不构成证据
    let mut out = report;
    out.push_str("\n\n## 概念示意草稿\n\n");
    out.push_str(&format!(
        "![AI 生成概念草稿](figures/{fname})\n\n\
         > 上图为 AI 生成的概念示意草稿,仅辅助理解研究核心结论,不构成证据,不包含真实数据。\n"
    ));
    out
}

/// 从报告中提取 TL;DR 节正文(首个二级节,截到下一个二级标题)。
fn extract_tldr_section(report: &str) -> Option<String> {
    let start = report.find("## TL;DR")?;
    let rest = &report[start + "## TL;DR".len()..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    let body: String = rest[..end]
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('>'))
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = body.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// 图示字节上限,与图片下载器的 20MB 护栏一致(防止模型返回超大 data: URI 撑爆内存/磁盘,M-6)。
const FIGURE_MAX_BYTES: usize = 20 * 1024 * 1024;

/// 保存研究图示到 `data/research/{id}/figures/`,返回文件名。
/// 按字节内容嗅探真实格式(智谱等厂家可能返回 JPEG),统一走 base64/URL 双通道。
async fn save_figure(p: &EngineParams, url: &str) -> Result<String, String> {
    use base64::Engine as _;
    let bytes: Vec<u8> = if let Some(rest) = url
        .strip_prefix("data:")
        .and_then(|r| r.split_once(',').map(|(_, payload)| payload))
    {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(rest)
            .map_err(|e| format!("base64 解码失败: {e}"))?;
        if decoded.len() > FIGURE_MAX_BYTES {
            return Err(format!(
                "图示超过 {}MB 上限",
                FIGURE_MAX_BYTES / 1024 / 1024
            ));
        }
        decoded
    } else {
        // 外部 URL 下载复用带 SSRF 校验的受控下载器(禁自动重定向、逐跳校验、20MB 上限)
        crate::commands::provider::download_image_checked(url).await?
    };
    let ext = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "jpg"
    } else {
        "png"
    };
    let dir = store::research_dir(&p.id).join("figures");
    std::fs::create_dir_all(&dir).map_err(|e| format!("figures 目录创建失败: {e}"))?;
    let fname = format!(
        "draft-{}.{}",
        // 无点毫秒格式,与图示文件名「单点白名单」约束一致
        chrono::Local::now().format("%Y%m%d-%H%M%S%3f"),
        ext
    );
    std::fs::write(dir.join(&fname), &bytes).map_err(|e| format!("图示写入失败: {e}"))?;
    Ok(fname)
}
