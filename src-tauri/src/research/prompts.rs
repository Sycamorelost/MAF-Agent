//! 提示词模板与消息拼装。

use crate::provider::ChatMessage;

use super::events::SourceItem;
use super::params::with_references;
use super::{EngineParams, skills, template};

/// 决策步上下文:legacy 与 staged 共用一套 ReAct 决策逻辑,差异全部由本结构注入。
pub(crate) struct DecideCtx<'a> {
    /// 阶段序号(legacy = 0)
    pub(crate) stage_no: u32,
    /// 阶段名(legacy = "")
    pub(crate) name: &'a str,
    /// 阶段目标
    pub(crate) goal: &'a str,
    pub(crate) max_rounds: u32,
    pub(crate) queries_per_round: u32,
    pub(crate) reflect: &'a str,
    pub(crate) read_length: &'a str,
    pub(crate) snippet_chars: usize,
    /// 生效决策模板(阶段覆盖 > 逐次覆盖 > templates.json > 内置,调用方已合并)
    pub(crate) decide_template: String,
    /// 允许模型判定「资料足够」提前 finish
    pub(crate) early_stop: bool,
    /// 前阶段摘要汇编(无则为 "")
    pub(crate) prev_summaries: &'a str,
    /// 前阶段资料明细摘要块(inherit_sources=true 时注入)
    pub(crate) inherit_digest: &'a str,
    pub(crate) must_cover_angles: &'a [String],
    pub(crate) query_language: &'a str,
    /// 科研增强:检索方法论片段(空 = 关闭)
    pub(crate) skill_query: String,
}

impl<'a> DecideCtx<'a> {
    pub(crate) fn legacy(p: &'a EngineParams) -> Self {
        Self {
            stage_no: 0,
            name: "",
            goal: "",
            max_rounds: p.params.max_rounds,
            queries_per_round: p.params.queries_per_round,
            reflect: &p.params.reflect,
            read_length: &p.params.read_length,
            snippet_chars: p.params.snippet_chars(),
            decide_template: p.params.decide_template(),
            early_stop: true,
            prev_summaries: "",
            inherit_digest: "",
            must_cover_angles: &[],
            query_language: "auto",
            skill_query: skills::fragments(&p.params).query,
        }
    }
}

/// ReAct 系统提示(轮次信息运行时注入)。
pub(crate) const REACT_SYSTEM: &str = r#"你是深度研究 Agent,使用 ReAct 循环(思考 → 检索 → 观察)围绕主题收集资料。

每轮你必须严格按以下两种格式之一输出:

格式 A(继续检索):
Thought: 你对当前资料的分析与下一步检索思路(中文,2-4 句)
Action: search
Queries:
1. 查询词一
2. 查询词二

格式 B(资料已足够,结束检索):
Thought: 总结为什么资料已经足够(中文)
Action: finish

规则:
1. 当前是第 {round}/{max_rounds} 轮;最后一轮必须输出 Action: finish。
2. 每轮最多 {queries_per_round} 条查询,查询用编号列表给出。
3. 查询词不要与历史观察中出现过的重复;变换角度(定义/现状/数据/争议/案例)。
4. 已收集资料不足以支撑一份结构化研究报告时,不要轻易 finish。
5. 只输出上述格式内容,不要输出任何其他文字。"#;

/// 语言代码 → 展示名。
fn lang_label(code: &str) -> String {
    match code {
        "zh" => "中文",
        "en" => "English",
        _ => "自动",
    }
    .to_string()
}

pub(crate) fn build_step_messages(
    p: &EngineParams,
    ctx: &DecideCtx<'_>,
    round: u32,
    sources: &[SourceItem],
    observations: &[String],
) -> Vec<ChatMessage> {
    // 决策模板:阶段覆盖/逐次覆盖/templates.json 自定义/内置(调用方已合并),未知占位符原样保留
    let system = template::render(
        &ctx.decide_template,
        &[
            ("topic", p.topic.clone()),
            ("round", round.to_string()),
            ("max_rounds", ctx.max_rounds.to_string()),
            ("queries_per_round", ctx.queries_per_round.to_string()),
            ("stage_name", ctx.name.to_string()),
            ("stage_goal", ctx.goal.to_string()),
            ("must_cover_angles", ctx.must_cover_angles.join("、")),
            ("query_language", lang_label(ctx.query_language)),
            ("prev_summaries", ctx.prev_summaries.to_string()),
        ],
    );
    // 科研增强:检索方法论片段追加到决策 system prompt(关闭时为空,静默降级)
    let system = if ctx.skill_query.is_empty() {
        system
    } else {
        format!("{system}\n\n{}", ctx.skill_query)
    };
    // 用户导入的参考资料:决策阶段据此避免重复检索已提供的资料
    let system = with_references(system, p);

    let mut user = format!(
        "研究主题: {}\n\n当前进度: 第 {}/{} 轮\n",
        p.topic, round, ctx.max_rounds
    );
    if !ctx.name.is_empty() {
        user.push_str(&format!("当前阶段: {}(目标:{})\n", ctx.name, ctx.goal));
    }
    if !ctx.must_cover_angles.is_empty() {
        user.push_str(&format!(
            "必覆盖角度(硬性要求):{}\n",
            ctx.must_cover_angles.join("、")
        ));
    }
    if ctx.query_language != "auto" {
        user.push_str(&format!(
            "查询语言倾向:{}\n",
            lang_label(ctx.query_language)
        ));
    }
    if !ctx.prev_summaries.is_empty() {
        user.push_str("\n【前阶段摘要】\n");
        user.push_str(ctx.prev_summaries);
        user.push('\n');
    }
    if !ctx.inherit_digest.is_empty() {
        user.push_str("\n【前阶段资料明细】\n");
        user.push_str(ctx.inherit_digest);
    }
    if !sources.is_empty() {
        user.push_str("\n【已收集资料】\n");
        let max_total = if ctx.read_length == "full" {
            16000
        } else {
            6000
        };
        user.push_str(&sources_digest(sources, ctx.snippet_chars, max_total));
    }
    if !observations.is_empty() {
        user.push_str("\n【历史观察】\n");
        for o in observations {
            let trimmed: String = o.chars().take(800).collect();
            user.push_str(&trimmed);
            user.push('\n');
        }
    }
    user.push_str("\n请输出本轮 Thought 与 Action。");

    vec![
        ChatMessage {
            role: "system".into(),
            content: system,
            images: vec![],
            ..Default::default()
        },
        ChatMessage {
            role: "user".into(),
            content: user,
            images: vec![],
            ..Default::default()
        },
    ]
}

