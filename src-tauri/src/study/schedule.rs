//! 周粒度排期、计划载荷落行、Markdown 导出与意图兜底修复。

use super::{Intent, PlanPayload, ScheduleWeek, StageSpec};

/// 计划载荷 → knowledge_nodes 行(先删后插,幂等;P0 仅建行,掌握度留给 P1)。
pub fn store_payload_nodes(
    conn: &rusqlite::Connection,
    plan_id: &str,
    payload: &PlanPayload,
) -> Result<(), String> {
    conn.execute("DELETE FROM knowledge_nodes WHERE plan_id=?1", [plan_id])
        .map_err(|e| format!("clear knowledge_nodes failed: {e}"))?;
    for s in &payload.stages {
        for t in &s.topics {
            let Some(n) = payload.nodes.iter().find(|n| &n.name == t) else {
                continue;
            };
            let prereqs = serde_json::to_string(&n.prereqs).map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO knowledge_nodes(plan_id, stage_index, name, difficulty, why, must_learn, prereq_json)
                 VALUES(?1,?2,?3,?4,?5,?6,?7)",
                rusqlite::params![
                    plan_id,
                    s.index as i64,
                    n.name,
                    n.difficulty as i64,
                    n.why,
                    n.must_learn as i64,
                    prereqs,
                ],
            )
            .map_err(|e| format!("insert knowledge_nodes failed: {e}"))?;
        }
    }
    Ok(())
}

/// 周粒度排期:阶段顺序展开,每周 = 对应阶段 + 每日分钟数;总周数上限 78。
pub fn calc_schedule(stages: &[StageSpec], daily_minutes: u32) -> Vec<ScheduleWeek> {
    let mut out = Vec::new();
    let mut week = 0u32;
    'outer: for s in stages {
        for _ in 0..s.estimated_weeks.max(1) {
            week += 1;
            if week > 78 {
                break 'outer;
            }
            out.push(ScheduleWeek {
                week,
                stage: s.index,
                focus: s.name.clone(),
                minutes_per_day: daily_minutes,
            });
        }
    }
    out
}

// ---------------- 导出 Markdown ----------------

/// 学习计划 → Markdown(导出用,纯函数可测)。
pub fn export_markdown(payload: &PlanPayload) -> String {
    let mut md = String::new();
    md.push_str(&format!("# 学习计划:{}\n\n", payload.topic));
    md.push_str(&format!("- **学习目标**:{}\n", payload.goal));
    md.push_str(&format!("- **当前水平**:{}\n", level_label(&payload.level)));
    md.push_str(&format!(
        "- **每日投入**:约 {} 分钟\n",
        payload.daily_minutes
    ));
    if let Some(d) = &payload.deadline {
        md.push_str(&format!("- **目标完成**:{d}\n"));
    }
    if payload.repaired {
        md.push_str("\n> 注:本计划为校验未通过后的降级结构(repaired),建议人工复核阶段划分。\n");
    }
    md.push_str("\n## 阶段规划\n\n");
    for s in &payload.stages {
        md.push_str(&format!(
            "### 阶段 {}:{}(难度 {} · 约 {} 周)\n\n",
            s.index + 1,
            s.name,
            s.difficulty,
            s.estimated_weeks
        ));
        if !s.goal.is_empty() {
            md.push_str(&format!("{}\n\n", s.goal));
        }
        md.push_str("| 知识点 | 难度 | 必学 | 说明 |\n|---|---|---|---|\n");
        for t in &s.topics {
            if let Some(n) = payload.nodes.iter().find(|n| &n.name == t) {
                md.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    n.name,
                    n.difficulty,
                    if n.must_learn { "✓" } else { "" },
                    n.why
                ));
            } else {
                md.push_str(&format!("| {t} | - | | |\n"));
            }
        }
        if !s.milestone.is_empty() {
            md.push_str(&format!("\n**里程碑**:{}\n", s.milestone));
        }
        if let Some(r) = payload.resources.iter().find(|r| r.stage == s.index) {
            md.push_str("\n**推荐资料**\n\n");
            for item in &r.items {
                md.push_str(&format!("- {item}\n"));
            }
        }
        md.push('\n');
    }
    if !payload.schedule.is_empty() {
        md.push_str("## 周排期\n\n");
        md.push_str("| 周 | 阶段 | 每日投入 |\n|---|---|---|\n");
        for w in &payload.schedule {
            md.push_str(&format!(
                "| 第 {} 周 | {} | 每天 {} 分钟 |\n",
                w.week, w.focus, w.minutes_per_day
            ));
        }
        md.push('\n');
    }
    md
}

