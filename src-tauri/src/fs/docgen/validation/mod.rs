//! 包级校验器:对已生成的 docx 解包做 OPC 一致性检查。
//!
//! 仅测试使用(`#[cfg(test)]` 注册),`quick-xml` 是 dev-dependency,
//! 因此不增加任何运行时依赖与开销。

mod opc;

pub use opc::validate_package;
