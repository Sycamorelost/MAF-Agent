//! 预置知识库「一键重新导入」:恢复缺失 + 合并不覆盖。
//!
//! 合并语义(用户数据零丢失):
//!   - 章节级:用户正文缺失的 `## ` 章节整体追加到文末;
//!   - 表格级:用户已有章节内,同名表头的表格按「首列键」做并集——
//!     用户已有的行原样保留(冲突用户优先),预置独有的行补到该表末尾;
//!   - 章节内的散文/清单/代码块不触碰(视为用户主权内容);
//!   - 前言(`# 标题` + 引言)不参与合并,标题以 front-matter 为准。

use std::collections::HashSet;

/// 章节:`## xxx` 标题行 + 其后到下一 `## ` 之前的全部行。
pub(crate) struct Section {
    pub heading: String,
    pub lines: Vec<String>,
}

/// 把预置正文切成章节列表(首个 `## ` 之前的前言直接丢弃,不参与合并)。
pub(crate) fn split_sections(body: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut cur: Option<Section> = None;
    for line in body.lines() {
        if line.starts_with("## ") {
            if let Some(s) = cur.take() {
                out.push(s);
            }
            cur = Some(Section {
                heading: line.to_string(),
                lines: Vec::new(),
            });
        } else if let Some(s) = cur.as_mut() {
            s.lines.push(line.to_string());
        }
    }
    if let Some(s) = cur {
        out.push(s);
    }
    out
}

fn is_table_line(l: &str) -> bool {
    l.trim_start().starts_with('|')
}

