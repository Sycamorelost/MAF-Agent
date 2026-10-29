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
pub mod profile;
pub mod style;
pub mod writer;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use profile::Profile;

/// 文档元信息(封面用)。字段留空则不生成封面。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DocMeta {
    pub author: String,
    pub org: String,
    pub course: String,
    pub date: String,
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
            ("作者", self.author.as_str()),
            ("单位", self.org.as_str()),
            ("日期", self.date.as_str()),
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
    /// 为空时按标题/正文自动识别用途
    pub profile: Option<Profile>,
    /// 为空时按用途默认(学术类开启,通用类关闭)
    pub toc: Option<bool>,
    pub meta: DocMeta,
    /// 相对图片路径的基准目录
    pub base_dir: PathBuf,
}

impl Default for DocRequest {
    fn default() -> Self {
        DocRequest {
            title: String::new(),
            subtitle: String::new(),
            body: String::new(),
            profile: None,
            toc: None,
            meta: DocMeta::default(),
            base_dir: PathBuf::new(),
        }
    }
}

impl DocRequest {
    pub fn effective_profile(&self) -> Profile {
        self.profile
            .unwrap_or_else(|| Profile::detect(&self.title, &self.body))
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
            profile: profile.and_then(Profile::parse),
            toc,
            meta,
            base_dir: PathBuf::new(),
        }
    }
}

/// 渲染 docx 字节(纯函数,不写磁盘)。
pub fn render(req: &DocRequest) -> Result<Vec<u8>, String> {
    let profile = req.effective_profile();
    let style = style::DocStyle::for_profile(profile);
    writer::render(req, &style, &req.base_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn request(body: &str) -> DocRequest {
        DocRequest {
            title: "实验三 排课 CSP 求解".into(),
            body: body.into(),
            ..DocRequest::default()
        }
    }

    fn part_xml(bytes: &[u8], name: &str) -> String {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        let mut xml = String::new();
        zip.by_name(name).unwrap().read_to_string(&mut xml).unwrap();
        xml
    }

    fn document_xml(bytes: &[u8]) -> String {
        part_xml(bytes, "word/document.xml")
    }

    fn styles_xml(bytes: &[u8]) -> String {
        part_xml(bytes, "word/styles.xml")
    }

    #[test]
    fn renders_zip_with_word_styles() {
        let req = request(
            "## 一、实验目的\n\n理解 CSP 约束传播。\n\n表 1 各轮剪枝结果\n| 轮次 | 节点数 |\n|:---|---:|\n| 1 | 120 |",
        );
        let bytes = render(&req).unwrap();
        assert!(bytes.starts_with(b"PK"), "应产出合法 docx(zip)");
        let doc = document_xml(&bytes);
        // 段落引用真实 Heading 样式,样式定义带大纲级别 → 导航/目录可用
        assert!(doc.contains(r#"w:pStyle w:val="Heading2""#));
        assert!(styles_xml(&bytes).contains(r#"w:styleId="Heading1""#));
        assert!(styles_xml(&bytes).contains("<w:outlineLvl"));
        // 三线表:上下粗线
        assert!(doc.contains(r#"w:sz="12""#));
        // 表格列宽使用固定网格
        assert!(doc.contains("<w:tblGrid>"));
    }

    #[test]
    fn general_profile_uses_grid_table() {
        let req = DocRequest {
            title: "项目周报".into(),
            body: "| 事项 | 状态 |\n|---|---|\n| 联调 | 完成 |".into(),
            ..DocRequest::default()
        };
        assert_eq!(req.effective_profile(), Profile::General);
        let xml = document_xml(&render(&req).unwrap());
        assert!(xml.contains(r#"w:color="BFBFBF""#));
    }

    #[test]
    fn toc_only_when_headings_exist() {
        let mut req = request("## 一、实验目的\n\n正文");
        assert!(req.want_toc());
        assert!(document_xml(&render(&req).unwrap()).contains("TOC"));

        req.body = "正文无标题".into();
        assert!(!document_xml(&render(&req).unwrap()).contains("TOC"));
    }

    #[test]
    fn cover_only_when_meta_present() {
        let mut req = request("## 一、实验目的\n\n正文");
        assert!(!req.has_cover());
        req.meta.author = "张三".into();
        req.meta.course = "人工智能".into();
        assert!(req.has_cover());
        let xml = document_xml(&render(&req).unwrap());
        assert!(xml.contains("张三"));
        assert!(xml.contains("人工智能"));
    }

    #[test]
    fn meta_rows_skip_empty_fields() {
        let meta = DocMeta {
            author: " 李四 ".into(),
            ..DocMeta::default()
        };
        assert_eq!(meta.rows(), vec![("作者", "李四")]);
        assert!(!meta.is_empty());
    }
}
