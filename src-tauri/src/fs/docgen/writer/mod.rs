//! docx 渲染层:把块结构 + 版式翻译成 docx-rs 文档。
//!
//! 这是唯一依赖 docx-rs 的模块;解析、版式数据都保持纯函数,便于替换渲染后端
//! (例如后续换 OOXML 模板或 PDF)。
//!
//! 版式落地为「真实 Word 样式」而非逐段直接格式化:
//! - Heading1~4 带 outlineLvl → 导航窗格、自动目录、样式替换可用;
//! - BodyText 首行缩进 2 字符、1.5 倍行距、两端对齐;
//! - Caption / TableText / SourceCode / QuoteText 各自独立;
//! - 表格用固定列宽 + 三线表(学术模板)或浅灰网格表(通用模板)。
//!
//! 职责拆分:
//! - `document.rs`:文档装配、封面、页眉页脚、块渲染与行内文本 run;
//! - `table.rs`:表格结构、表头/单元格样式与列宽分配;
//! - `image.rs`:图片定位、缩放与段落封装;
//! - `styles.rs`:Word 样式表。

mod document;
mod image;
mod styles;
mod table;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use document::render;