/// 「仅开头规划」策略的规划提示:一次性输出全部查询。
const PLAN_ALL_SYSTEM: &str = r#"你是深度研究 Agent 的规划器。围绕主题一次性规划全部检索查询。

严格按以下格式输出:
Thought: 你的检索规划思路(中文,2-4 句,说明覆盖了哪些角度)
Action: search
Queries:
1. 查询词一
2. 查询词二
...

规则:
1. 总计规划 {total} 条查询,按执行顺序排列(先基础定义与现状,后数据/案例/争议)。
2. 查询角度多样:定义 / 现状 / 数据 / 争议 / 案例 / 对比,避免语义重复。
3. 每条查询不超过 30 字,使用空格分隔关键词。
4. 只输出上述格式内容,不要输出任何其他文字。"#;

pub(crate) fn build_plan_all_messages(p: &EngineParams, total: u32) -> Vec<ChatMessage> {
    let mut system = PLAN_ALL_SYSTEM.replace("{total}", &total.to_string());
    // 科研增强:规划器同样接收检索方法论片段
    let skill_query = skills::fragments(&p.params).query;
    if !skill_query.is_empty() {
        system.push_str("\n\n");
        system.push_str(&skill_query);
    }
    let user = format!(
        "研究主题: {}\n\n请一次性规划全部 {} 条检索查询。",
        p.topic, total
    );
    // 参考资料:让规划阶段知道已有资料范围,避免重复检索
    let user = if let Some(b) = p.references.as_deref() {
        format!("{user}\n\n{}", b)
    } else {
        user
    };
    vec![
        ChatMessage {
            role: "system".into(),
            content: system,
            images: vec![],
            ..Default::default()
        },
        ChatMessage {
            role: "user".into(),
            content: user,
            images: vec![],
            ..Default::default()
        },
    ]
}

/// 资料摘要块:`[id] title — url\nsnippet`,整块字符数受 max_total 限制。
pub(crate) fn sources_digest(
    sources: &[SourceItem],
    max_snippet: usize,
    max_total: usize,
) -> String {
    let mut out = String::new();
    for s in sources {
        let snippet: String = s.snippet.chars().take(max_snippet).collect();
        let entry = format!("[{}] {} — {}\n{}\n\n", s.id, s.title, s.url, snippet);
        if out.len() + entry.len() > max_total {
            break;
        }
        out.push_str(&entry);
    }
    out
}

pub(crate) const REPORT_SYSTEM: &str = r#"你是研究报告撰写器。基于给定的资料列表,输出 Markdown 研究报告。

结构要求(严格按此章节顺序):
# 《{topic}》研究报告
## TL;DR
(3-5 句核心结论)
## 主要发现
(分点陈述,每点附引用编号)
## 分析
(对比 / 趋势 / 归因)
## 局限
(资料缺口与不确定性)
## 资料来源
(本节留空,由系统生成)

规则:
1. 正文中的论断用 [n] 引用编号标注来源,编号对应资料列表中的 [n]。
2. 不得虚构资料列表之外的事实与引用编号。
3. 只输出 Markdown 正文,不要用代码块包裹,不要输出解释性前言。
4. 对比数据用 Markdown 管道表格,表题写在表格上一行(如「表 1 …」);标题层级用 ## / ###,不跳级。"#;

/// 英文报告模板(输出语言 = English 时使用)。
pub(crate) const REPORT_SYSTEM_EN: &str = r#"You are a research report writer. Based on the given source list, output a Markdown research report in English.

Required structure (strict order):
# Research Report: {topic}
## TL;DR
(3-5 sentences of core conclusions)
## Key Findings
(bulleted; each point cites [n])
## Analysis
(comparison / trends / attribution)
## Limitations
(gaps and uncertainty in the sources)
## Sources
(leave empty; generated by the system)

Rules:
1. Cite claims with [n] matching the source list.
2. Never fabricate facts or citation numbers outside the source list.
3. Output Markdown body only; no code fences, no preamble.
4. Use Markdown pipe tables for comparisons, with the caption on the line above (e.g. "Table 1 ..."); use ## / ### headings without skipping levels."#;
