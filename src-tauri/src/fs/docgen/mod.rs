//! 文档生成(docgen):Markdown → 高质量 .docx。
//!
//! 分层(依赖单向,便于替换与单测):
//! - `profile`:文档用途与写作规范(实验报告/论文/实践报告/通用);
//! - `style`:版式数据(页面/字体/字号/行距/缩进/表格样式),不依赖 docx-rs;
//! - `markdown`:块级解析(标题/列表/表格/代码/题注/图片),只产出中立结构;
//! - `inline`:行内标记 + 中文排版规范化 + LaTeX 兜底;
//! - `writer`:唯一依赖 docx-rs 的渲染层。
//!
//! 调研来源(GitHub 高星项目)及落地方式见 `profile` 模块头注释。

pub mod inline;
pub mod markdown;
pub mod math;
pub mod math_docx;
pub mod notes_index;
pub mod profile;
pub mod refs;
pub mod style;
pub mod svg_docx;
pub mod templates;
#[cfg(test)]
pub mod validation;
pub mod writer;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use profile::Profile;

/// 公式落地方式。默认 OMML(可编辑、可检索);`Svg` 为显式请求的静态矢量图。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormulaMode {
    /// Office Math,Word 内可编辑
    #[default]
    Omml,
    /// 纯静态 SVG + PNG 兼容副本,不可按公式编辑或搜索
    Svg,
}

impl FormulaMode {
    /// 解析工具参数;未知取值明确失败,不静默回退。
    pub fn parse(raw: Option<&str>) -> Result<Self, String> {
        match raw.map(str::trim).filter(|s| !s.is_empty()) {
            None | Some("omml") => Ok(Self::Omml),
            Some("svg") => Ok(Self::Svg),
            Some(other) => Err(format!(
                "formula_mode 仅支持 omml / svg,收到 `{other}`"
            )),
        }
    }
}

/// 文档元信息(封面用)。字段留空则不生成封面。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DocMeta {
    pub author: String,
    pub org: String,
    pub course: String,
    pub date: String,
    /// 文档版本(显示在封面并进入页眉)
    pub version: String,
    /// 产品名(白皮书/手册)
    pub product: String,
    /// 密级/公开范围
    pub classification: String,
    /// 成文地点
    pub location: String,
    /// 媒体或支持联系
    pub contact: String,
}

impl DocMeta {
    pub fn is_empty(&self) -> bool {
        self.rows().is_empty()
    }

    /// 封面字段(有值才显示)。
    pub fn rows(&self) -> Vec<(&'static str, &str)> {
        let mut out = Vec::new();
        for (label, value) in [
            ("课程", self.course.as_str()),
            ("产品", self.product.as_str()),
            ("版本", self.version.as_str()),
            ("作者", self.author.as_str()),
            ("单位", self.org.as_str()),
            ("地点", self.location.as_str()),
            ("日期", self.date.as_str()),
            ("密级", self.classification.as_str()),
            ("联系", self.contact.as_str()),
        ] {
            let value = value.trim();
            if !value.is_empty() {
                out.push((label, value));
            }
        }
        out
    }
}

/// 一次文档生成请求。与文件 IO 解耦,渲染只产出字节。
#[derive(Debug, Clone)]
pub struct DocRequest {
    pub title: String,
    pub subtitle: String,
    pub body: String,
    /// 文体模板 id(paper/lab/official/notice/minutes/summary/practice/general)
    pub template: Option<String>,
    /// 为空时按标题/正文自动识别用途
    pub profile: Option<Profile>,
    /// 为空时按用途默认(学术类开启,通用类关闭)
    pub toc: Option<bool>,
    pub meta: DocMeta,
    /// 公式落地方式(默认 OMML)
    pub formula_mode: FormulaMode,
    /// 样式参数包:内置名 / JSON 路径 / reference DOCX 路径
    pub style_pack: Option<String>,
    /// 品牌令牌:内联 JSON 或本地 JSON 路径
    pub brand: Option<String>,
    /// 相对图片路径的基准目录
    pub base_dir: PathBuf,
}

impl Default for DocRequest {
    fn default() -> Self {
        DocRequest {
            title: String::new(),
            subtitle: String::new(),
            body: String::new(),
            template: None,
            profile: None,
            toc: None,
            meta: DocMeta::default(),
            formula_mode: FormulaMode::Omml,
            style_pack: None,
            brand: None,
            base_dir: PathBuf::new(),
        }
    }
}

impl DocRequest {
    /// 页眉文字:标题 + 版本(版本为空时只显示标题)。
    pub fn header_line(&self) -> String {
        let version = self.meta.version.trim();
        if version.is_empty() {
            self.title.clone()
        } else {
            format!("{}  v{}", self.title.trim(), version)
        }
    }

    pub fn effective_profile(&self) -> Profile {
        self.profile
            .unwrap_or_else(|| Profile::detect(&self.title, &self.body))
    }

    /// 生效模板:显式 `template` > 显式 `profile` > 标题/正文自动识别。
    pub fn effective_template(&self) -> &'static templates::Template {
        if let Some(t) = self.template.as_deref().and_then(templates::lookup) {
            return t;
        }
        if let Some(p) = self.profile {
            let id = templates::id_for_profile(p);
            return templates::lookup(id).expect("内置模板必须存在");
        }
        templates::detect(&self.title, &self.body)
    }

    pub fn want_toc(&self) -> bool {
        self.toc
            .unwrap_or_else(|| self.effective_profile().toc_default())
    }

    pub fn has_cover(&self) -> bool {
        !self.meta.is_empty()
    }

    /// 从工具参数构建请求(profile 支持中英文别名,无法识别时按标题/正文自动判断)。
    pub fn from_parts(
        title: &str,
        body: &str,
        profile: Option<&str>,
        toc: Option<bool>,
        meta: DocMeta,
    ) -> DocRequest {
        DocRequest {
            title: title.to_string(),
            subtitle: String::new(),
            body: body.to_string(),
            template: None,
            profile: profile.and_then(Profile::parse),
            toc,
            meta,
            formula_mode: FormulaMode::Omml,
            style_pack: None,
            brand: None,
            base_dir: PathBuf::new(),
        }
    }
}

/// 渲染 docx 字节(纯函数,不写磁盘)。
pub fn render(req: &DocRequest) -> Result<Vec<u8>, String> {
    let template_id = req.effective_template().id;
    let mut style = templates::style_of(template_id);
    if let Some(source) = req.style_pack.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let applied = templates::package::load_and_apply(source, template_id)?;
        for w in &applied.warnings {
            log::warn!("docgen 样式参数包降级: {w}");
        }
        if let Some(hash) = &applied.sha256 {
            log::info!("docgen reference 样式来源 {} sha256={hash}", applied.name);
        } else {
            log::info!("docgen 样式参数包 `{}` 已应用", applied.name);
        }
        style = applied.style;
    }
    if let Some(source) = req.brand.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        style::brand::BrandTokens::load(source)?.apply_style(&mut style);
    }
    style.formula_mode = req.formula_mode;
    writer::render(req, &style, &req.base_dir)
}

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod refs_tests;

#[cfg(test)]
mod tests;