pub(crate) fn level_label(level: &str) -> &'static str {
    match level {
        "zero" => "零基础",
        "beginner" => "初学者",
        "intermediate" => "中级",
        "advanced" => "高级",
        _ => "未指定",
    }
}

// ---------------- 意图兜底(确定性修复) ----------------

/// 意图修复:clamp 分钟数、goal 兜底、deadline 格式兜底。
pub(crate) fn fix_intent(mut i: Intent, fb_minutes: u32, fb_deadline: Option<String>) -> Intent {
    i.topic = i.topic.trim().to_string();
    if i.daily_minutes == 0 {
        i.daily_minutes = fb_minutes.max(1);
    }
    i.daily_minutes = i.daily_minutes.clamp(5, 600);
    if i.goal.trim().is_empty() {
        i.goal = i.topic.clone();
    }
    if let Some(d) = &i.deadline
        && (d.trim().is_empty() || d == "null")
    {
        i.deadline = None;
    }
    if i.deadline.is_none() {
        i.deadline = fb_deadline.filter(|d| !d.trim().is_empty());
    }
    if i.lang != "en" {
        i.lang = "zh".into();
    }
    i
}

/// 从残破/截断的模型输出中打捞 `"key"` 的原始值 token(字符串含引号;number/bool/null 为裸 token)。
/// 只用于意图的 6 个标量字段,容忍数组断裂、尾随闲话与未闭合字符串。
fn salvage_field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{key}\"");
    let mut from = 0usize;
    while let Some(pos) = text[from..].find(&pat) {
        let abs = from + pos;
        let after = text[abs + pat.len()..].trim_start();
        let Some(rest) = after.strip_prefix(':') else {
            from = abs + pat.len();
            continue;
        };
        let rest = rest.trim_start();
        if let Some(body) = rest.strip_prefix('"') {
            // 字符串值:扫描到未转义结束引号
            let start = text.len() - body.len();
            let bytes = body.as_bytes();
            let mut j = 0usize;
            while j < bytes.len() {
                match bytes[j] {
                    b'\\' => j += 2,
                    b'"' => return Some(&text[start..start + j + 1]),
                    _ => j += 1,
                }
            }
            return None; // 未闭合(截断),该字段放弃
        }
        let end = rest
            .find(|c: char| c == ',' || c == '}' || c == ']' || c.is_whitespace())
            .unwrap_or(rest.len());
        let tok = &rest[..end];
        if tok.is_empty() {
            return None;
        }
        return Some(tok);
    }
    None
}

fn salvage_str(text: &str, key: &str) -> String {
    salvage_field(text, key)
        .map(|t| serde_json::from_str::<String>(t).unwrap_or_else(|_| t.trim_matches('"').into()))
        .unwrap_or_default()
}

/// 严格解析失败时的意图打捞:缺的字段走默认值 → missing 重算 → 面板追问,永不报错。
pub(crate) fn salvage_intent(text: &str) -> Intent {
    let mut i = Intent {
        topic: salvage_str(text, "topic"),
        level: salvage_str(text, "level"),
        goal: salvage_str(text, "goal"),
        ..Default::default()
    };
    if let Some(m) = salvage_field(text, "daily_minutes") {
        i.daily_minutes = m.trim_matches('"').parse::<u32>().unwrap_or(0);
    }
    let dl = salvage_str(text, "deadline");
    if !dl.is_empty() && dl != "null" {
        i.deadline = Some(dl);
    }
    i
}

pub(crate) fn missing_fields(i: &Intent) -> Vec<String> {
    let mut out = Vec::new();
    if i.level.trim().is_empty() {
        out.push("level".into());
    }
    if i.daily_minutes == 0 {
        out.push("daily_minutes".into());
    }
    if i.deadline.is_none() {
        out.push("deadline".into());
    }
    out
}

