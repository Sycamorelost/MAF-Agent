#![allow(dead_code)]
#![allow(unused_imports)]

//! 文件读写与办公格式导出(绝对路径可直达本机任意位置,相对路径以工作区为基准)。
//!
//! 所有写操作写前自动备份(可回滚),路径经 `fs::paths` 解析(位置与文件名均不设限)。
//! 导出支持 docx / xlsx,供 Agent 产出可直接交付的文件。
//!
//! 按职责拆分:`io`(读写/分页/删除)、`grep`(检索)、`edit`(精确替换)、
//! `export`(docx/xlsx)、`chunk`(大文件分批写入)、`common`(共享类型与工具)。

mod chunk;
mod common;
mod edit;
mod export;
mod grep;
mod io;

pub use chunk::{
    WriteChunkState, write_chunk_append, write_chunk_begin, write_chunk_finish, write_chunk_known,
};
pub use common::{FileEntry, MAX_READ_CHARS, SheetInput, WriteResult};
pub use edit::EditResult;
pub use edit::edit_text;
pub use export::{export_docx_ext, export_xlsx};
pub use grep::grep;
pub use io::{delete, list, read_text, read_text_page, root, root_for_session, write_text};

#[cfg(test)]
pub(crate) use chunk::{
    CHUNK_SESSION_TTL, MAX_CHUNK_SESSIONS_PER_OWNER, backdate_chunk, write_chunk_begin_in,
};
#[cfg(test)]
pub(crate) use edit::edit_text_in;
#[cfg(test)]
pub(crate) use export::{export_docx_in, export_xlsx_in};
#[cfg(test)]
pub(crate) use grep::grep_in;
#[cfg(test)]
pub(crate) use io::{delete_in, read_text_in, read_text_page_in, write_text_in};

#[cfg(test)]
mod tests;
