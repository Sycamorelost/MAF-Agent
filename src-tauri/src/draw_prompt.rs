//! 绘图 Prompt 组装(P0)。
//!
//! 调研结论(FLUX prompt upsampling / sdxl_prompt_styler / OneButtonPrompt /
//! sd-webui-prompt-all-in-one / danbooru-tags-upsampler)的核心共识:
//! - 按模型族分流:DALL·E 走自然语言、FLUX 走叙述式、SDXL 走 tag、CogView 中文原生;
//! - 关键词顺序固定为「主体 > 动作 > 风格 > 构图/光照 > 细节」,越靠前权重越高;
//! - 规则拼装优先(零额外 token),LLM 扩写只在用户主动触发时才值得做;
//! - 去重/去掉质量堆砌(8k/masterpiece/octane 等)并按模型上限截断,减少无效 token。
//!
//! 本模块只做纯函数组装:不发起网络请求,便于单测与前端实时预览。
//! 注意:当前 Provider 走 OpenAI 兼容 `/images/generations`,没有独立 negative 字段,
//! 负向词只能作为 `Avoid: ...` 文本附加——这一点在 warnings 里明确告知用户。

use serde::Serialize;

/// 模型族:决定 prompt 形态、长度上限与质量词策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptFamily {
    Sdxl,
    Flux,
    OpenAi,
    CogView,
    Generic,
}

impl PromptFamily {
    pub fn key(self) -> &'static str {
        match self {
            Self::Sdxl => "sdxl",
            Self::Flux => "flux",
            Self::OpenAi => "openai",
            Self::CogView => "cogview",
            Self::Generic => "generic",
        }
    }

    /// 给 UI 展示的模型族说明。
    pub fn label(self) -> &'static str {
        match self {
            Self::Sdxl => "SD/SDXL · 标签式 · 75 token/段",
            Self::Flux => "FLUX · 叙述式 · 越靠前的词权重越高",
            Self::OpenAi => "DALL·E · 自然语言长句",
            Self::CogView => "CogView · 中文原生描述",
            Self::Generic => "通用 OpenAI 兼容模型",
        }
    }

    /// 最终 Prompt 字符上限(按各模型族的有效长度折中,避免无效堆砌)。
    pub fn max_chars(self) -> usize {
        match self {
            Self::Sdxl => 300,    // 75 CLIP token≈300 字符
            Self::Flux => 1200,   // T5 512 token 的安全工作区
            Self::OpenAi => 2000, // DALL·E 上限 4000,但 2000 已足够且更省
            Self::CogView => 1000,
            Self::Generic => 800,
        }
    }

    fn narrative(self) -> bool {
        matches!(self, Self::Flux | Self::OpenAi | Self::CogView)
    }
}

/// 组装结果:前端实时预览与生成请求共用同一份数据,保证「看到的就是发出去的」。
#[derive(Debug, Clone, Serialize)]
pub struct ComposedPrompt {
    pub prompt: String,
    pub family: String,
    pub family_label: String,
    pub max_chars: usize,
    pub chars: usize,
    pub est_tokens: usize,
    pub truncated: bool,
    pub warnings: Vec<String>,
}

/// 按 Provider + 模型名识别模型族。
pub fn detect_family(provider_id: &str, model: &str) -> PromptFamily {
    let m = model.to_ascii_lowercase();
    let p = provider_id.to_ascii_lowercase();
    if m.contains("cogview") || m.contains("glm-image") {
        return PromptFamily::CogView;
    }
    if (p.contains("zhipu") || p.contains("glm")) && m.contains("image") {
        return PromptFamily::CogView;
    }
    if m.contains("flux") {
        return PromptFamily::Flux;
    }
    if m.contains("dall-e") || m.contains("dalle") || m.contains("gpt-image") {
        return PromptFamily::OpenAi;
    }
    if m.contains("stable-diffusion")
        || m.contains("sdxl")
        || m.contains("sd3")
        || m.contains("sd15")
        || m.contains("sd-")
    {
        return PromptFamily::Sdxl;
    }
    PromptFamily::Generic
}

/// 估算 token 数:中日韩字符≈1 token/字,ASCII≈4 字符/token(偏保守)。
pub fn estimate_tokens(s: &str) -> usize {
    let mut cjk = 0usize;
    let mut ascii = 0usize;
    for c in s.chars() {
        if is_cjk(c) {
            cjk += 1;
        } else if c.is_ascii() {
            ascii += 1;
        } else {
            cjk += 1;
        }
    }
    cjk + ascii.div_ceil(4)
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x303F   // CJK 标点
        | 0x3400..=0x4DBF // 扩展 A
        | 0x4E00..=0x9FFF // 基本区
        | 0xF900..=0xFAFF // 兼容区
        | 0xFF00..=0xFFEF // 全角
        | 0x20000..=0x2FA1F // 扩展 B+
    )
}