pub(crate) fn default_questions(missing: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for f in missing {
        match f.as_str() {
            "level" => out.push("你目前的基础大概是什么水平?(零基础 / 有一定基础 / 熟练)".into()),
            "daily_minutes" => out.push("你每天大概能投入多少分钟学习?".into()),
            "deadline" => out.push("有没有期望完成的时间?(没有可留空)".into()),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::study::{NodeSpec, StageResource};

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
    fn intent_tolerates_explicit_nulls() {
        // DeepSeek 等模型会输出 "field": null,serde(default) 不兜显式 null,必须可解析
        let raw = r#"{"topic":"Java 前端","level":null,"goal":null,"daily_minutes":null,"deadline":null,"lang":"zh","missing":null,"questions":null}"#;
        let i: Intent = serde_json::from_str(raw).unwrap();
        assert_eq!(i.topic, "Java 前端");
        assert_eq!(i.daily_minutes, 0, "null 解析为 0,由 fix_intent 兜底");
        assert!(i.missing.is_empty());
        assert!(i.questions.is_empty());
    }

    #[test]
    fn salvage_intent_from_truncated_output() {
        // 复现「expected `,` or `]`」:questions 数组断裂 + 输出截断,严格解析失败
        let raw = r#"{"topic":"Java 前端设计","level":"zero","goal":"入门","daily_minutes":30,"deadline":"2026-12-09","lang":"zh","missing":["questions":["你每天"#;
        assert!(
            serde_json::from_str::<Intent>(raw).is_err(),
            "前置:严格解析必须失败"
        );
        let i = salvage_intent(raw);
        assert_eq!(i.topic, "Java 前端设计");
        assert_eq!(i.level, "zero");
        assert_eq!(i.daily_minutes, 30);
        assert_eq!(i.deadline.as_deref(), Some("2026-12-09"));
        assert!(i.missing.is_empty());
    }

    #[test]
    fn salvage_handles_broken_and_truncated_output() {
        // 复现「expected , or ]」:值内含逗号紧接下一键,严格解析失败但值本身闭合完整
        let raw =
            r#"{"topic":"我想入门学习 java 的前端设计,"level":"beginner","daily_minutes":45}"#;
        assert!(
            serde_json::from_str::<Intent>(raw).is_err(),
            "前置:严格解析必须失败"
        );
        let i = salvage_intent(raw);
        assert_eq!(i.topic, "我想入门学习 java 的前端设计,");
        assert_eq!(i.level, "beginner");
        assert_eq!(i.daily_minutes, 45);
        // 输出真被截断(字符串未闭合)→ 该字段放弃,其余照常
        let i2 = salvage_intent(r#"{"topic":"abc"#);
        assert_eq!(i2.topic, "");
        // 完全不是 JSON → 全默认(30 分钟),由 missing 重算兜底追问
        assert_eq!(salvage_intent("完全不是 JSON 的闲话回复").daily_minutes, 30);
    }

    #[test]
    fn schedule_expands_weeks_with_cap() {
        let mut stages = vec![stage(0, "a", 1, &["A"]), stage(1, "b", 2, &["B"])];
        stages[0].estimated_weeks = 2;
        stages[1].estimated_weeks = 3;
        let sch = calc_schedule(&stages, 30);
        assert_eq!(sch.len(), 5);
        assert_eq!(sch[0].week, 1);
        assert_eq!(sch[4].week, 5);
        assert_eq!(sch[4].focus, "b");
        assert_eq!(sch[4].minutes_per_day, 30);
        // 上限 78 周
        let mut big = vec![stage(0, "a", 1, &["A"])];
        big[0].estimated_weeks = 200;
        assert_eq!(calc_schedule(&big, 25).len(), 78);
    }

    #[test]
    fn export_markdown_contains_sections() {
        let payload = PlanPayload {
            topic: "Rust 入门".into(),
            goal: "8 周上手".into(),
            level: "zero".into(),
            daily_minutes: 30,
            stages: vec![stage(0, "基础", 1, &["所有权"])],
            nodes: vec![node("所有权", 2, true)],
            schedule: calc_schedule(&[stage(0, "基础", 1, &["所有权"])], 30),
            resources: vec![StageResource {
                stage: 0,
                items: vec!["The Book".into()],
            }],
            repaired: true,
            ..Default::default()
        };
        let md = export_markdown(&payload);
        assert!(md.contains("# 学习计划:Rust 入门"));
        assert!(md.contains("repaired"));
        assert!(md.contains("所有权"));
        assert!(md.contains("The Book"));
        assert!(md.contains("第 1 周"));
        assert!(md.contains("零基础"));
    }
}
