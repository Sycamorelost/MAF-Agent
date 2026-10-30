//! memory / session / message 相关 Tauri commands。
//!
//! 拆分(§10.2.1):`crud` 收敛会话 / 消息 / 分支与设置命令,
//! `diff` 收敛 memory.md 读写、版本与提炼命令;
//! 本文件保留对外路径再导出(`crate::commands::memory::*`)。

// 命令重导出用于保持 crate::commands::memory::xxx 原路径;注册表改走子模块路径后,
// 这些 re-export 在本 crate 内没有直接调用点。
#![allow(unused_imports)]

pub(crate) mod crud;
pub(crate) mod diff;

use serde::Serialize;

use crate::memory;

#[derive(Debug, Serialize)]
pub struct MemoryDto {
    pub frontmatter: memory::frontmatter::Frontmatter,
    pub body: String,
}

#[derive(Debug, Serialize)]
pub struct VersionItem {
    pub id: i64,
    pub created: String,
    pub note: String,
}

/// 记忆注入预算状态(迭代 D3.5):当前正文估算 tokens / 预算上限 / 是否超限。
#[derive(Debug, Serialize)]
pub struct BudgetStatus {
    pub tokens: usize,
    pub limit: usize,
    pub over: bool,
}

pub use crud::{
    AppendMessageInput, SessionInfo, append_message, clear_messages, create_session, delete_branch,
    delete_message, delete_messages_from, delete_session, delete_session_if_empty, fork_branch,
    get_session, list_branches, list_messages, list_sessions, prune_empty_sessions, rename_branch,
    rename_session, set_active_branch, set_session_memory_strategy, set_session_model,
    set_session_permission_mode, set_session_web_search, set_session_workbench_mode,
    update_message, update_session_settings,
};
pub use diff::{
    SummarizeMemoryInput, append_memory, list_memory_versions, memory_budget_status,
    needs_summarize, read_memory, read_memory_version, rollback_memory, snapshot_memory,
    summarize_memory, unwatch_memory, watch_memory, write_memory,
};

#[cfg(test)]
mod tests {
    use super::diff::{append_to_section, resolve_section};

    #[test]
    fn resolve_section_defaults_to_ziliao() {
        assert_eq!(resolve_section(None).unwrap(), "资料");
        assert_eq!(resolve_section(Some("")).unwrap(), "资料");
        assert_eq!(resolve_section(Some("  ")).unwrap(), "资料");
    }

    #[test]
    fn resolve_section_accepts_all_known_sections() {
        for sec in crate::memory::template::KNOWN_SECTIONS {
            assert_eq!(resolve_section(Some(sec)).unwrap(), sec);
        }
    }

    #[test]
    fn resolve_section_rejects_unknown() {
        assert!(resolve_section(Some("不存在的节")).is_err());
        assert!(resolve_section(Some("资料; DROP TABLE")).is_err());
    }

    #[test]
    fn append_to_section_creates_heading_when_missing() {
        let out = append_to_section("# 会话摘要\n\nhello", "资料", "- [a](http://x)");
        assert!(out.contains("# 会话摘要"), "原内容保留");
        assert!(out.contains("# 资料\n\n- [a](http://x)"), "自动补标题");
    }

    #[test]
    fn append_to_section_inserts_under_existing_heading() {
        let body = "# 资料\n\n- 旧条目\n\n# 待办\n\n- [ ] x";
        let out = append_to_section(body, "资料", "- 新条目");
        let zi = out.find("- 新条目").unwrap();
        let todo = out.find("# 待办").unwrap();
        assert!(zi < todo, "新条目应插在「待办」之前");
        assert!(out.contains("- 旧条目"), "旧条目保留");
    }

    #[test]
    fn append_to_section_at_end_when_no_following_heading() {
        let body = "# 摘要\n\ns\n\n# 资料\n\n- 旧";
        let out = append_to_section(body, "资料", "- 新");
        assert!(
            out.contains("# 资料\n\n\n- 新") || out.contains("# 资料\n\n- 新"),
            "插入到资料节"
        );
        assert!(out.contains("- 旧"), "旧内容保留");
    }
}
