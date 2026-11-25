//! 幻灯片设计 token:字号层级、行距、版心与 9 套主题色板。
//!
//! 模板与渲染层只引用这里的常量,避免字号/间距魔法数字散落各处。
//! 色板同时提供 WCAG 对比度检查(正文与背景 AA ≥ 4.5:1)。

/// 画布尺寸(英寸,16:9)
pub(super) const SLIDE_W_IN: f64 = 13.333;
pub(super) const SLIDE_H_IN: f64 = 7.5;

/// 版心:左右留白、内容起始/结束高度与宽度
pub(super) const MARGIN_X_IN: f64 = 0.6;
pub(super) const CONTENT_TOP_IN: f64 = 1.55;
pub(super) const CONTENT_BOTTOM_IN: f64 = 6.3;
pub(super) const CONTENT_W_IN: f64 = 12.1;

/// 页脚装饰
pub(super) const ACCENT_BAR_H_IN: f64 = 0.075;
pub(super) const FOOTER_RULE_Y_IN: f64 = 6.55;
pub(super) const FOOTER_TEXT_Y_IN: f64 = 6.63;
pub(super) const FOOTER_RULE_W_IN: f64 = 12.1;
pub(super) const FOOTER_TEXT_W_IN: f64 = 9.0;
pub(super) const FOOTER_PAGE_X_IN: f64 = 11.95;
pub(super) const FOOTER_PAGE_W_IN: f64 = 0.75;
pub(super) const FOOTER_ROW_H_IN: f64 = 0.26;
pub(super) const FOOTNOTE_Y_IN: f64 = 6.12;

/// 指标卡
pub(super) const KPI_TOP_IN: f64 = 2.15;
pub(super) const KPI_CARD_H_IN: f64 = 2.3;
pub(super) const KPI_GAP_IN: f64 = 0.25;
pub(super) const KPI_PAD_IN: f64 = 0.2;

/// 表格与图表位置(EMU / 英寸)
pub(super) const TABLE_X_EMU: u32 = 700_000;
pub(super) const TABLE_Y_EMU: u32 = 1_900_000;
pub(super) const TABLE_W_EMU: u32 = 10_600_000;
pub(super) const CHART_X_IN: f64 = 0.7;
pub(super) const CHART_Y_IN: f64 = 1.55;
pub(super) const CHART_W_IN: f64 = 11.9;
pub(super) const CHART_H_IN: f64 = 4.7;
pub(super) const DIAGRAM_H_IN: f64 = 4.4;

/// 字号层级(pt)
pub(super) const COVER_TITLE_PT: u32 = 40;
pub(super) const COVER_SUB_PT: u32 = 18;
pub(super) const SECTION_TITLE_PT: u32 = 36;
pub(super) const CLOSING_TITLE_PT: u32 = 36;
pub(super) const TITLE_PT: u32 = 28;
pub(super) const BODY_PT: u32 = 18;
pub(super) const BODY_2COL_PT: u32 = 16;
pub(super) const BODY_IMAGE_PT: u32 = 17;
pub(super) const BODY_TABLE_PT: u32 = 14;
/// 正文可缩到的下限;到下限仍溢出则分页并给诊断
pub(super) const BODY_MIN_PT: u32 = 14;
/// 缩字档位:从大往小尝试
pub(super) const BODY_SIZE_STEPS: [u32; 5] = [18, 17, 16, 15, 14];

/// 行距倍数
pub(super) const LINE_SPACING: f64 = 1.25;

/// 1 英寸 = 72 pt
pub(super) const PT_PER_IN: f64 = 72.0;

/// 单套主题色板(hex,不含 #)。
pub(super) struct Palette {
    pub name: &'static str,
    pub background: &'static str,
    pub text: &'static str,
    pub title: &'static str,
    pub accent: &'static str,
    pub muted: &'static str,
}

