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
//! - `document.rs`:装配调度(封面/目录/分节)与打包;
//! - `body.rs`:块 → 段落、行内 run、书签挂载;
//! - `field.rs`:REF/SEQ/TOC 域;
//! - `numbering.rs`:原生多级编号(`w:numPr`);
//! - `section.rs`:封面/前置/正文/附录分节参数;
//! - `table.rs` / `image.rs` / `styles.rs`:表格、图片、样式表。

mod body;
mod document;
mod field;
mod front_matter;
mod image;
mod metrics;
mod numbering;
mod section;
mod styles;
mod table;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use document::render;
