//! 通用样式参数包:版本化 JSON + reference DOCX 两条来源。
//!
//! 只做「已知字段覆盖」,不复制 OOXML 结构;所有来源都会先校验版本、
//! 尺寸范围与安全边界。内置参数包提供不依赖任何单位素材的通用样例。

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::super::style::DocStyle;
use super::super::style::HeadingNumbering;
use super::super::style::brand::BrandTokens;
use super::reference_docx;
use super::super::templates;

/// 参数包格式版本。
pub const SCHEMA_VERSION: u32 = 1;
/// JSON 参数包体积上限。
const MAX_PACK_BYTES: u64 = 256 * 1024;

/// 一次参数包应用结果。
#[derive(Debug, Clone)]
pub struct AppliedStyle {
    pub style: DocStyle,
    pub name: String,
    /// 来源文件 SHA-256(reference DOCX 或 JSON 文件)
    pub sha256: Option<String>,
    /// 降级/忽略项说明
    pub warnings: Vec<String>,
}

/// 版本化样式参数包。字段均为可选覆盖项,单位与 [`DocStyle`] 一致。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StylePack {
    pub schema_version: u32,
    pub name: String,
    /// 基础模板 id(为空时沿用调用方模板)
    pub base: String,
    pub latin: Option<String>,
    pub east_asia: Option<String>,
    pub heading_east_asia: Option<String>,
    pub body_size: Option<usize>,
    pub body_line: Option<i32>,
    pub fixed_line: Option<i32>,
    pub body_indent: Option<i32>,
    pub heading_sizes: Option<[usize; 4]>,
    pub table_size: Option<usize>,
    pub caption_size: Option<usize>,
    pub meta_size: Option<usize>,
    pub title_size: Option<usize>,
    /// [上, 下, 左, 右] twip
    pub margins: Option<[i32; 4]>,
    /// [宽, 高] twip
    pub page: Option<[u32; 2]>,
    /// none / academic / official / simple
    pub numbering: Option<String>,
    pub three_line: Option<bool>,
    pub header_fill: Option<bool>,
    pub justify: Option<bool>,
    pub brand: Option<BrandTokens>,
}

impl Default for StylePack {
    fn default() -> Self {
        StylePack {
            schema_version: SCHEMA_VERSION,
            name: String::new(),
            base: String::new(),
            latin: None,
            east_asia: None,
            heading_east_asia: None,
            body_size: None,
            body_line: None,
            fixed_line: None,
            body_indent: None,
            heading_sizes: None,
            table_size: None,
            caption_size: None,
            meta_size: None,
            title_size: None,
            margins: None,
            page: None,
            numbering: None,
            three_line: None,
            header_fill: None,
            justify: None,
            brand: None,
        }
    }
}

impl StylePack {
    /// 解析 JSON 文本并校验。
    pub fn parse(json: &str) -> Result<Self, String> {
        let pack: StylePack =
            serde_json::from_str(json).map_err(|e| format!("style_pack 格式错误:{e}"))?;
        pack.validate()?;
        Ok(pack)
    }

    /// 内置通用参数包。
    pub fn builtin(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "generic-thesis" | "generic_thesis" => Some(StylePack {
                name: "generic-thesis".into(),
                base: "thesis".into(),
                body_size: Some(24),
                body_line: Some(360),
                body_indent: Some(480),
                heading_sizes: Some([32, 28, 24, 24]),
                three_line: Some(true),
                justify: Some(true),
                ..StylePack::default()
            }),
            "generic-brand" | "generic_brand" => Some(StylePack {
                name: "generic-brand".into(),
                base: "general".into(),
                latin: Some("Arial".into()),
                east_asia: Some("微软雅黑".into()),
                body_size: Some(22),
                body_line: Some(276),
                body_indent: Some(0),
                three_line: Some(false),
                header_fill: Some(true),
                justify: Some(false),
                brand: Some(BrandTokens {
                    latin_font: "Arial".into(),
                    east_asia_font: "微软雅黑".into(),
                    ..BrandTokens::default()
                }),
                ..StylePack::default()
            }),
            _ => None,
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "style_pack.schema_version 必须为 {SCHEMA_VERSION},收到 {}",
                self.schema_version
            ));
        }
        if let Some(page) = self.page
            && (page.iter().any(|v| !(8_000..=40_000).contains(v)))
        {
            return Err("style_pack.page 超出支持的页面尺寸范围".into());
        }
        if let Some(margins) = self.margins
            && margins.iter().any(|v| *v < 0)
        {
            return Err("style_pack.margins 不能为负".into());
        }
        for size in [self.body_size, self.table_size, self.caption_size, self.meta_size, self.title_size]
            .into_iter()
            .flatten()
        {
            if !(8..=120).contains(&size) {
                return Err(format!("style_pack 字号 {size} 超出 8..=120 半点范围"));
            }
        }
        if let Some(brand) = &self.brand {
            brand.validate()?;
        }
        Ok(())
    }

    /// 应用到版式。
    pub fn apply(&self, style: &mut DocStyle) {
        if let Some(v) = &self.latin {
            style.latin = v.clone();
        }
        if let Some(v) = &self.east_asia {
            style.east_asia = v.clone();
        }
        if let Some(v) = &self.heading_east_asia {
            style.heading_east_asia = v.clone();
        }
        if let Some(v) = self.body_size {
            style.body_size = v;
        }
        if let Some(v) = self.body_line {
            style.body_line = v;
        }
        if let Some(v) = self.fixed_line {
            style.fixed_line = (v > 0).then_some(v);
        }
        if let Some(v) = self.body_indent {
            style.body_indent = v;
        }
        if let Some(v) = self.heading_sizes {
            style.heading_sizes = v;
        }
        if let Some(v) = self.table_size {
            style.table_size = v;
        }
        if let Some(v) = self.caption_size {
            style.caption_size = v;
        }
        if let Some(v) = self.meta_size {
            style.meta_size = v;
        }
        if let Some(v) = self.title_size {
            style.title_size = v;
        }
        if let Some([t, b, l, r]) = self.margins {
            style.margin_top = t;
            style.margin_bottom = b;
            style.margin_left = l;
            style.margin_right = r;
        }
        if let Some([w, h]) = self.page {
            style.page_w = w;
            style.page_h = h;
        }
        if let Some(n) = &self.numbering {
            style.numbering = parse_numbering(n);
        }
        if let Some(v) = self.three_line {
            style.three_line = v;
        }
        if let Some(v) = self.header_fill {
            style.header_fill = v;
        }
        if let Some(v) = self.justify {
            style.justify = v;
        }
        if let Some(brand) = &self.brand {
            brand.apply_style(style);
        }
    }
}

