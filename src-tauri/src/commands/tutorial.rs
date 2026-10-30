//! 教程卡命令(迭代 D7.2)。
//!
//! 对应 idea/02 §7 内嵌教程系统:教程内容为项目根 `tutorials/*.md`,
//! 每文件一张卡,YAML front-matter 标注所属页面/标题/简介/排序,
//! 用户可直接改写 md 文件,重启后生效。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 一张教程卡(front-matter 元数据 + markdown 正文)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TutorialDoc {
    pub id: String,
    /// 所属页面:chat / draw / translate
    pub page: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub order: i64,
    /// front-matter 之后的 markdown 正文
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct FrontMatter {
    id: String,
    page: String,
    title: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    order: i64,
}

/// 教程目录:`<项目根>/tutorials`(与 data/ 同级)。
fn tutorials_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(|p| p.join("tutorials"))
        .unwrap_or_else(|| PathBuf::from("tutorials"))
}

/// 解析单个 md 文件:`---\n<yaml>\n---\n<body>`。
fn parse_tutorial(file_name: &str, raw: &str) -> Result<TutorialDoc, String> {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return Err(format!("{file_name}: 缺少 front-matter"));
    }
    let rest = &trimmed[3..];
    let end = rest
        .find("\n---")
        .ok_or_else(|| format!("{file_name}: front-matter 未闭合"))?;
    let fm: FrontMatter = serde_yaml::from_str(&rest[..end])
        .map_err(|e| format!("{file_name}: YAML 解析失败: {e}"))?;
    let content = rest[end + 4..].trim_start_matches(['\r', '\n']).to_string();
    Ok(TutorialDoc {
        id: fm.id,
        page: fm.page,
        title: fm.title,
        summary: fm.summary,
        order: fm.order,
        content,
    })
}

/// 扫描目录,返回指定页面的教程卡(按 order 升序)。
pub fn load_tutorials_from(dir: &PathBuf, page: &str) -> Vec<TutorialDoc> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().to_string();
            match parse_tutorial(&name, &raw) {
                Ok(doc) if doc.page == page => out.push(doc),
                Ok(_) => {}
                Err(e) => log::warn!("tutorial skipped: {e}"),
            }
        }
    }
    out.sort_by_key(|d| d.order);
    out
}

/// 列出某页面的全部教程卡。
#[tauri::command]
pub async fn list_tutorials(page: String) -> Vec<TutorialDoc> {
    load_tutorials_from(&tutorials_dir(), &page)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_tutorial() {
        let raw = "---\nid: t1\npage: chat\ntitle: 标题\nsummary: 一句话\norder: 2\n---\n\n正文 **md**\n";
        let doc = parse_tutorial("t1.md", raw).unwrap();
        assert_eq!(doc.id, "t1");
        assert_eq!(doc.page, "chat");
        assert_eq!(doc.order, 2);
        assert_eq!(doc.content, "正文 **md**\n");
    }

    #[test]
    fn parse_missing_front_matter_errors() {
        assert!(parse_tutorial("x.md", "# 无头部").is_err());
        assert!(parse_tutorial("x.md", "---\nid: t\npage: p\ntitle: t\n正文未闭合").is_err());
    }

    #[test]
    fn load_repo_tutorials_sorted_and_filtered() {
        // 仓库自带 tutorials/,至少含 chat 页教程
        let docs = load_tutorials_from(&tutorials_dir(), "chat");
        assert!(
            !docs.is_empty(),
            "repo tutorials/ should contain chat docs"
        );
        assert!(docs.iter().all(|d| d.page == "chat"));
        let orders: Vec<i64> = docs.iter().map(|d| d.order).collect();
        let mut sorted = orders.clone();
        sorted.sort();
        assert_eq!(orders, sorted);
    }
}