/// 组装最终 Prompt。
pub fn compose(
    prompt: &str,
    style_prompt: Option<&str>,
    negative: Option<&str>,
    provider_id: &str,
    model: &str,
) -> ComposedPrompt {
    let family = detect_family(provider_id, model);
    let max_chars = family.max_chars();

    // 1) 主体描述归一:叙述式保留句子,标签式按逗号拆分/去重
    let base = normalize_base(prompt, family);

    // 2) 风格关键词追加在主体之后(顺序:主体 > 动作 > 风格 > 光照 > 细节)
    let style = style_prompt
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| normalize_style(s, family))
        .unwrap_or_default();

    // 3) 负向词:当前通道无独立参数,统一转成 `Avoid: ...` 文本,并限制长度
    let neg = negative
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("Avoid: {}", normalize_fragment(s)))
        .unwrap_or_default();

    // 4) 预算分配:后缀(风格+负向)最多占 1/3,主体占大头
    let suffix_raw = match (style.is_empty(), neg.is_empty()) {
        (true, true) => String::new(),
        (false, true) => style.clone(),
        (true, false) => neg.clone(),
        (false, false) => format!("{style}, {neg}"),
    };
    let suffix_budget = if suffix_raw.is_empty() {
        0
    } else {
        (max_chars / 3).max(80)
    };
    let (suffix, suffix_cut) = truncate_fragment(&suffix_raw, suffix_budget);
    let sep = if suffix.is_empty() { 0 } else { 2 };
    let base_budget = max_chars.saturating_sub(suffix.chars().count() + sep);
    let (base_cut, base_truncated) = truncate_fragment(&base, base_budget);
    let truncated = base_truncated || suffix_cut;

    let mut final_prompt = base_cut;
    if !suffix.is_empty() {
        if final_prompt.is_empty() {
            final_prompt = suffix;
        } else {
            final_prompt = format!("{final_prompt}, {suffix}");
        }
    }
    final_prompt = final_prompt.trim().trim_matches(',').trim().to_string();

    // 5) 明确告知用户「为什么这样写」,避免静默改动
    let mut warnings = Vec::new();
    if truncated {
        warnings.push(format!(
            "已按「{}」上限 {} 字符截断,避免超出模型有效长度浪费 token。",
            family.label(),
            max_chars
        ));
    }
    if family != PromptFamily::CogView && contains_cjk(&final_prompt) {
        warnings.push(
            "当前模型对英文描述更敏感:中文可保留,但补充英文关键词通常能提高命中率。"
                .to_string(),
        );
    }
    if !neg.is_empty() {
        warnings.push(
            "该通道没有独立的 negative 参数,负向词已作为 `Avoid:` 文本附加在 Prompt 末尾。"
                .to_string(),
        );
    }

    let chars = final_prompt.chars().count();
    ComposedPrompt {
        est_tokens: estimate_tokens(&final_prompt),
        prompt: final_prompt,
        family: family.key().to_string(),
        family_label: family.label().to_string(),
        max_chars,
        chars,
        truncated,
        warnings,
    }
}

/// 主体描述归一化。
fn normalize_base(raw: &str, family: PromptFamily) -> String {
    let s = collapse_ws(raw);
    if s.is_empty() {
        return s;
    }
    if family.narrative() {
        // 叙述式:保留句子,仅去掉重复的分隔符与质量堆砌词
        let s = if matches!(family, PromptFamily::Flux | PromptFamily::OpenAi) {
            strip_quality_terms(&s)
        } else {
            s
        };
        trim_separators(&s)
    } else {
        // 标签式:拆分 + 忽略大小写去重,统一用 ", " 连接
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for seg in split_segments(&s) {
            let key = seg.to_lowercase();
            if seen.insert(key) {
                out.push(seg);
            }
        }
        out.join(", ")
    }
}

/// 风格词归一化:叙述式加 ", in ... style" 太啰嗦,统一用逗号追加关键词更稳。
fn normalize_style(raw: &str, family: PromptFamily) -> String {
    let s = collapse_ws(raw);
    if family.narrative() {
        trim_separators(&s)
    } else {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for seg in split_segments(&s) {
            let key = seg.to_lowercase();
            if seen.insert(key) {
                out.push(seg);
            }
        }
        out.join(", ")
    }
}

fn normalize_fragment(raw: &str) -> String {
    trim_separators(&collapse_ws(raw))
}

