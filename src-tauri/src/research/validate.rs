//! 模型输出解析与阶段校验。

use super::events::SourceItem;

pub(crate) enum Decision {
    Finish,
    Search(Vec<String>),
}

pub(crate) enum StepAction {
    Search(Vec<String>),
    Finish,
}

pub(crate) struct Step {
    pub(crate) thought: String,
    pub(crate) action: StepAction,
}

/// 必覆盖角度校验:角度关键词在资料标题/摘要与观察文本中均未出现视为缺失(提醒,不阻断)。
pub(crate) fn verify_angles(
    angles: &[String],
    sources: &[SourceItem],
    observations: &[String],
) -> Vec<String> {
    if angles.is_empty() {
        return Vec::new();
    }
    let mut hay = String::new();
    for s in sources {
        hay.push_str(&s.title);
        hay.push_str(&s.snippet);
    }
    for o in observations {
        hay.push_str(o);
    }
    angles
        .iter()
        .map(|a| a.trim())
        .filter(|a| !a.is_empty() && !hay.contains(a))
        .map(String::from)
        .collect()
}

/// 解析模型输出为 Step(Thought + Action)。
/// 容错:找不到 Action 行时按「空查询的 search」处理(上层再兜底)。
pub(crate) fn parse_step(text: &str, max_queries: u32) -> Step {
    let lines: Vec<&str> = text.lines().collect();

    let action_idx = lines.iter().position(|l| {
        let t = l.trim();
        t.starts_with("Action:") || t.starts_with("Action\u{FF1A}")
    });

    let thought = extract_thought(&lines, action_idx);

    let action = match action_idx {
        Some(i) => {
            let line = lines[i].trim();
            let value = line
                .splitn(2, |c| c == ':' || c == '\u{FF1A}')
                .nth(1)
                .unwrap_or("")
                .trim()
                .to_lowercase();
            if value.contains("finish") {
                StepAction::Finish
            } else {
                StepAction::Search(collect_queries(&lines[i + 1..], max_queries))
            }
        }
        None => StepAction::Search(Vec::new()),
    };

    Step { thought, action }
}

fn extract_thought(lines: &[&str], action_idx: Option<usize>) -> String {
    let start = lines.iter().position(|l| {
        let t = l.trim();
        t.starts_with("Thought:") || t.starts_with("Thought\u{FF1A}")
    });
    match start {
        Some(i) => {
            let mut parts: Vec<String> = Vec::new();
            let first = lines[i].trim();
            let after = first
                .splitn(2, |c| c == ':' || c == '\u{FF1A}')
                .nth(1)
                .unwrap_or("")
                .trim();
            if !after.is_empty() {
                parts.push(after.to_string());
            }
            let end = action_idx.unwrap_or(lines.len()).min(lines.len());
            for l in &lines[i + 1..end] {
                let t = l.trim();
                if !t.is_empty() {
                    parts.push(t.to_string());
                }
            }
            parts.join(" ")
        }
        None => {
            let end = action_idx.unwrap_or(lines.len());
            lines[..end]
                .iter()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

/// 从 Action 行之后收集查询词(编号列表或短横列表),clamp 到 max。
fn collect_queries(lines: &[&str], max: u32) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in lines {
        let t = l.trim();
        if t.is_empty() {
            if !out.is_empty() {
                break;
            }
            continue;
        }
        // 跳过 "Queries:" 头
        if t.to_lowercase().starts_with("queries") {
            continue;
        }
        match strip_list_marker(t) {
            Some(q) => {
                let q = q.trim().trim_matches(|c| c == '"' || c == '"').to_string();
                if !q.is_empty() {
                    out.push(q);
                }
            }
            None => {
                // 已开始收集后遇到非列表行 → 结束
                if !out.is_empty() {
                    break;
                }
            }
        }
        if out.len() >= max as usize {
            break;
        }
    }
    out
}

/// 剥离列表标记:`- xxx` / `1. xxx` / `1、xxx` / `1) xxx` / `1: xxx`。
pub(crate) fn strip_list_marker(line: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix('-') {
        return Some(rest.trim().to_string());
    }
    let mut idx = 0;
    for ch in line.chars() {
        if ch.is_ascii_digit() {
            idx += ch.len_utf8();
        } else {
            break;
        }
    }
    if idx > 0 {
        let rest = &line[idx..];
        let mut chars = rest.chars();
        if let Some(sep) = chars.next() {
            if matches!(sep, '.' | '、' | ')' | ':' | '\u{FF1A}') {
                return Some(chars.collect::<String>().trim().to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_search_step() {
        let text = "Thought: 目前资料不足,需要查最新数据\nAction: search\nQueries:\n1. 深度研究 市场 规模\n2. ReAct 循环 实现\n3. AI agent 搜索 案例";
        let step = parse_step(text, 4);
        assert!(step.thought.contains("资料不足"));
        match step.action {
            StepAction::Search(qs) => {
                assert_eq!(qs.len(), 3);
                assert_eq!(qs[0], "深度研究 市场 规模");
            }
            StepAction::Finish => panic!("expected search"),
        }
    }

    #[test]
    fn parse_finish_step() {
        let text = "Thought: 资料已足够覆盖主题\nAction: finish";
        let step = parse_step(text, 4);
        assert!(matches!(step.action, StepAction::Finish));
        assert_eq!(step.thought, "资料已足够覆盖主题");
    }

    #[test]
    fn parse_fullwidth_colons_and_dash_list() {
        let text = "Thought\u{FF1A}先看看定义\nAction\u{FF1A}search\nQueries\u{FF1A}\n- searxng json api\n- tavily 搜索";
        let step = parse_step(text, 4);
        assert_eq!(step.thought, "先看看定义");
        match step.action {
            StepAction::Search(qs) => {
                assert_eq!(qs.len(), 2);
                assert_eq!(qs[0], "searxng json api");
            }
            StepAction::Finish => panic!("expected search"),
        }
    }

    #[test]
    fn parse_fallback_without_action() {
        let text = "我觉得应该继续搜索相关内容";
        let step = parse_step(text, 4);
        match step.action {
            StepAction::Search(qs) => assert!(qs.is_empty()),
            StepAction::Finish => panic!("expected search fallback"),
        }
        assert!(!step.thought.is_empty());
    }

    #[test]
    fn queries_clamped_to_max() {
        let text = "Thought: t\nAction: search\n1. a\n2. b\n3. c\n4. d\n5. e";
        let step = parse_step(text, 2);
        match step.action {
            StepAction::Search(qs) => assert_eq!(qs.len(), 2),
            StepAction::Finish => panic!(),
        }
    }
}
