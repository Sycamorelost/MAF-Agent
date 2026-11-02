//! 附件相关 Tauri commands(迭代 D1.1)。
//!
//! 落盘约定:`data/attachments/{session_id}/{时间戳}_{安全文件名}`。
//! 文本类附件(docx/xlsx/pdf/txt/md/csv 等)保存时立即提取纯文本并分块入库;
//! 消息 JSON 只存元数据 + 摘要 + 最多 20,000 字符的正文摘录(全文在 `doc_chunks`),
//! 避免 200 页 PDF 把消息表撑大。
//!
//! 职责拆分:
//! - `meta.rs`:附件元数据、落盘目录与路径/文件名安全校验、文本提取入库;
//! - `save.rs`:保存 / 文本预览 / 字节读取命令;
//! - `center.rs`:附件中心聚合、再次引入会话、删除与引用统计。

// 命令子模块需对外可见:命令注册表使用子模块原始路径(Tauri 宏要求真实模块路径)。
pub(crate) mod center;
pub(crate) mod save;

mod meta;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use center::{
    AttachmentEntry, delete_attachment, insert_attachment_to_session, list_all_attachments,
};
#[allow(unused_imports)]
pub use meta::AttachmentMeta;
#[allow(unused_imports)]
pub use save::{extract_text_preview, read_attachment_data, save_attachment};

// 兄弟模块与测试复用的 DB/路径 helper(不扩大为 pub)。
#[allow(unused_imports)]
pub(crate) use center::{
    count_doc_id_refs_on, count_references_on, delete_one_on, display_name_from_path, list_all_on,
    remove_from_message_on,
};
#[allow(unused_imports)]
pub(crate) use meta::{
    MAX_ATTACHMENT_BYTES, ensure_in_attachments_dir, extract_and_index, sanitize_name,
};
