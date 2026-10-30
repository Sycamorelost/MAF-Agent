//! 预设三级合并(D19)与工具名单匹配。
//!
//! 生效顺序:会话钉住(sessions.preset_id)> Agent 模板默认(agents.preset)> 全局默认
//! (settings.default_preset);任一级 id 无效(被删/覆盖文件损坏)落到下一级,
//! 最终回退 standard,再退清单第一项 —— 预设永远可解析,不产生「无预设」状态。

use serde::Serialize;

use super::{DEFAULT_PRESET, Preset, ToolFilter};

/// 生效来源:session / agent / global。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PresetSource {
    Session,
    Agent,
    Global,
}

/// 三级合并结果。
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedPreset {
    pub preset: Preset,
    pub source: PresetSource,
}

fn find<'a>(presets: &'a [Preset], id: &str) -> Option<&'a Preset> {
    presets.iter().find(|p| p.id == id)
}

/// 解析生效预设:会话 > Agent > 全局默认 > standard > 清单首项。
pub fn resolve(
    presets: &[Preset],
    session_preset_id: Option<&str>,
    agent_preset: Option<&str>,
    default_preset: &str,
) -> ResolvedPreset {
    if let Some(p) = session_preset_id.and_then(|id| find(presets, id)) {
        return ResolvedPreset { preset: p.clone(), source: PresetSource::Session };
    }
    if let Some(p) = agent_preset.and_then(|id| find(presets, id)) {
        return ResolvedPreset { preset: p.clone(), source: PresetSource::Agent };
    }
    let p = find(presets, default_preset)
        .or_else(|| find(presets, DEFAULT_PRESET))
        .or_else(|| presets.first())
        .expect("预设清单为空:内置四档必须存在");
    ResolvedPreset { preset: p.clone(), source: PresetSource::Global }
}

/// 工具是否放行:include 命中(精确名或 `*`)且未被 exclude 命中。
/// exclude 的 `*` 只排除「未显式 include」的工具 —— 白名单模式,
/// 对应内置 JSON 的 `"include": [...], "exclude": ["*"]` 写法。
pub fn tool_allowed(f: &ToolFilter, name: &str) -> bool {
    let included = f.include.iter().any(|p| p == "*" || p == name);
    if !included {
        return false;
    }
    let named_exclude = f.exclude.iter().any(|p| p == name);
    let star_exclude =
        f.exclude.iter().any(|p| p == "*") && !f.include.iter().any(|p| p == name);
    !named_exclude && !star_exclude
}

/// 按预设过滤工具定义列表(内置与 MCP 工具共用)。
pub fn filter_defs<T>(f: &ToolFilter, tools: Vec<T>, name_of: impl Fn(&T) -> &str) -> Vec<T> {
    tools.into_iter().filter(|t| tool_allowed(f, name_of(t))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::builtin;

    fn presets() -> Vec<Preset> {
        builtin::presets()
    }

    #[test]
    fn resolve_session_wins_then_agent_then_global() {
        let list = presets();
        // 会话钉住优先
        let r = resolve(&list, Some("deep"), Some("code"), "standard");
        assert_eq!(r.preset.id, "deep");
        assert_eq!(r.source, PresetSource::Session);
        // 会话未钉 → Agent 模板
        let r = resolve(&list, None, Some("code"), "standard");
        assert_eq!(r.preset.id, "code");
        assert_eq!(r.source, PresetSource::Agent);
        // 两级都空 → 全局默认
        let r = resolve(&list, None, None, "economy");
        assert_eq!(r.preset.id, "economy");
        assert_eq!(r.source, PresetSource::Global);
    }

    #[test]
    fn resolve_invalid_ids_fall_back() {
        let list = presets();
        // 会话 id 无效 → 落 Agent;Agent 无效 → 全局;全局无效 → standard
        let r = resolve(&list, Some("gone"), Some("code"), "standard");
        assert_eq!(r.preset.id, "code");
        let r = resolve(&list, Some("gone"), Some("gone"), "gone");
        assert_eq!(r.preset.id, DEFAULT_PRESET);
        assert_eq!(r.source, PresetSource::Global);
    }

    #[test]
    fn tool_allowlist_star_exclude() {
        let list = presets();
        let eco = find(&list, "economy").unwrap();
        // 白名单内放行
        assert!(tool_allowed(&eco.tools, "fs_read"));
        assert!(tool_allowed(&eco.tools, "shell_exec"));
        // 白名单外(含写入类与任意 MCP 工具)被 exclude:* 排除
        assert!(!tool_allowed(&eco.tools, "fs_write"));
        assert!(!tool_allowed(&eco.tools, "delegate"));
        assert!(!tool_allowed(&eco.tools, "some_mcp_tool"));
    }

    #[test]
    fn tool_star_include_allows_all() {
        let list = presets();
        for id in ["deep", "code"] {
            let p = find(&list, id).unwrap();
            assert!(tool_allowed(&p.tools, "delegate"));
            assert!(tool_allowed(&p.tools, "fs_write"));
            assert!(tool_allowed(&p.tools, "any_mcp_tool"));
        }
        let std = find(&list, "standard").unwrap();
        assert!(tool_allowed(&std.tools, "fs_write"));
        assert!(tool_allowed(&std.tools, "todo_write"));
        assert!(!tool_allowed(&std.tools, "delegate"));
        assert!(!tool_allowed(&std.tools, "fs_delete"));
    }

    #[test]
    fn named_exclude_beats_star_include() {
        let f = ToolFilter {
            include: vec!["*".into()],
            exclude: vec!["fs_delete".into()],
        };
        assert!(tool_allowed(&f, "fs_write"));
        assert!(!tool_allowed(&f, "fs_delete"));
    }
}
