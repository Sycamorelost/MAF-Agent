//! ReAct 决策步:按反思策略产出本轮查询。

use super::super::events::{EventSink, ResearchEvent, SourceItem};
use super::super::llm::{est_tok, llm_collect};
use super::super::prompts::{build_plan_all_messages, build_step_messages, DecideCtx};
use super::super::validate::{parse_step, Decision, StepAction};
use super::super::EngineParams;
use super::RunError;

/// 非反思轮沿用上轮查询时的角度轮换词(避免与历史查询完全重复)。
const ANGLES: [&str; 4] = ["最新进展", "数据与规模", "典型案例", "争议与风险"];

/// 按反思策略产出本轮决策。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn decide(
    p: &EngineParams,
    sink: &EventSink,
    ctx: &DecideCtx<'_>,
    round: u32,
    sources: &[SourceItem],
    observations: &[String],
    planned: &mut Option<Vec<String>>,
    last_queries: &mut Vec<String>,
    est_tokens: &mut u64,
) -> Result<Decision, RunError> {
    let qpr = ctx.queries_per_round;
    match ctx.reflect {
        // 仅开头规划:第 1 轮让模型一次性规划全部查询,后续按批消费
        "final" => {
            if let Some(plan) = planned.as_ref() {
                let start = ((round - 1) * qpr) as usize;
                if start >= plan.len() {
                    return Ok(Decision::Finish);
                }
                let slice: Vec<String> = plan[start..].iter().take(qpr as usize).cloned().collect();
                sink.emit(ResearchEvent::Thought {
                    round,
                    content: format!("按开头规划执行第 {round} 批检索。"),
                    stage: ctx.stage_no,
                });
                return Ok(Decision::Search(slice));
            }
            let total = ctx.max_rounds.saturating_mul(qpr).min(50);
            let messages = build_plan_all_messages(p, total);
            let prompt_tok = messages.iter().map(|m| est_tok(&m.content)).sum::<u64>();
            let (text, usage) = llm_collect(p, messages).await?;
            // H3:真实 usage 优先校准,不支持时回退字符估算
            *est_tokens += usage.unwrap_or(prompt_tok + est_tok(&text));
            let step = parse_step(&text, total);
            sink.emit(ResearchEvent::Thought {
                round,
                content: step.thought.clone(),
                stage: ctx.stage_no,
            });
            let qs = match step.action {
                StepAction::Search(qs) if !qs.is_empty() => qs,
                _ => vec![p.topic.clone()],
            };
            let first: Vec<String> = qs.iter().take(qpr as usize).cloned().collect();
            *planned = Some(qs);
            Ok(Decision::Search(first))
        }
        // 每 3 轮反思:仅第 1、4、7… 轮走完整决策步,中间轮沿用上轮查询 + 角度词
        "every3" => {
            let reflect_now = round == 1 || (round - 1) % 3 == 0;
            if !reflect_now && !last_queries.is_empty() {
                let angle = ANGLES[(round as usize) % ANGLES.len()];
                let qs: Vec<String> = last_queries
                    .iter()
                    .map(|q| format!("{q} {angle}"))
                    .take(qpr as usize)
                    .collect();
                sink.emit(ResearchEvent::Thought {
                    round,
                    content: format!("沿用上轮检索思路,侧重「{angle}」。"),
                    stage: ctx.stage_no,
                });
                return Ok(Decision::Search(qs));
            }
            let d = llm_decide(p, sink, ctx, round, sources, observations, est_tokens).await?;
            if let Decision::Search(qs) = &d {
                *last_queries = qs.clone();
            }
            Ok(d)
        }
        // 每轮反思(默认)
        _ => llm_decide(p, sink, ctx, round, sources, observations, est_tokens).await,
    }
}

/// 完整 ReAct 决策步:让模型输出 Thought + Action(search/finish),含兜底规则。
async fn llm_decide(
    p: &EngineParams,
    sink: &EventSink,
    ctx: &DecideCtx<'_>,
    round: u32,
    sources: &[SourceItem],
    observations: &[String],
    est_tokens: &mut u64,
) -> Result<Decision, RunError> {
    let messages = build_step_messages(p, ctx, round, sources, observations);
    let prompt_tok = messages.iter().map(|m| est_tok(&m.content)).sum::<u64>();
    let (text, usage) = llm_collect(p, messages).await?;
    // H3:真实 usage 优先校准,不支持时回退字符估算
    *est_tokens += usage.unwrap_or(prompt_tok + est_tok(&text));
    let step = parse_step(&text, ctx.queries_per_round);

    sink.emit(ResearchEvent::Thought {
        round,
        content: step.thought.clone(),
        stage: ctx.stage_no,
    });

    let decision = match step.action {
        StepAction::Finish => {
            if !ctx.early_stop && round < ctx.max_rounds {
                // 阶段约束「必须跑满轮次」:finish 转为带角度词的继续检索
                let angle = ANGLES[(round as usize) % ANGLES.len()];
                Decision::Search(vec![format!("{} {angle}", p.topic)])
            } else if !sources.is_empty() || round == ctx.max_rounds {
                Decision::Finish
            } else {
                // 一条资料都没有就 finish:强制先按主题搜一轮
                Decision::Search(vec![p.topic.clone()])
            }
        }
        StepAction::Search(qs) => {
            if qs.is_empty() {
                if round == 1 {
                    Decision::Search(vec![p.topic.clone()])
                } else {
                    Decision::Finish
                }
            } else {
                Decision::Search(qs)
            }
        }
    };
    Ok(decision)
}