/// 9 套主题:7 套既有(取值保持兼容)+ ocean / sunset。
pub(super) const THEMES: [Palette; 9] = [
    Palette {
        name: "corporate",
        background: "FFFFFF",
        text: "212121",
        title: "0D2B45",
        accent: "2F5597",
        muted: "6B7280",
    },
    Palette {
        name: "modern",
        background: "FAFAFA",
        text: "212121",
        title: "111827",
        accent: "2563EB",
        muted: "6B7280",
    },
    Palette {
        name: "tech",
        background: "FFFFFF",
        text: "1F2937",
        title: "0F172A",
        accent: "0EA5E9",
        muted: "64748B",
    },
    Palette {
        name: "vibrant",
        background: "FFFFFF",
        text: "212121",
        title: "4A148C",
        accent: "EA580C",
        muted: "7C3AED",
    },
    Palette {
        name: "nature",
        background: "FFFFFF",
        text: "1B2E1B",
        title: "14532D",
        accent: "15803D",
        muted: "4B5563",
    },
    Palette {
        name: "dark",
        background: "111827",
        text: "E5E7EB",
        title: "F9FAFB",
        accent: "F59E0B",
        muted: "9CA3AF",
    },
    Palette {
        name: "academic",
        background: "FFFFFF",
        text: "111827",
        title: "1F2937",
        accent: "374151",
        muted: "6B7280",
    },
    Palette {
        name: "ocean",
        background: "F0F9FF",
        text: "0C4A6E",
        title: "082F49",
        accent: "0369A1",
        muted: "475569",
    },
    Palette {
        name: "sunset",
        background: "FFF7ED",
        text: "431407",
        title: "7C2D12",
        accent: "C2410C",
        muted: "6B7280",
    },
];

/// 取主题色板;未知名回退 corporate。
pub(super) fn palette(name: &str) -> &'static Palette {
    let key = name.trim().to_ascii_lowercase();
    THEMES.iter().find(|p| p.name == key).unwrap_or(&THEMES[0])
}

/// 生效中的颜色集合(内置或用户主题包),统一以拥有所有权的字符串返回。
#[derive(Debug, Clone)]
pub(super) struct Colors {
    pub name: String,
    pub background: String,
    pub text: String,
    pub title: String,
    pub accent: String,
    pub muted: String,
}

/// 版本化主题包:只允许覆盖已有色板字段,不接受任意 OOXML。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ThemePack {
    pub schema_version: u32,
    pub name: String,
    pub background: String,
    pub text: String,
    pub title: String,
    pub accent: String,
    pub muted: String,
}

static CUSTOM_THEMES: std::sync::LazyLock<std::sync::RwLock<Vec<ThemePack>>> =
    std::sync::LazyLock::new(|| std::sync::RwLock::new(Vec::new()));

/// 解析并登记主题包;颜色自动去 `#` 并在后台执行 AA 对比度检查。
pub(super) fn install_pack(raw: &str) -> Result<String, String> {
    let mut pack: ThemePack =
        serde_json::from_str(raw.trim()).map_err(|e| format!("theme_pack 格式错误:{e}"))?;
    if pack.schema_version != 1 {
        return Err(format!(
            "theme_pack.schema_version 必须为 1,收到 {}",
            pack.schema_version
        ));
    }
    let key = pack.name.trim().to_ascii_lowercase();
    if key.is_empty() {
        return Err("theme_pack.name 不能为空".into());
    }
    if THEMES.iter().any(|p| p.name == key) {
        return Err(format!("theme_pack 不能覆盖内置主题 `{key}`"));
    }
    for color in [
        &mut pack.background,
        &mut pack.text,
        &mut pack.title,
        &mut pack.accent,
        &mut pack.muted,
    ] {
        *color = color.trim().trim_start_matches('#').to_ascii_uppercase();
        if color.len() != 6 || !color.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("theme_pack 颜色 `{color}` 不是 6 位十六进制"));
        }
    }
    if !aa_ok(&pack.text, &pack.background) || !aa_ok(&pack.title, &pack.background) {
        return Err("theme_pack 正文/标题与背景对比度不足 4.5:1".into());
    }
    pack.name = key.clone();
    let mut registry = CUSTOM_THEMES.write().map_err(|_| "主题包登记失败")?;
    registry.retain(|p| p.name != key);
    registry.push(pack);
    Ok(key)
}

