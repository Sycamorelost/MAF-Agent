//! memory.md 的 YAML frontmatter 解析与序列化。
//!
//! 简化策略:
//! - 只用 serde_yaml 解析 `---...---` 之间的内容
//! - 缺字段给默认值,宽容解析
//! - 写回时保留我们认识的字段,其他字段直传(用户在 frontmatter 里加的自定义 key 不丢)

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// memory.md 顶部的 frontmatter 结构。
///
/// 注:已知字段走强类型;用户自定义字段进 `extra`,保证回写不丢。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    #[serde(default)]
    pub conversation: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub models_used: Vec<String>,
    #[serde(default = "default_version")]
    pub memory_version: u32,
    #[serde(default)]
    pub updated: String,

    /// 用户自定义字段,序列化时平铺回去
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

fn default_version() -> u32 {
    1
}

impl Default for Frontmatter {
    fn default() -> Self {
        Self {
            conversation: String::new(),
            id: String::new(),
            created: String::new(),
            models_used: Vec::new(),
            memory_version: 1,
            updated: String::new(),
            extra: BTreeMap::new(),
        }
    }
}

/// 把 memory.md 拆成 (frontmatter, body)。
/// 若无 frontmatter,frontmatter 返回 Default,body 为全文。
pub fn parse(text: &str) -> Result<(Frontmatter, String), String> {
    let t = text.trim_start_matches('\u{feff}'); // 去 BOM
    if !t.starts_with("---") {
        return Ok((Frontmatter::default(), text.to_string()));
    }
    // 找第二个 --- 行
    let mut iter = t.split_inclusive('\n');
    let first_line = iter.next().unwrap_or("---\n"); // "---\n"
    let mut fm_end_byte = first_line.len();
    let mut fm_str = String::new();
    let mut found = false;
    for line in iter {
        if line.trim_end() == "---" {
            found = true;
            break;
        }
        fm_str.push_str(line);
        fm_end_byte += line.len();
    }
    if !found {
        return Err("frontmatter 未闭合".into());
    }
    // fm_end_byte 此时指向 "---" 行起点;跳过该行才是正文
    let after_marker = &t[fm_end_byte..];
    let body = after_marker
        .lines()
        .skip(1) // 跳过 "---" 本身
        .collect::<Vec<_>>()
        .join("\n");

    let fm: Frontmatter =
        serde_yaml::from_str(&fm_str).map_err(|e| format!("frontmatter YAML 解析失败: {e}"))?;
    Ok((fm, body))
}

/// 序列化 frontmatter + body 回完整文本。
pub fn render(fm: &Frontmatter, body: &str) -> Result<String, String> {
    let yaml = serde_yaml::to_string(fm).map_err(|e| format!("frontmatter 序列化失败: {e}"))?;
    // serde_yaml 会输出 "conversation: xxx\n---\n" 不需要尾 ---;去掉它
    let yaml = yaml.trim_end();
    let yaml = yaml.strip_suffix("---").unwrap_or(yaml).trim_end();
    Ok(format!("---\n{}\n---\n\n{}", yaml, body.trim_start()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let text = "---\nconversation: \"测试\"\nid: abc123\ncreated: 2026-10-13 14:00\nmodels_used: [gpt-5.2]\nmemory_version: 1\nupdated: 2026-10-13 14:00\n---\n\n# 会话摘要\n\nhello\n";
        let (fm, body) = parse(text).unwrap();
        assert_eq!(fm.id, "abc123");
        assert_eq!(fm.conversation, "测试");
        assert_eq!(fm.models_used, vec!["gpt-5.2".to_string()]);
        assert!(body.contains("# 会话摘要"));

        let back = render(&fm, &body).unwrap();
        let (fm2, _) = parse(&back).unwrap();
        assert_eq!(fm2.id, "abc123");
        assert_eq!(fm2.memory_version, 1);
    }

    #[test]
    fn no_frontmatter_returns_default() {
        let (fm, body) = parse("# 直接正文").unwrap();
        assert_eq!(fm.id, "");
        assert_eq!(body, "# 直接正文");
    }

    #[test]
    fn extra_keys_preserved() {
        let text = "---\nid: x\ncustom_key: hello\n---\n\nbody";
        let (fm, _) = parse(text).unwrap();
        assert!(fm.extra.contains_key("custom_key"));
        let back = render(&fm, "body").unwrap();
        assert!(back.contains("custom_key: hello"));
    }
}
