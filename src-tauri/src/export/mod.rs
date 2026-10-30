//! 通用导出能力(P0 自 commands/research 迁出):另存为路径令牌与导出路径规范。
//!
//! 安全模型:N-6 令牌(渲染层不能凭空指定任意路径)+ H-2 路径规范与覆盖校验。
//! 研究报告导出与会话导出(memory/crud.rs)复用同一套校验。

pub mod path;

#[allow(unused_imports)]
pub use path::{
    EXPORT_TARGET_EXISTS, ExportPathTokens, SavePathPick, ensure_export_path, ensure_writable,
};
