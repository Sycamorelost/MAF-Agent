//! 阶段摘要与撰写阶段的 LLM 生成。

use crate::provider::ChatMessage;

use super::super::EngineParams;
use super::super::llm::llm_collect;
use super::super::params::with_references;
use super::super::prompts::sources_digest;
use super::super::skills;
use super::super::stage;
use super::super::template;
use super::{Pool, RunError, assemble_summaries};

/// 阶段摘要系统提示(内置;可被 stage.summary_prompt 覆盖)。
const STAGE_SUMMARY_SYSTEM: &str = r#"你是研究阶段摘要撰写器。基于本阶段收集的资料与检索观察,输出该阶段的结构化摘要。

要求:
1. 3-6 句,围绕阶段目标提炼核心结论;论断用 [n] 引用编号标注来源,编号对应资料列表。
2. 若给出了必覆盖角度缺口或资料缺口,在末尾一句话如实说明。
3. 只输出摘要正文,不要标题、不要代码块、不要前言。"#;

/// 生成阶段摘要(检索阶段收尾;custom summary_prompt 覆盖内置)。
/// H3:拿到真实 usage 时计入预算(不支持的渠道保持原「摘要不计费」行为)。
// 参数为流水线阶段所需的独立上下文(引擎参数/阶段配置/检索结果/预算等)
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_stage_summary(
    p: &EngineParams,
    st: &stage::StageConfig,
    eff: &stage::EffStage,
    pool: &mut Pool,
    stage_start: usize,
    observations: &[String],
    missing: &[String],
    gap_note: Option<&str>,
) -> Result<String, RunError> {
    let tpl = st
        .summary_prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(STAGE_SUMMARY_SYSTEM);
    let max_total = if eff.read_length == "full" {
        12000
    } else {
        8000
    };
    let digest = sources_digest(&pool.sources[stage_start..], 400, max_total);
    let mut system = template::render(
        tpl,
        &[
            ("topic", p.topic.clone()),
            ("stage_name", st.name.clone()),
            ("stage_goal", st.goal.clone()),
            ("sources_digest", digest.clone()),
            (
                "output_lang",
                if p.params.output_lang == "en" {
                    "English".to_string()
                } else {
                    "中文".to_string()
                },
            ),
        ],
    );
    // 科研增强:资料消化纪律注入
    let skill_digest = skills::fragments(&p.params).digest;
    if !skill_digest.is_empty() {
        system.push_str("\n\n");
        system.push_str(&skill_digest);
    }
    // 用户导入的参考资料:阶段摘要阶段可直接引用
    let system = with_references(system, p);
    let mut user = format!(
        "研究主题: {}\n阶段: {}(目标:{})\n\n【本阶段资料】\n{}",
        p.topic, st.name, st.goal, digest
    );
    if !observations.is_empty() {
        user.push_str("\n【检索观察】\n");
        for o in observations.iter().take(12) {
            let trimmed: String = o.chars().take(500).collect();
            user.push_str(&trimmed);
            user.push('\n');
        }
    }
    if !missing.is_empty() {
        user.push_str(&format!("\n必覆盖角度缺口:{}\n", missing.join("、")));
    }
    if let Some(g) = gap_note {
        user.push_str(&format!("\n资料缺口:{g}\n"));
    }
    user.push_str("\n请输出本阶段摘要。");

    let (text, usage) = llm_collect(
        p,
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
        ],
    )
    .await?;
    if let Some(u) = usage {
        pool.est_tokens += u;
    }
    Ok(text.trim().to_string())
}

/// 撰写阶段系统提示(内置;可被 stage.summary_prompt 覆盖)。
const SYNTHESIS_SYSTEM: &str = r#"你是研究分析撰写器。不发起新检索,仅基于给定资料与前期阶段摘要,产出本阶段的结构化分析文本。

要求:
1. 围绕阶段目标组织内容;论断用 [n] 引用编号标注来源,编号对应资料列表。
2. 充分利用前期阶段摘要中的结论,保持口径一致。
3. 只输出正文(Markdown),不要用代码块包裹。"#;

/// 撰写阶段:不检索,基于既有资料池 + 前阶段摘要产出分析文本(即该阶段摘要)。
/// H3:拿到真实 usage 时计入预算(不支持的渠道保持原「撰写不计费」行为)。
pub(crate) async fn run_synthesis(
    p: &EngineParams,
    st: &stage::StageConfig,
    pool: &mut Pool,
    summaries: &[(String, String)],
) -> Result<String, RunError> {
    let tpl = st
        .summary_prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(SYNTHESIS_SYSTEM);
    let digest = sources_digest(&pool.sources, 400, 16000);
    let prev = assemble_summaries(summaries);
    let mut system = template::render(
        tpl,
        &[
            ("topic", p.topic.clone()),
            ("stage_name", st.name.clone()),
            ("stage_goal", st.goal.clone()),
            ("sources_digest", digest.clone()),
            ("prev_summaries", prev.clone()),
            (
                "output_lang",
                if p.params.output_lang == "en" {
                    "English".to_string()
                } else {
                    "中文".to_string()
                },
            ),
        ],
    );
    // 科研增强:撰写阶段接收报告写作纪律
    let skill_syn = skills::fragments(&p.params).synthesis;
    if !skill_syn.is_empty() {
        system.push_str("\n\n");
        system.push_str(&skill_syn);
    }
    // 用户导入的参考资料:撰写阶段直接作为事实依据
    let system = with_references(system, p);
    let mut user = format!(
        "研究主题: {}\n阶段: {}(目标:{})\n",
        p.topic, st.name, st.goal
    );
    if !prev.is_empty() {
        user.push_str("\n【前阶段摘要】\n");
        user.push_str(&prev);
    }
    user.push_str(&format!(
        "\n\n【资料列表】\n{digest}\n请输出本阶段分析文本。"
    ));

    let (text, usage) = llm_collect(
        p,
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
        ],
    )
    .await?;
    if let Some(u) = usage {
        pool.est_tokens += u;
    }
    Ok(text.trim().to_string())
}