fn parse_numbering(raw: &str) -> HeadingNumbering {
    match raw.trim().to_ascii_lowercase().as_str() {
        "academic" => HeadingNumbering::Academic,
        "official" => HeadingNumbering::Official,
        "simple" => HeadingNumbering::Simple,
        _ => HeadingNumbering::None,
    }
}

/// 按来源加载参数包:内置名 / reference DOCX / JSON 文件。
pub fn load_and_apply(source: &str, template_id: &str) -> Result<AppliedStyle, String> {
    let source = source.trim();
    if source.is_empty() {
        return Err("style_pack 不能为空".into());
    }
    let mut style = templates::style_of(template_id);
    let mut warnings = Vec::new();
    let mut sha256 = None;

    let (name, pack) = if let Some(pack) = StylePack::builtin(source) {
        (pack.name.clone(), Some(pack))
    } else if source.to_ascii_lowercase().ends_with(".docx") {
        let imported = reference_docx::import(Path::new(source))?;
        imported.pack.apply(&mut style);
        warnings.extend(imported.warnings);
        sha256 = Some(imported.sha256);
        (imported.name, None)
    } else {
        let json = read_json(Path::new(source))?;
        let pack = StylePack::parse(&json)?;
        (pack.name.clone(), Some(pack))
    };

    if let Some(pack) = pack {
        // base 显式声明时以其模板版式为底,再叠加本包覆盖
        if !pack.base.trim().is_empty()
            && let Some(t) = templates::lookup(&pack.base)
        {
            style = templates::style_of(t.id);
        }
        pack.apply(&mut style);
    }
    if name.trim().is_empty() {
        warnings.push("参数包未提供 name,已使用模板名".into());
    }
    Ok(AppliedStyle {
        style,
        name: if name.trim().is_empty() {
            template_id.to_string()
        } else {
            name
        },
        sha256,
        warnings,
    })
}

fn read_json(path: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("读取 style_pack 失败:{e}"))?;
    if meta.len() > MAX_PACK_BYTES {
        return Err(format!(
            "style_pack 文件超过 {} KB 上限",
            MAX_PACK_BYTES / 1024
        ));
    }
    std::fs::read_to_string(path).map_err(|e| format!("读取 style_pack 失败:{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_version_and_fields() {
        assert!(StylePack::parse(r#"{"schema_version":99}"#).is_err());
        assert!(StylePack::parse(r#"{"schema_version":1,"nope":1}"#).is_err());
    }

    #[test]
    fn builtin_packs_apply_expected_metrics() {
        let a = load_and_apply("generic-thesis", "general").unwrap();
        assert_eq!(a.style.body_size, 24);
        assert!(a.style.three_line);
        let b = load_and_apply("generic-brand", "paper").unwrap();
        assert_eq!(b.style.latin, "Arial");
        assert_eq!(b.style.body_indent, 0);
    }

    #[test]
    fn partial_pack_keeps_base_values() {
        let pack = StylePack::parse(r#"{"schema_version":1,"body_size":30}"#).unwrap();
        let mut style = templates::style_of("paper");
        let before = style.body_line;
        pack.apply(&mut style);
        assert_eq!(style.body_size, 30);
        assert_eq!(style.body_line, before);
    }

    #[test]
    fn page_and_margin_ranges_are_checked() {
        assert!(StylePack::parse(r#"{"schema_version":1,"page":[100,16838]}"#).is_err());
        assert!(StylePack::parse(r#"{"schema_version":1,"margins":[-1,1,1,1]}"#).is_err());
    }
}
