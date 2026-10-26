//! 单个检索阶段的 ReAct 内层循环。

use super::super::EngineParams;
use super::super::events::{EventSink, ResearchEvent, SourceItem};
use super::super::llm::est_tok;
use super::super::prompts::{DecideCtx, sources_digest};
use super::super::skills;
use super::super::stage;
use super::super::store;
use super::super::validate::{Decision, verify_angles};
use super::decide::decide;
use super::synthesis::run_stage_summary;
use super::{Pool, RunError, assemble_summaries, check_cancel, wait_gate};

/// 单个检索阶段:ReAct 内层循环 + 阶段内约束护栏 + min_sources 补轮 + 角度校验 + 阶段摘要。
/// 返回 (阶段摘要, 执行轮次, partial);partial=true 表示因失败策略提前中止(M2)。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_research_stage(
    p: &EngineParams,
    sink: &EventSink,
    no: u32,
    st: &stage::StageConfig,
    pool: &mut Pool,
    summaries: &[(String, String)],
    rounds_base: i64,
    started: &std::time::Instant,
) -> Result<(String, u32, bool), RunError> {
    let eff = st.effective(&p.params);
    let stage_start = pool.sources.len();
    let searches0 = pool.searches_done;
    let tok0 = pool.est_tokens;
    let mut observations: Vec<String> = Vec::new();
    let mut planned: Option<Vec<String>> = None;
    let mut last_queries: Vec<String> = Vec::new();
    let mut round = 0u32;
    let mut extra_used = 0u32;
    // 失败策略 = 中止本阶段(M2):循环内命中置位,尾部据此标记 partial
    let mut abort_stage = false;
    // 时长护栏(与 run_staged 同一起点)
    let dur_limit = if p.params.max_duration_minutes > 0 {
        Some(std::time::Duration::from_secs(
            p.params.max_duration_minutes as u64 * 60,
        ))
    } else {
        None
    };

    let prev_text = if eff.inject_prev_summaries {
        assemble_summaries(summaries)
    } else {
        String::new()
    };

    loop {
        // 轮次控制:正常轮次用尽后,min_sources 未达标自动补轮(最多 2 轮)
        if round >= eff.max_rounds {
            let got = pool.sources.len() - stage_start;
            if eff.min_sources > 0 && got < eff.min_sources as usize && extra_used < 2 {
                extra_used += 1;
            } else {
                break;
            }
        }
        // 时长护栏:超限阶段收尾(全局在下一阶段起点统一标记耗尽)
        if let Some(lim) = dur_limit
            && started.elapsed() >= lim
        {
            sink.emit(ResearchEvent::Observation {
                round,
                content: format!(
                    "已达全局时长上限 {} 分钟,阶段收尾",
                    p.params.max_duration_minutes
                ),
                stage: no,
            });
            break;
        }
        round += 1;
        check_cancel(p)?;
        wait_gate(p, sink, round, no).await?;
        sink.emit(ResearchEvent::RoundStart {
            round,
            max_rounds: eff.max_rounds,
            stage: no,
        });

        // 决策上下文:本阶段资料明细 +(可选)前阶段资料
        let inherit_digest = if eff.inherit_sources && stage_start > 0 {
            sources_digest(&pool.sources[..stage_start], 200, 4000)
        } else {
            String::new()
        };
        let ctx = DecideCtx {
            stage_no: no,
            name: &st.name,
            goal: &st.goal,
            max_rounds: eff.max_rounds,
            queries_per_round: eff.queries_per_round,
            reflect: &eff.reflect,
            read_length: &eff.read_length,
            snippet_chars: if eff.read_length == "full" { 1200 } else { 240 },
            decide_template: st
                .decide_prompt
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .unwrap_or_else(|| p.params.decide_template()),
            early_stop: eff.early_stop_on_enough,
            prev_summaries: &prev_text,
            inherit_digest: &inherit_digest,
            must_cover_angles: &eff.must_cover_angles,
            query_language: &eff.query_language,
            skill_query: skills::fragments(&p.params).query,
        };
        // H2:决策调用前再判一次时长(单次 LLM 调用内部不可中断,只能在调用边界补足粒度)
        if let Some(lim) = dur_limit
            && started.elapsed() >= lim
        {
            sink.emit(ResearchEvent::Observation {
                round,
                content: format!(
                    "已达全局时长上限 {} 分钟,阶段收尾",
                    p.params.max_duration_minutes
                ),
                stage: no,
            });
            break;
        }
        let decision = decide(
            p,
            sink,
            &ctx,
            round,
            &pool.sources[stage_start..],
            &observations,
            &mut planned,
            &mut last_queries,
            &mut pool.est_tokens,
        )
        .await?;

        let queries = match decision {
            Decision::Finish => break,
            Decision::Search(qs) => qs,
        };
        sink.emit(ResearchEvent::Queries {
            round,
            queries: queries.clone(),
            stage: no,
        });

        let mut obs_lines: Vec<String> = Vec::new();
        // 本轮待执行查询:先按全局/阶段检索上限与全局资料上限过滤
        let mut todo: Vec<&String> = Vec::new();
        for q in &queries {
            if p.params.max_searches > 0
                && pool.searches_done + todo.len() as u32 >= p.params.max_searches
            {
                obs_lines.push(format!(
                    "已达全局最大检索次数 {} 次,停止检索",
                    p.params.max_searches
                ));
                break;
            }
            if eff.max_searches > 0
                && pool.searches_done + todo.len() as u32 - searches0 >= eff.max_searches
            {
                obs_lines.push(format!(
                    "已达本阶段检索次数上限 {} 次,停止检索",
                    eff.max_searches
                ));
                break;
            }
            if p.params.max_sources > 0 && pool.sources.len() >= p.params.max_sources as usize {
                obs_lines.push(format!(
                    "已达全局资料上限 {} 条,停止检索",
                    p.params.max_sources
                ));
                break;
            }
            todo.push(q);
        }
        pool.searches_done += todo.len() as u32;
        // 并发执行(M3):buffer_unordered 按完成顺序产出结果,失败策略即时判定短路;
        // abort_research / abort_stage 时直接 drop 流,不再等待剩余在途 future 的结果。
        let conc = p.params.concurrent_queries.clamp(1, 5) as usize;
        let fail_policy = p.params.fail_policy.as_str();
        check_cancel(p)?;
        wait_gate(p, sink, round, no).await?;
        let real_qs: Vec<String> = todo
            .iter()
            .map(|q| stage::StageConfig::rewrite_query_eff(&eff, q))
            .collect();
        // 先整批 emit Searching(本轮查询全部下发)
        for q in &todo {
            sink.emit(ResearchEvent::Searching {
                round,
                query: (*q).clone(),
                stage: no,
            });
        }
        {
            use futures::StreamExt;
            // 显式循环构建 future 列表(避免闭包返回 async 块触发的生命周期推断问题)
            let mut futs = Vec::with_capacity(todo.len());
            for (q, rq) in todo.iter().zip(real_qs.iter()) {
                let q_owned = (*q).clone();
                let engine = &p.engine;
                futs.push(async move {
                    let res = engine.search(rq, 5).await;
                    (q_owned, res)
                });
            }
            let mut stream = futures::stream::iter(futs).buffer_unordered(conc.max(1));
            while let Some((q, res)) = stream.next().await {
                check_cancel(p)?;
                match res {
                    Ok(raws) => {
                        let mut news: Vec<SourceItem> = Vec::new();
                        for r in raws {
                            // 域名黑名单过滤
                            if stage::domain_blocked(&r.url, &eff.exclude_domains) {
                                continue;
                            }
                            // 全局资料上限 + 阶段新增资料上限
                            if p.params.max_sources > 0
                                && pool.sources.len() + news.len() >= p.params.max_sources as usize
                            {
                                break;
                            }
                            if eff.max_sources > 0
                                && (pool.sources.len() - stage_start + news.len())
                                    >= eff.max_sources as usize
                            {
                                break;
                            }
                            if pool.seen_urls.insert(r.url.clone()) {
                                let item = SourceItem {
                                    id: pool.next_id,
                                    title: r.title,
                                    url: r.url,
                                    snippet: r.snippet,
                                    engine: p.engine.name().to_string(),
                                    published: r.published,
                                };
                                pool.next_id += 1;
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
                            for s in &news {
                                pool.est_tokens += est_tok(&s.snippet);
                            }
                            pool.sources.extend(news.clone());
                            sink.emit(ResearchEvent::Sources {
                                round,
                                sources: news,
                                stage: no,
                            });
                        }
                    }
                    Err(e) => match fail_policy {
                        "abort_stage" => {
                            obs_lines.push(format!("查询「{q}」失败: {e}(按失败策略中止本阶段)"));
                            abort_stage = true;
                        }
                        "abort_research" => {
                            return Err(RunError::Fatal(format!(
                                "查询「{q}」失败: {e}(按失败策略中止研究)"
                            )));
                        }
                        _ => {
                            obs_lines.push(format!("查询「{q}」失败: {e}"));
                        }
                    },
                }
                if abort_stage {
                    break;
                }
            }
        }

        let observation = obs_lines.join("\n");
        sink.emit(ResearchEvent::Observation {
            round,
            content: observation.clone(),
            stage: no,
        });
        observations.push(format!("第{round}轮:\n{observation}"));
        let _ = store::update_rounds(&p.id, rounds_base + round as i64);

        // 失败策略 = 中止本阶段:不再进入下一轮
        if abort_stage {
            break;
        }
        // 阶段 Token 预算护栏
        if eff.token_budget > 0 && pool.est_tokens - tok0 > eff.token_budget as u64 {
            sink.emit(ResearchEvent::Observation {
                round,
                content: format!(
                    "已达本阶段 Token 预算上限(约 {} tok),阶段收尾",
                    eff.token_budget
                ),
                stage: no,
            });
            break;
        }
        // 全局 Token 预算护栏
        if p.params.token_budget > 0 && pool.est_tokens > p.params.token_budget as u64 {
            sink.emit(ResearchEvent::Observation {
                round,
                content: format!(
                    "已达全局 Token 预算上限(约 {} tok),阶段收尾",
                    p.params.token_budget
                ),
                stage: no,
            });
            break;
        }
    }

    let rounds_done = round;
    // 必覆盖角度校验(提醒不阻断):角度词在本阶段资料与观察中均未出现 → 缺失
    let missing = verify_angles(
        &eff.must_cover_angles,
        &pool.sources[stage_start..],
        &observations,
    );
    if !missing.is_empty() {
        sink.emit(ResearchEvent::AngleGap {
            stage: no,
            missing: missing.clone(),
        });
    }
    // 资料下限缺口标注
    let got = pool.sources.len() - stage_start;
    let gap_note = if eff.min_sources > 0 && got < eff.min_sources as usize {
        Some(format!(
            "本阶段资料 {} 条,未达下限 {} 条(已补 {} 轮检索)",
            got, eff.min_sources, extra_used
        ))
    } else {
        None
    };

    // H2:摘要前再判时长——超限跳过 LLM 摘要,用 fallback 文本收尾
    let dur_hit = dur_limit.is_some_and(|lim| started.elapsed() >= lim);
    let mut summary = if dur_hit {
        sink.emit(ResearchEvent::Observation {
            round: rounds_done,
            content: format!(
                "已达全局时长上限 {} 分钟,跳过摘要生成",
                p.params.max_duration_minutes
            ),
            stage: no,
        });
        format!(
            "(已达全局时长上限,跳过摘要生成;本阶段共收集 {} 条资料)",
            pool.sources.len() - stage_start
        )
    } else {
        run_stage_summary(
            p,
            st,
            &eff,
            pool,
            stage_start,
            &observations,
            &missing,
            gap_note.as_deref(),
        )
        .await?
    };
    // M2:失败策略提前中止 → 摘要加前缀,StageDone 标记 partial
    if abort_stage {
        summary = format!("(因失败策略提前中止){summary}");
    }
    Ok((summary, rounds_done, abort_stage))
}
