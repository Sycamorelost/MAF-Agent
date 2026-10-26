//! 计划结构的确定性校验、解析修复与知识点覆盖检查(不依赖模型自检)。

use super::{Intent, NodeSpec, StageResource, StageSpec};

/// 从模型输出中提取 JSON 对象(容忍代码围栏与前后闲话)。
pub fn extract_json(text: &str) -> Option<String> {
    let s = text.trim();
    let s = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```"))
        .unwrap_or(s);
    let s = s.strip_suffix("```").unwrap_or(s).trim();
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end < start {
        return None;
    }
    Some(s[start..=end].to_string())
}

/// 阶段结构确定性校验,返回问题列表(空 = 通过)。
pub fn validate_stages(stages: &[StageSpec], nodes: &[NodeSpec]) -> Vec<String> {
    let mut issues = Vec::new();
    if stages.len() < 3 || stages.len() > 6 {
        issues.push(format!("阶段数必须为 3-6 个,当前 {} 个", stages.len()));
        return issues;
    }
    let known: std::collections::HashSet<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut covered_must: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (si, s) in stages.iter().enumerate() {
        if s.topics.is_empty() {
            issues.push(format!("阶段 {}「{}」没有知识点", si + 1, s.name));
            continue;
        }
        if s.topics.len() > 6 {
            issues.push(format!(
                "阶段 {}「{}」知识点过多({} 个 > 6)",
                si + 1,
                s.name,
                s.topics.len()
            ));
        }
        for t in &s.topics {
            if !known.contains(t.as_str()) {
                issues.push(format!(
                    "阶段 {}「{}」引用了地图外知识点「{}」",
                    si + 1,
                    s.name,
                    t
                ));
            }
            if !seen.insert(t.as_str()) {
                issues.push(format!("知识点「{t}」被多个阶段重复安排"));
            }
        }
        if s.milestone.trim().is_empty() {
            issues.push(format!("阶段 {}「{}」缺少里程碑", si + 1, s.name));
        }
        if s.estimated_weeks == 0 {
            issues.push(format!("阶段 {}「{}」周数不能为 0", si + 1, s.name));
        }
        if si > 0 {
            let diff = (s.difficulty as i32) - (stages[si - 1].difficulty as i32);
            if diff.abs() > 2 {
                issues.push(format!(
                    "阶段 {}→{} 难度跳级({} → {}),相邻差不得超过 2",
                    si,
                    si + 1,
                    stages[si - 1].difficulty,
                    s.difficulty
                ));
            }
        }
    }
    for n in nodes {
        if n.must_learn && !seen.contains(n.name.as_str()) {
            covered_must.insert(n.name.as_str());
        }
    }
    if !covered_must.is_empty() {
        let names: Vec<&str> = covered_must.into_iter().take(5).collect();
        issues.push(format!("必学知识点未被安排:{}", names.join("、")));
    }
    issues
}

/// 降级方案:按难度排序均分 3 个阶段(标 repaired),保证覆盖全部知识点。
pub fn fallback_repaired(nodes: &[NodeSpec], intent: &Intent) -> Vec<StageSpec> {
    let mut sorted: Vec<&NodeSpec> = nodes.iter().collect();
    sorted.sort_by(|a, b| {
        a.difficulty
            .cmp(&b.difficulty)
            .then(b.must_learn.cmp(&a.must_learn))
    });
    let n = sorted.len().max(1);
    let per = n.div_ceil(3);
    let names = ["基础入门", "进阶强化", "综合应用"];
    let mut out = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let chunk: Vec<&NodeSpec> = sorted
            .iter()
            .skip(i * per)
            .take(if i == 2 { n - i * per } else { per })
            .copied()
            .collect();
        if chunk.is_empty() {
            continue;
        }
        let difficulty = chunk.iter().map(|c| c.difficulty).max().unwrap_or(1);
        let milestone = chunk
            .iter()
            .find(|c| c.must_learn)
            .map(|c| format!("掌握{}", c.name))
            .unwrap_or_else(|| format!("完成「{name}」阶段全部知识点学习"));
        out.push(StageSpec {
            index: out.len() as u32,
            name: name.to_string(),
            goal: format!("{}:{}", name, intent.goal),
            difficulty,
            estimated_weeks: 2,
            topics: chunk.iter().map(|c| c.name.clone()).collect(),
            milestone,
        });
    }
    out
}