/// 查找已登记的自定义主题包。
pub(super) fn custom_pack(name: &str) -> Option<ThemePack> {
    let key = name.trim().to_ascii_lowercase();
    CUSTOM_THEMES.read().ok()?.iter().find(|p| p.name == key).cloned()
}

/// 取生效颜色:自定义主题优先,否则用内置色板。
pub(super) fn colors(name: &str) -> Colors {
    if let Some(p) = custom_pack(name) {
        return Colors {
            name: p.name,
            background: p.background,
            text: p.text,
            title: p.title,
            accent: p.accent,
            muted: p.muted,
        };
    }
    let p = palette(name);
    Colors {
        name: p.name.to_string(),
        background: p.background.to_string(),
        text: p.text.to_string(),
        title: p.title.to_string(),
        accent: p.accent.to_string(),
        muted: p.muted.to_string(),
    }
}

/// 主题强调色(用于顶部色条、指标卡与图表)。
pub(super) fn accent(name: &str) -> String {
    colors(name).accent
}

fn hex_channel(hex: &str, i: usize) -> Option<f64> {
    let s = hex.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    Some(v as f64 / 255.0)
}

fn luminance(hex: &str) -> Option<f64> {
    let f = |c: f64| {
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let r = f(hex_channel(hex, 0)?);
    let g = f(hex_channel(hex, 1)?);
    let b = f(hex_channel(hex, 2)?);
    Some(0.2126 * r + 0.7152 * g + 0.0722 * b)
}

/// WCAG 对比度(1..21);非法 hex 返回 0。
pub(super) fn contrast_ratio(fg: &str, bg: &str) -> f64 {
    match (luminance(fg), luminance(bg)) {
        (Some(a), Some(b)) => {
            let (hi, lo) = if a > b { (a, b) } else { (b, a) };
            (hi + 0.05) / (lo + 0.05)
        }
        _ => 0.0,
    }
}

/// 正文/背景是否满足 WCAG AA(≥ 4.5:1)。
pub(super) fn aa_ok(fg: &str, bg: &str) -> bool {
    contrast_ratio(fg, bg) >= 4.5
}

/// 检查单套主题的正文与标题对比度,返回问题描述。
pub(super) fn check_theme(name: &str) -> Vec<String> {
    let p = colors(name);
    let mut out = Vec::new();
    if !aa_ok(&p.text, &p.background) {
        out.push(format!(
            "{} 主题正文对比度不足:{:.2}:1(需 ≥ 4.5)",
            p.name,
            contrast_ratio(&p.text, &p.background)
        ));
    }
    if !aa_ok(&p.title, &p.background) {
        out.push(format!(
            "{} 主题标题对比度不足:{:.2}:1(需 ≥ 4.5)",
            p.name,
            contrast_ratio(&p.title, &p.background)
        ));
    }
    if !aa_ok(&p.muted, &p.background) {
        out.push(format!(
            "{} 主题脚注对比度不足:{:.2}:1(需 ≥ 4.5)",
            p.name,
            contrast_ratio(&p.muted, &p.background)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_themes_pass_wcag_aa() {
        assert_eq!(THEMES.len(), 9);
        for p in THEMES.iter() {
            assert!(
                check_theme(p.name).is_empty(),
                "主题问题:{:?}",
                check_theme(p.name)
            );
        }
    }

    #[test]
    fn contrast_ratios_are_sane() {
        assert!(contrast_ratio("000000", "FFFFFF") > 20.0);
        assert!(!aa_ok("FFFFFF", "FFFFFF"));
        assert_eq!(contrast_ratio("bad", "FFFFFF"), 0.0);
        assert_eq!(accent("unknown"), "2F5597");
    }
}
