//! MCP 工具名命名空间。
//!
//! 不同 server 完全可能暴露同名工具(`read_file` / `search` 等),早期实现把裸名
//! 直接下发给模型,路由靠 HashMap 迭代顺序取首个匹配 —— 既不确定也会串台。
//! 现在统一下发 `mcp__{server}__{tool}`,并由 runtime 用「候选名 == 本名下发展名」
//! 反查,不依赖字符串反解析,因此 server/tool 名里出现分隔符也不会歧义。

/// 命名空间前缀。
pub const PREFIX: &str = "mcp__";

/// OpenAI function name 上限。
const MAX_LEN: usize = 64;

/// 仅保留 `[A-Za-z0-9_-]`,其余替换为 `_`;并折叠连续下划线,
/// 保证 `__` 只作为命名空间分隔符出现。
fn sanitize(raw: &str) -> String {
    let mut s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    while s.contains("__") {
        s = s.replace("__", "_");
    }
    if s.is_empty() {
        s.push('_');
    }
    s
}

/// FNV-1a:短、稳定、无依赖,用于超长名字的确定性去重后缀。
fn fnv1a(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in s.as_bytes() {
        h ^= u32::from(*b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// `mcp__{sanitized_server}__` 前缀:判断某个下发展名属于哪台 server。
pub fn server_prefix(server: &str) -> String {
    format!("{PREFIX}{}__", sanitize(server))
}

/// 生成下发给模型的工具名:确定、合法、跨 server 唯一。
pub fn qualify(server: &str, tool: &str) -> String {
    let s = sanitize(server);
    let t = sanitize(tool);
    let full = format!("{PREFIX}{s}__{t}");
    if full.len() <= MAX_LEN {
        return full;
    }
    // 超长:保留 server 前缀 + 工具名前缀 + 名称哈希,避免截断后互相撞名
    let hash = format!("{:06x}", fnv1a(&full) & 0xff_ffff);
    let budget = MAX_LEN - PREFIX.len() - 2 - 1 - hash.len();
    let s_keep = s.len().min(20).min(budget.saturating_sub(9));
    let t_keep = budget - s_keep;
    format!(
        "{PREFIX}{}__{}_{hash}",
        &s[..s_keep],
        &t[..t_keep.min(t.len())]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualifies_with_stable_namespace() {
        assert_eq!(qualify("filesystem", "read_file"), "mcp__filesystem__read_file");
        assert_eq!(qualify("filesystem", "read_file"), qualify("filesystem", "read_file"));
    }

    #[test]
    fn same_tool_on_two_servers_never_collides() {
        assert_ne!(qualify("a", "search"), qualify("b", "search"));
    }

    #[test]
    fn sanitizes_illegal_chars_and_separators() {
        let n = qualify("my server", "read.file");
        assert_eq!(n, "mcp__my_server__read_file");
        // 业务名里的 __ 被折叠,分隔符唯一
        assert_eq!(qualify("a__b", "c__d"), "mcp__a_b__c_d");
    }

    #[test]
    fn long_names_stay_within_openai_limit_and_unique() {
        let a = qualify(&"s".repeat(60), &"t".repeat(60));
        let b = qualify(&"s".repeat(60), &format!("{}x", "t".repeat(60)));
        assert!(a.len() <= MAX_LEN, "{}", a.len());
        assert!(b.len() <= MAX_LEN);
        assert_ne!(a, b);
    }

    #[test]
    fn ascii_only_output() {
        let n = qualify("服务器", "读取文件");
        assert!(n.is_ascii());
    }
}