/// 未被任何阶段覆盖的知识点。
pub(crate) fn uncovered_nodes(nodes: &[NodeSpec], stages: &[StageSpec]) -> Option<Vec<String>> {
    let seen: std::collections::HashSet<&str> = stages
        .iter()
        .flat_map(|s| s.topics.iter().map(|t| t.as_str()))
        .collect();
    let missing: Vec<String> = nodes
        .iter()
        .filter(|n| !seen.contains(n.name.as_str()))
        .map(|n| n.name.clone())
        .collect();
    if missing.is_empty() {
        None
    } else {
        Some(missing)
    }
}

// ---------------- 解析与修复 ----------------

/// 解析知识地图输出,做确定性修复(clamp 难度、去重、修剪悬空前置)。
pub(crate) fn parse_nodes(v: &serde_json::Value) -> Vec<NodeSpec> {
    let mut out: Vec<NodeSpec> = Vec::new();
    let Some(arr) = v.get("nodes").and_then(|x| x.as_array()) else {
        return out;
    };
    for item in arr {
        let name = item
            .get("name")
            .and_then(|x| x.as_str())
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        if name.is_empty() || name.chars().count() > 24 {
            continue;
        }
        if out.iter().any(|n: &NodeSpec| n.name == name) {
            continue;
        }
        let difficulty = item
            .get("difficulty")
            .and_then(|x| x.as_u64())
            .unwrap_or(3)
            .clamp(1, 5) as u8;
        let why = item
            .get("why")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let must_learn = item
            .get("must_learn")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        let prereqs = item
            .get("prereqs")
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        out.push(NodeSpec {
            name,
            difficulty,
            why,
            must_learn,
            prereqs,
        });
    }
    // 悬空前置引用修剪
    let known: std::collections::HashSet<String> = out.iter().map(|n| n.name.clone()).collect();
    for n in &mut out {
        n.prereqs.retain(|p| known.contains(p) && *p != n.name);
    }
    out
}

/// 解析阶段输出(裸解析,不修复)。
pub(crate) fn parse_stages(v: &serde_json::Value) -> Vec<StageSpec> {
    let mut out = Vec::new();
    let Some(arr) = v.get("stages").and_then(|x| x.as_array()) else {
        return out;
    };
    for (i, item) in arr.iter().enumerate() {
        let name = item
            .get("name")
            .and_then(|x| x.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or_else(|| format!("阶段 {}", i + 1));
        let goal = item
            .get("goal")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let difficulty = item
            .get("difficulty")
            .and_then(|x| x.as_u64())
            .unwrap_or(1)
            .clamp(1, 5) as u8;
        let estimated_weeks = item
            .get("estimated_weeks")
            .and_then(|x| x.as_u64())
            .unwrap_or(2)
            .clamp(1, 26) as u32;
        let topics = item
            .get("topics")
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let milestone = item
            .get("milestone")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        out.push(StageSpec {
            index: i as u32,
            name,
            goal,
            difficulty,
            estimated_weeks,
            topics,
            milestone,
        });
    }
    out
}

/// 阶段修复:补默认名/周数;剔除重复与地图外知识点(不删阶段,校验器会继续给反馈)。
pub(crate) fn fix_stages(stages: &mut [StageSpec], nodes: &[NodeSpec]) {
    let known: std::collections::HashSet<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, s) in stages.iter_mut().enumerate() {
        s.index = i as u32;
        if s.estimated_weeks == 0 {
            s.estimated_weeks = 2;
        }
        s.topics.retain(|t| {
            known.contains(t.as_str()) && !seen.contains(t) && {
                seen.insert(t.clone());
                true
            }
        });
    }
}

