//! memory.md 默认骨架。
//!
//! 对应 idea/03-memory-flow.md §1.1:YAML frontmatter + 6 节固定结构。
//! 用户可自由增删节;提炼器「只更新已知节、保留用户自定义节」。

use chrono::{Local, SecondsFormat};

/// 已知的 6 个标准节,顺序固定。
pub const KNOWN_SECTIONS: [&str; 6] = [
    "会话摘要",
    "关键事实与结论",
    "用户偏好与约束",
    "待办 / 未决问题",
    "文档摘要",
    "资料",
];

/// frontmatter 默认值模板。
pub fn render(id: &str, title: &str, model_id: &str) -> String {
    let now = Local::now().to_rfc3339_opts(SecondsFormat::Secs, false);
    let mut s = String::with_capacity(1024);
    s.push_str("---\n");
    s.push_str(&format!("conversation: \"{}\"\n", escape_yaml(title)));
    s.push_str(&format!("id: {}\n", id));
    s.push_str(&format!("created: {}\n", now));
    s.push_str(&format!("models_used: [{}]\n", model_id));
    s.push_str("memory_version: 1\n");
    s.push_str(&format!("updated: {}\n", now));
    s.push_str("---\n\n");

    for sec in KNOWN_SECTIONS {
        s.push_str(&format!("# {}\n\n", sec));
    }
    s
}

/// YAML 双引号字符串的最低限度转义。
fn escape_yaml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_contains_frontmatter_and_six_sections() {
        let s = render("abc123de", "测试会话", "gpt-5.2");
        assert!(s.starts_with("---\n"));
        assert!(s.contains("id: abc123de"));
        assert!(s.contains("conversation: \"测试会话\""));
        for sec in KNOWN_SECTIONS {
            assert!(s.contains(&format!("# {}", sec)), "missing section: {}", sec);
        }
    }
}