/// 质量堆砌词:对自然语言/叙述式模型无实际增益,反而占 token(调研结论)。
const QUALITY_SPAM: &[&str] = &[
    "8k",
    "4k",
    "16k",
    "masterpiece",
    "best quality",
    "ultra detailed",
    "highly detailed",
    "extremely detailed",
    "octane",
    "unreal engine",
    "award winning",
];

fn strip_quality_terms(s: &str) -> String {
    split_segments(s)
        .into_iter()
        .filter(|seg| !QUALITY_SPAM.contains(&seg.to_lowercase().as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn split_segments(s: &str) -> Vec<String> {
    s.split([',', '，', ';', '；', '、', '\n'])
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty())
        .collect()
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn trim_separators(s: &str) -> String {
    s.trim()
        .trim_matches(|c: char| matches!(c, ',' | '，' | '.' | '。' | ';' | '；' | '、' | ' '))
        .to_string()
}

fn contains_cjk(s: &str) -> bool {
    s.chars().any(is_cjk)
}

/// 按字符数截断,尽量落在短语边界(逗号/句号),避免把单词或句子砍半。
fn truncate_fragment(s: &str, max_chars: usize) -> (String, bool) {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        return (s.to_string(), false);
    }
    let boundary = [',', '，', ';', '；', '。', '.', '、'];
    let mut cut = max_chars;
    let floor = max_chars.saturating_sub(max_chars / 4);
    if let Some(i) = (floor..max_chars)
        .rev()
        .find(|&i| chars.get(i).is_some_and(|c| boundary.contains(c)))
    {
        cut = i;
    }
    if cut == 0 {
        cut = max_chars;
    }
    let out: String = chars[..cut].iter().collect();
    let out = out
        .trim()
        .trim_matches(|c: char| boundary.contains(&c) || c == ' ')
        .to_string();
    (out, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_families() {
        assert_eq!(detect_family("zhipu", "cogview-3-plus"), PromptFamily::CogView);
        assert_eq!(detect_family("custom", "flux-dev"), PromptFamily::Flux);
        assert_eq!(detect_family("openai", "dall-e-3"), PromptFamily::OpenAi);
        assert_eq!(detect_family("custom", "sdxl-turbo"), PromptFamily::Sdxl);
        assert_eq!(detect_family("deepseek", "image-x"), PromptFamily::Generic);
    }

    #[test]
    fn dedupes_tags_case_insensitive() {
        let c = compose("cat, Cat, dog, cat ", None, None, "custom", "sdxl");
        assert_eq!(c.prompt, "cat, dog");
        assert_eq!(c.family, "sdxl");
    }

    #[test]
    fn appends_style_after_base() {
        let c = compose(
            "a cat on a windowsill",
            Some("watercolor painting"),
            None,
            "openai",
            "dall-e-3",
        );
        assert_eq!(c.prompt, "a cat on a windowsill, watercolor painting");
    }

    #[test]
    fn converts_negative_to_avoid_text_and_warns() {
        let c = compose("portrait", None, Some("blurry, watermark"), "openai", "dall-e-3");
        assert!(c.prompt.contains("Avoid: blurry, watermark"));
        assert!(c.warnings.iter().any(|w| w.contains("negative")));
    }

    #[test]
    fn strips_quality_spam_for_narrative_models() {
        let c = compose(
            "a cat, 8k, masterpiece, best quality, warm light",
            None,
            None,
            "openai",
            "dall-e-3",
        );
        assert!(!c.prompt.contains("8k"));
        assert!(!c.prompt.contains("masterpiece"));
        assert!(c.prompt.contains("warm light"));
    }

    #[test]
    fn truncates_at_phrase_boundary_and_respects_cap() {
        // 每段唯一,避免被标签去重合并掉,才能真实触发长度截断
        let long = (0..100)
            .map(|i| format!("a red fox {i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let c = compose(&long, None, None, "custom", "sdxl");
        assert!(c.truncated);
        assert!(c.chars <= c.max_chars);
        assert!(!c.prompt.ends_with(','));
    }

    #[test]
    fn warns_on_chinese_for_non_cogview() {
        let c = compose("黄昏时分的湖边小屋,水彩风格", None, None, "openai", "dall-e-3");
        assert!(c.warnings.iter().any(|w| w.contains("英文")));
    }

    #[test]
    fn token_estimate_counts_cjk_and_ascii() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("猫"), 1);
        assert_eq!(estimate_tokens("一只猫"), 3);
    }

    #[test]
    fn empty_prompt_yields_empty_composition() {
        let c = compose("   ", None, None, "openai", "dall-e-3");
        assert!(c.prompt.is_empty());
        assert_eq!(c.est_tokens, 0);
    }
}