/// 解析资料推荐,clamp 阶段序号与条数。
pub(crate) fn parse_resources(v: &serde_json::Value, stage_count: usize) -> Vec<StageResource> {
    let mut out = Vec::new();
    let Some(arr) = v.get("resources").and_then(|x| x.as_array()) else {
        return out;
    };
    for item in arr {
        let stage = match item.get("stage").and_then(|x| x.as_u64()) {
            Some(s) if (s as usize) < stage_count => s as u32,
            _ => continue,
        };
        let mut items: Vec<String> = item
            .get("items")
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .take(6)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        if items.is_empty() {
            continue;
        }
        items.dedup();
        if let Some(existing) = out
            .iter_mut()
            .find(|r: &&mut StageResource| r.stage == stage)
        {
            existing.items.extend(items);
        } else {
            out.push(StageResource { stage, items });
        }
    }
    out.sort_by_key(|r| r.stage);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, d: u8, must: bool) -> NodeSpec {
        NodeSpec {
            name: name.into(),
            difficulty: d,
            why: String::new(),
            must_learn: must,
            prereqs: Vec::new(),
        }
    }

    fn stage(i: usize, name: &str, d: u8, topics: &[&str]) -> StageSpec {
        StageSpec {
            index: i as u32,
            name: name.into(),
            goal: "g".into(),
            difficulty: d,
            estimated_weeks: 2,
            topics: topics.iter().map(|s| s.to_string()).collect(),
            milestone: "m".into(),
        }
    }

    #[test]
    fn extract_json_handles_fences_and_noise() {
        assert_eq!(
            extract_json("```json\n{\"a\":1}\n```").unwrap(),
            "{\"a\":1}"
        );
        assert_eq!(extract_json("好的:{\"a\":1} 以上").unwrap(), "{\"a\":1}");
        assert!(extract_json("no json here").is_none());
    }

    #[test]
    fn validate_rejects_bad_structures() {
        let nodes = vec![
            node("A", 1, true),
            node("B", 2, false),
            node("C", 3, true),
            node("D", 4, false),
        ];
        // 阶段过少
        let st = vec![stage(0, "s", 1, &["A"])];
        assert!(
            validate_stages(&st, &nodes)
                .iter()
                .any(|i| i.contains("阶段数"))
        );
        // 难度跳级(相邻差 >2)
        let st = vec![
            stage(0, "s1", 1, &["A"]),
            stage(1, "s2", 2, &["B"]),
            stage(2, "s3", 5, &["C", "D"]),
        ];
        assert!(
            validate_stages(&st, &nodes)
                .iter()
                .any(|i| i.contains("跳级"))
        );
        // 地图外知识点 + 必学缺失
        let st = vec![
            stage(0, "s1", 1, &["B", "X"]),
            stage(1, "s2", 2, &["D"]),
            stage(2, "s3", 3, &["A"]),
        ];
        let issues = validate_stages(&st, &nodes);
        assert!(issues.iter().any(|i| i.contains("地图外")));
        assert!(issues.iter().any(|i| i.contains("必学")));
        // 重复安排
        let st = vec![
            stage(0, "s1", 1, &["A", "B"]),
            stage(1, "s2", 2, &["B", "C"]),
            stage(2, "s3", 3, &["D"]),
        ];
        assert!(
            validate_stages(&st, &nodes)
                .iter()
                .any(|i| i.contains("重复"))
        );
    }

    #[test]
    fn validate_accepts_good_structure() {
        let nodes = vec![
            node("A", 1, true),
            node("B", 2, false),
            node("C", 3, true),
            node("D", 4, false),
        ];
        let st = vec![
            stage(0, "s1", 1, &["A", "B"]),
            stage(1, "s2", 2, &["C"]),
            stage(2, "s3", 3, &["D"]),
        ];
        assert!(validate_stages(&st, &nodes).is_empty());
    }

    #[test]
    fn fallback_covers_all_nodes_and_flags_repaired() {
        let intent = Intent {
            topic: "Rust".into(),
            ..Default::default()
        };
        let nodes: Vec<NodeSpec> = (0..9)
            .map(|i| node(&format!("N{i}"), (i % 5 + 1) as u8, i % 2 == 0))
            .collect();
        let stages = fallback_repaired(&nodes, &intent);
        assert_eq!(stages.len(), 3);
        let total: usize = stages.iter().map(|s| s.topics.len()).sum();
        assert_eq!(total, 9, "降级方案必须覆盖全部知识点");
        assert!(
            !validate_stages(&stages, &nodes)
                .iter()
                .any(|i| i.contains("必学"))
        );
    }

    #[test]
    fn parse_nodes_fixes_and_prunes() {
        let v: serde_json::Value = serde_json::json!({
            "nodes": [
                {"name": "所有权", "difficulty": 9, "why": "w", "must_learn": true, "prereqs": ["不存在"]},
                {"name": "所有权", "difficulty": 2},
                {"name": "", "difficulty": 2},
                {"name": "借用", "difficulty": 2, "prereqs": ["所有权", "借用"]},
            ]
        });
        let nodes = parse_nodes(&v);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].difficulty, 5, "难度 clamp 到 5");
        assert!(nodes[0].prereqs.is_empty(), "悬空前置被修剪");
        assert_eq!(nodes[1].prereqs, vec!["所有权".to_string()], "自引用被修剪");
    }

    #[test]
    fn parse_resources_clamps_stage_and_dedups() {
        let v: serde_json::Value = serde_json::json!({
            "resources": [
                {"stage": 0, "items": ["The Book", "The Book", "Rustlings"]},
                {"stage": 9, "items": ["越界"]},
                {"stage": 1, "items": []},
            ]
        });
        let r = parse_resources(&v, 2);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].stage, 0);
        assert_eq!(
            r[0].items,
            vec!["The Book".to_string(), "Rustlings".to_string()]
        );
    }
}
