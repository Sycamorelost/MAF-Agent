//! Agent 模板(迭代 B2 → R3 A3/B4 → R4 收口):22 个内置模板 + 用户变体/自定义模板。
//!
//! 对应 idea/07-agent-templates.md(已按 2026-10-13 决策舍弃 mcp_tools 字段)。
//! 内置模板硬编码;用户创建的变体/自定义模板落盘 `data/agents.json`。
//! 记忆读写矩阵:memory_strategy 驱动注入策略(full/summary/none),
//! memory_write 决定是否参与自动提炼,memory_sections 标注写入哪个节(展示用)。
//!
//! 职责拆分:
//! - `template.rs`:模板结构与分类、v2 字段默认值、提示词变量渲染;
//! - `builtin.rs`:22 个内置模板定义;
//! - `store.rs`:自定义模板的落盘读写与查询/增删校验。

mod builtin;
mod store;
mod template;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use builtin::builtin_templates;
#[allow(unused_imports)]
pub use store::{find, list_all, load_custom, remove_custom, upsert_custom};
#[allow(unused_imports)]
pub use template::{AgentTemplate, CATEGORIES, render_prompt};
// 测试与兄弟子模块使用的默认值构造器(不扩大为 pub)。
#[allow(unused_imports)]
pub(crate) use template::v2_defaults;
