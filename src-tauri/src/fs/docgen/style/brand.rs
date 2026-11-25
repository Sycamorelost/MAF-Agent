//! 品牌令牌:文档、图表、幻灯片与确定性图片共用的颜色和字体字典。
//!
//! 只承载可公开复用的样式值,不包含任何单位徽标、字体文件或许可受限资产。

use serde::{Deserialize, Serialize};

/// 一组品牌令牌(颜色统一 `#RRGGBB` 大写十六进制)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BrandTokens {
    /// 主色
    pub primary: String,
    /// 强调色
    pub accent: String,
    /// 正文色
    pub text: String,
    /// 次要/脚注色
    pub muted: String,
    /// 西文字体
    pub latin_font: String,
    /// 中文字体
    pub east_asia_font: String,
}

impl Default for BrandTokens {
    fn default() -> Self {
        BrandTokens {
            primary: "#1F4E79".into(),
            accent: "#C00000".into(),
            text: "#111111".into(),
            muted: "#595959".into(),
            latin_font: "Times New Roman".into(),
            east_asia_font: "宋体".into(),
        }
    }
}

impl BrandTokens {
    /// 校验并解析 JSON;未知字段、非法颜色和空字体都明确失败。
    pub fn parse(json: &str) -> Result<Self, String> {
        let tokens: BrandTokens =
            serde_json::from_str(json).map_err(|e| format!("brand 参数格式错误:{e}"))?;
        tokens.validate()?;
        Ok(tokens)
    }

    /// 校验颜色与字体字段。
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("primary", &self.primary),
            ("accent", &self.accent),
            ("text", &self.text),
            ("muted", &self.muted),
        ] {
            if !is_hex_color(value) {
                return Err(format!("brand.{name} 必须是 #RRGGBB 颜色,收到 `{value}`"));
            }
        }
        for (name, value) in [
            ("latin_font", &self.latin_font),
            ("east_asia_font", &self.east_asia_font),
        ] {
            if value.trim().is_empty() {
                return Err(format!("brand.{name} 不能为空"));
            }
        }
        Ok(())
    }

    /// 按来源加载:内联 JSON(`{` 开头)或本地 JSON 文件路径。
    pub fn load(source: &str) -> Result<Self, String> {
        let source = source.trim();
        if source.starts_with('{') {
            return Self::parse(source);
        }
        let meta = std::fs::metadata(source).map_err(|e| format!("读取 brand 文件失败:{e}"))?;
        if meta.len() > 64 * 1024 {
            return Err("brand 文件超过 64KB 上限".into());
        }
        let json = std::fs::read_to_string(source).map_err(|e| format!("读取 brand 文件失败:{e}"))?;
        Self::parse(&json)
    }

    /// 写入文档版式(字体与正文色)。
    pub fn apply_style(&self, style: &mut super::DocStyle) {
        style.latin = self.latin_font.clone();
        style.east_asia = self.east_asia_font.clone();
        style.heading_east_asia = self.east_asia_font.clone();
    }
}

/// 颜色写法:`#RRGGBB`。
pub fn is_hex_color(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 7
        && bytes[0] == b'#'
        && bytes[1..].iter().all(u8::is_ascii_hexdigit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tokens_validate() {
        assert!(BrandTokens::default().validate().is_ok());
    }

    #[test]
    fn rejects_bad_color_and_unknown_field() {
        assert!(BrandTokens::parse(r#"{"primary":"blue"}"#).is_err());
        assert!(BrandTokens::parse(r#"{"unknown":1}"#).is_err());
    }

    #[test]
    fn parses_and_applies_fonts() {
        let t = BrandTokens::parse(r#"{"latin_font":"Arial","east_asia_font":"思源宋体"}"#).unwrap();
        let mut style = super::super::DocStyle::for_profile(super::super::Profile::General);
        t.apply_style(&mut style);
        assert_eq!(style.latin, "Arial");
        assert_eq!(style.east_asia, "思源宋体");
    }
}
