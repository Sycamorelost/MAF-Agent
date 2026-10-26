//! 快速研究(legacy):单层 ReAct 循环。

use std::collections::HashSet;

use super::super::EngineParams;
use super::super::events::{EventSink, ResearchEvent, SourceItem};
use super::super::export::run_report;
use super::super::llm::est_tok;
use super::super::prompts::DecideCtx;
use super::super::store;
use super::super::validate::Decision;
use super::decide::decide;
use super::{RunError, check_cancel, wait_gate};

/// 快速研究(legacy):原单层 ReAct 循环,事件 stage 字段恒为 0。
pub(crate) async fn run_legacy(p: &EngineParams, sink: &EventSink) -> Result<String, RunError> {
    let mut sources: Vec<SourceItem> = Vec::new();
    let mut seen_urls: HashSet<String> = HashSet::new();
    let mut next_id: u32 = 1;
    let mut observations: Vec<String> = Vec::new();
    let mut searches_done: u32 = 0;
    let mut est_tokens: u64 = 0;
    // final 策略:第 1 轮一次性规划出的全部查询
    let mut planned: Option<Vec<String>> = None;
    // every3 策略:非反思轮沿用的查询
    let mut last_queries: Vec<String> = Vec::new();
    let ctx = DecideCtx::legacy(p);

    for round in 1..=p.params.max_rounds {
        check_cancel(p)?;
        wait_gate(p, sink, round, 0).await?;
        sink.emit(ResearchEvent::RoundStart {
            round,
            max_rounds: p.params.max_rounds,
            stage: 0,
        });

        // 1. 决策步(按反思策略分派:完整 ReAct / 每 3 轮 / 仅开头规划)
        let decision = decide(
            p,
            sink,
            &ctx,
            round,
            &sources,
            &observations,
            &mut planned,
            &mut last_queries,
            &mut est_tokens,
        )
        .await?;

        let queries = match decision {
            Decision::Finish => {
                let _ = store::update_rounds(&p.id, round as i64);
                break;
            }
            Decision::Search(qs) => qs,
        };

        // 2. 执行本轮查询
        sink.emit(ResearchEvent::Queries {
            round,
            queries: queries.clone(),
            stage: 0,
        });

        let mut obs_lines: Vec<String> = Vec::new();
        for q in &queries {
            check_cancel(p)?;
            wait_gate(p, sink, round, 0).await?;
            if p.params.max_searches > 0 && searches_done >= p.params.max_searches {
                obs_lines.push(format!(
                    "已达最大检索次数 {} 次,停止检索",
                    p.params.max_searches
                ));
                break;
            }
            searches_done += 1;
            let real_q = p.params.rewrite_query(q);
            sink.emit(ResearchEvent::Searching {
                round,
                query: q.clone(),
                stage: 0,
            });
            match p.engine.search(&real_q, 5).await {
                Ok(raws) => {
                    let mut news: Vec<SourceItem> = Vec::new();
                    for r in raws {
                        if seen_urls.insert(r.url.clone()) {
                            let item = SourceItem {
                                id: next_id,
                                title: r.title,
                                url: r.url,
                                snippet: r.snippet,
                                engine: p.engine.name().to_string(),
                                published: r.published,
                            };
                            next_id += 1;
                            news.push(item);
                        }
                    }
                    if news.is_empty() {
                        obs_lines.push(format!("查询「{q}」无新结果"));
                    } else {
                        let ids: Vec<String> = news.iter().map(|s| s.id.to_string()).collect();
                        obs_lines.push(format!(
                            "查询「{q}」获得 {} 条新结果 [{}]",
                            news.len(),
                            ids.join(",")
                        ));
                        // 资料注入上下文同样消耗 token,按字符估算计入预算
                        for s in &news {
                            est_tokens += est_tok(&s.snippet);
                        }
                        sources.extend(news.clone());
                        sink.emit(ResearchEvent::Sources {
                            round,
                            sources: news,
                            stage: 0,
                        });
                    }
                }
                Err(e) => {
                    obs_lines.push(format!("查询「{q}」失败: {e}"));
                }
            }
        }

        let observation = obs_lines.join("\n");
        sink.emit(ResearchEvent::Observation {
            round,
            content: observation.clone(),
            stage: 0,
        });
        observations.push(format!("第{round}轮:\n{observation}"));
        let _ = store::update_rounds(&p.id, round as i64);

        // 检索次数预算:达上限提前进入报告阶段
        if p.params.max_searches > 0 && searches_done >= p.params.max_searches {
            break;
        }
        // Token 预算护栏:超预算提前进入报告阶段
        if p.params.token_budget > 0 && est_tokens > p.params.token_budget as u64 {
            sink.emit(ResearchEvent::Observation {
                round,
                content: format!(
                    "已达 Token 预算上限(约 {} tok),提前进入报告阶段",
                    p.params.token_budget
                ),
                stage: 0,
            });
            break;
        }
    }

    // 3. 报告阶段
    check_cancel(p)?;
    wait_gate(p, sink, p.params.max_rounds, 0).await?;
    let report = run_report(p, sink, &sources, "", None).await?;
    Ok(report)
}