/// 分隔行:`| --- | :---: |` 形态(必须含至少一个 `-`)。
fn is_sep_line(l: &str) -> bool {
    let t = l.trim();
    t.starts_with('|')
        && t.contains('-')
        && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// 表格块 = 连续表格行;返回每块的 (起始下标, 结束下标开区间)。
fn table_blocks(lines: &[String]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if is_table_line(&lines[i]) {
            let mut j = i;
            while j < lines.len() && is_table_line(&lines[j]) {
                j += 1;
            }
            out.push((i, j));
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// 行键 = 首列内容(去竖线、trim、小写),大小写不敏感防重复补行。
fn row_key(row: &str) -> String {
    row.trim()
        .trim_start_matches('|')
        .split('|')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase()
}

/// 去掉 lines 尾部空行;必要时补一个空行分隔后再追加。
fn push_block(lines: &mut Vec<String>, block: impl IntoIterator<Item = String>) {
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    if !lines.is_empty() {
        lines.push(String::new());
    }
    lines.extend(block);
}

/// 把缺失章节整体追加到文末(剥掉章节内容首尾空行)。
fn append_section(lines: &mut Vec<String>, sec: &Section) {
    let body: Vec<String> = sec
        .lines
        .iter()
        .skip_while(|l| l.trim().is_empty())
        .cloned()
        .collect();
    let mut body = body;
    while body.last().is_some_and(|l| l.trim().is_empty()) {
        body.pop();
    }
    let mut block = vec![sec.heading.clone()];
    block.extend(body);
    push_block(lines, block);
}

/// 合并预置正文到用户正文。返回 (合并后正文, 合并细节列表);
/// 细节为空 = 用户内容已包含全部预置要点(调用方视为「重复跳过」)。
pub(crate) fn merge_body(user_body: &str, preset_body: &str) -> (String, Vec<String>) {
    let mut lines: Vec<String> = user_body.lines().map(|s| s.to_string()).collect();
    let mut details: Vec<String> = Vec::new();

    for sec in split_sections(preset_body) {
        let heading = sec.heading.trim().to_string();
        let label = heading.trim_start_matches('#').trim().to_string();

        // 章节缺失 → 整体追加到文末
        if !lines.iter().any(|l| l.trim() == heading) {
            append_section(&mut lines, &sec);
            details.push(format!("恢复章节「{label}」"));
            continue;
        }

        // 章节存在 → 逐表格并集(每次变更后重算范围,防止下标漂移)
        for (ps, pe) in table_blocks(&sec.lines) {
            let pt: Vec<String> = sec.lines[ps..pe].to_vec();
            let header = pt[0].trim().to_string();

            let h = lines
                .iter()
                .position(|l| l.trim() == heading)
                .expect("章节标题刚判定存在");
            let end = lines
                .iter()
                .skip(h + 1)
                .position(|l| l.starts_with("## "))
                .map(|p| h + 1 + p)
                .unwrap_or(lines.len());

            // 用户章节范围内找同名表头的表格
            let target = table_blocks(&lines[h + 1..end])
                .into_iter()
                .map(|(s, e)| (h + 1 + s, h + 1 + e))
                .find(|(s, _)| lines[*s].trim() == header);

            match target {
                Some((ts, te)) => {
                    let user_keys: HashSet<String> = lines[ts + 1..te]
                        .iter()
                        .filter(|l| !is_sep_line(l))
                        .map(|l| row_key(l))
                        .collect();
                    let insert: Vec<String> = pt[1..]
                        .iter()
                        .filter(|r| !is_sep_line(r))
                        .filter(|r| !user_keys.contains(&row_key(r)))
                        .cloned()
                        .collect();
                    if !insert.is_empty() {
                        details.push(format!("表格「{label}」补充 {} 行", insert.len()));
                        lines.splice(te..te, insert);
                    }
                }
                None => {
                    // 表格整体缺失 → 插到章节标题之后
                    let mut block = vec![String::new()];
                    block.extend(pt);
                    lines.splice(h + 1..h + 1, block);
                    details.push(format!("恢复表格({label})"));
                }
            }
        }
    }

    (lines.join("\n"), details)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRESET: &str = "# 测试表\n\n> 引言。\n\n## 一、基础\n\n| 英文 | 中文 | 备注 |\n| --- | --- | --- |\n| function | 函数 | |\n| variable | 变量 | |\n\n## 二、进阶\n\n| 英文 | 中文 | 备注 |\n| --- | --- | --- |\n| cache | 缓存 | |\n| thread | 线程 | |\n\n说明文字。\n";

    #[test]
    fn identical_body_no_change() {
        let (_merged, details) = merge_body(PRESET, PRESET);
        assert!(details.is_empty(), "完全相同应无合并动作: {details:?}");
    }

    #[test]
    fn user_edits_kept_and_missing_rows_added() {
        // 用户:删了 variable 行、改 function 译名、加了自定义行
        let user = "# 测试表\n\n## 一、基础\n\n| 英文 | 中文 | 备注 |\n| --- | --- | --- |\n| function | 函式(我的译法) | |\n| my-term | 我的术语 | |\n\n## 二、进阶\n\n| 英文 | 中文 | 备注 |\n| --- | --- | --- |\n| cache | 缓存 | |\n| thread | 线程 | |\n\n说明文字。\n";
        let (merged, details) = merge_body(user, PRESET);
        assert!(merged.contains("函式(我的译法)"), "用户改写的行必须保留");
        assert!(merged.contains("my-term"), "用户自增行必须保留");
        assert!(merged.contains("| variable | 变量 | |"), "缺失行应补回");
        assert_eq!(details, vec!["表格「一、基础」补充 1 行"]);
        // 幂等:再合并应无动作
        let (_m2, d2) = merge_body(&merged, PRESET);
        assert!(d2.is_empty(), "合并后应幂等: {d2:?}");
    }

    #[test]
    fn missing_section_appended_at_end() {
        let user = "# 测试表\n\n## 一、基础\n\n| 英文 | 中文 | 备注 |\n| --- | --- | --- |\n| function | 函数 | |\n| variable | 变量 | |\n";
        let (merged, details) = merge_body(user, PRESET);
        assert!(details.iter().any(|d| d.contains("恢复章节「二、进阶」")));
        // 追加在文末,且包含表格与散文
        let pos_sec = merged.find("## 二、进阶").unwrap();
        let pos_base = merged.find("## 一、基础").unwrap();
        assert!(pos_sec > pos_base, "恢复的章节应追加在已有内容之后");
        assert!(merged.contains("| cache | 缓存 | |"));
        assert!(merged.contains("说明文字。"));
    }

    #[test]
    fn foreign_body_appends_all_sections_keeps_user_text() {
        let user = "# 完全无关的笔记\n\n今天的心情记录。";
        let (merged, details) = merge_body(user, PRESET);
        assert!(merged.starts_with("# 完全无关的笔记\n\n今天的心情记录。"), "用户原文必须在开头原样保留");
        assert!(merged.contains("## 一、基础") && merged.contains("## 二、进阶"));
        assert_eq!(details.len(), 2, "两个章节都应恢复: {details:?}");
    }

    #[test]
    fn empty_user_body_becomes_preset_sections() {
        let (merged, details) = merge_body("", PRESET);
        assert!(merged.contains("## 一、基础"));
        assert!(merged.contains("| function | 函数 | |"));
        assert_eq!(details.len(), 2);
    }

    #[test]
    fn case_insensitive_key_not_duplicated() {
        let user = "## 一、基础\n\n| 英文 | 中文 | 备注 |\n| --- | --- | --- |\n| FUNCTION | 函数 | |\n| variable | 变量 | |\n\n## 二、进阶\n\n| 英文 | 中文 | 备注 |\n| --- | --- | --- |\n| cache | 缓存 | |\n| thread | 线程 | |\n\n说明文字。\n";
        let (_merged, details) = merge_body(user, PRESET);
        assert!(details.is_empty(), "大小写差异不应造成重复补行: {details:?}");
    }

    #[test]
    fn missing_table_inserted_after_heading() {
        let user = "## 一、基础\n\n我把整个表删了。\n\n## 二、进阶\n\n| 英文 | 中文 | 备注 |\n| --- | --- | --- |\n| cache | 缓存 | |\n| thread | 线程 | |\n\n说明文字。\n";
        let (merged, details) = merge_body(user, PRESET);
        assert!(details.iter().any(|d| d.contains("恢复表格(一、基础)")));
        assert!(merged.contains("我把整个表删了。"), "用户散文必须保留");
        let h = merged.find("## 一、基础").unwrap();
        let t = merged.find("| 英文 | 中文 | 备注 |").unwrap();
        assert!(t > h && t < merged.find("我把整个表删了").unwrap(), "表格应插回章节标题之后");
    }
}
