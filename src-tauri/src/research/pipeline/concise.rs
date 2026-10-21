//! 简洁研究(concise):单对话多轮迭代。

use std::collections::HashSet;

use crate::provider::{ChatMessage, ChatRequest, StreamChunk};

use super::super::events::{EventSink, ResearchEvent, SourceItem};
use super::super::export::{finalize_report, run_audit};
use super::super::llm::{chat_stream_compat, est_tok, llm_collect};
use super::super::params::with_references;
use super::super::prompts::sources_digest;
use super::super::skills;
use super::super::store;
use super::super::validate::strip_list_marker;
use super::super::EngineParams;
use super::{check_cancel, wait_gate, RunError};

/// 简洁模式轮次上限(防失控;每轮 = 1 次整稿输出 + 1 组轻量检索)。
const CONCISE_MAX_ROUNDS: u32 = 10;
/// 每轮轻量检索的查询数上限。
const CONCISE_QUERIES_PER_ROUND: u32 = 3;
/// 上一稿注入自评/重写上下文的最大字符数。
const CONCISE_PREV_CHARS: usize = 24_000;

const CONCISE_EVAL_SYSTEM: &str = r#"你是简洁研究迭代的自评器。给定研究主题、上一稿报告与用户参考资料,请自评上一稿的不足,并给出本轮补充检索的查询词。严格按以下格式输出:

不足:
- (列出上一稿的 2-4 条主要不足:证据缺口 / 过时表述 / 结构问题)
查询:
- (给出 2-3 个能补足上述缺口的检索查询词,每行一个)

不要输出其他内容。"#;

/// 从自评输出解析「查询:」小节的查询词;无明确小节标记时回退为全文行解析。
fn concise_queries(text: &str, max: u32) -> Vec<String> {
    let mut in_q = false;
    let mut out: Vec<String> = Vec::new();
    let mut fallback: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        let low = t.to_lowercase();
        if t.starts_with("查询") || low.starts_with("queries") {
            in_q = true;
            continue;
        }
        if t.starts_with("不足") || low.starts_with("gaps") || t.starts_with('#') {
            in_q = false;
            continue;
        }
        if let Some(q) = strip_list_marker(t) {
            if q.is_empty() {
                continue;
            }
            fallback.push(q.clone());
            if in_q {
                out.push(q);
                if out.len() >= max as usize {
                    break;
                }
            }
        }
    }
    if out.is_empty() {
        fallback.into_iter().take(max as usize).collect()
    } else {
        out
    }
}

/// 简洁研究:单对话多轮迭代。
/// 第 1 轮基于主题(含用户参考资料注入)输出完整初稿;
/// 第 2..N 轮 = 自评上一稿(Thought)→ 轻量检索(Sources/Observation)→ 整稿重写(ReportDelta 流式)。
/// 复用 legacy 的搜索去重 / 取消暂停 / 事件落盘;token 预算 / 检索次数 / 总时长照常生效;
/// 各轮稿件以「稿件存档」观察节点留在时间线可回看;最终稿经 finalize_report 统一引用列表。
pub(crate) async fn run_concise(
    p: &EngineParams,
    sink: &EventSink,
) -> Result<String, RunError> {
    use std::time::Instant;

    let rounds = p.params.max_rounds.clamp(1, CONCISE_MAX_ROUNDS);
    let started = Instant::now();
    let mut sources: Vec<SourceItem> = Vec::new();
    let mut seen_urls: HashSet<String> = HashSet::new();
    let mut next_id: u32 = 1;
    let mut searches_done: u32 = 0;
    let mut est_tokens: u64 = 0;
    let mut draft = String::new();

    for round in 1..=rounds {
        check_cancel(p)?;
        wait_gate(p, sink, round, 0).await?;
        sink.emit(ResearchEvent::RoundStart {
            round,
            max_rounds: rounds,
            stage: 0,
        });

        // 全局护栏(第 1 轮不检查,保证至少产出一稿):
        // 先存档上一稿,再检查时长 / token 预算 / 检索次数是否耗尽 → 提前收尾
        if round > 1 {
            // 上一轮稿件存档:整稿留在时间线观察节点(配合可调高度卡回看)
            sink.emit(ResearchEvent::Observation {
                round: round - 1,
                content: format!("【第 {} 轮稿件存档】\n{}", round - 1, draft),
                stage: 0,
            });

            let mut guard: Option<String> = None;
            if p.params.max_duration_minutes > 0
                && started.elapsed().as_secs() > p.params.max_duration_minutes as u64 * 60
            {
                guard = Some(format!("已达最大时长 {} 分钟", p.params.max_duration_minutes));
            } else if p.params.token_budget > 0 && est_tokens > p.params.token_budget as u64 {
                guard = Some(format!("已达 Token 预算上限(约 {} tok)", p.params.token_budget));
            } else if p.params.max_searches > 0 && searches_done >= p.params.max_searches {
                guard = Some(format!("已达最大检索次数 {} 次", p.params.max_searches));
            }
            if let Some(reason) = guard {
                sink.emit(ResearchEvent::Observation {
                    round,
                    content: format!("{reason},提前收尾,以当前稿为准"),
                    stage: 0,
                });
                break;
            }
        }

        if round > 1 {
            // 1. 自评上一稿 → 本轮补检查询
            let eval_user = format!(
                "研究主题: {}\n\n【上一稿报告】\n{}\n\n请自评上一稿不足并给出本轮检索查询词。",
                p.topic,
                draft.chars().take(CONCISE_PREV_CHARS).collect::<String>()
            );
            let (eval_text, usage) = llm_collect(
                p,
                vec![
                    ChatMessage {
                        role: "system".into(),
                        content: CONCISE_EVAL_SYSTEM.into(),
                        images: vec![],
                    },
                    ChatMessage {
                        role: "user".into(),
                        content: eval_user,
                        images: vec![],
                    },
                ],
            )
            .await?;
            est_tokens += usage.unwrap_or_else(|| est_tok(&eval_text));
            sink.emit(ResearchEvent::Thought {
                round,
                content: eval_text.trim().to_string(),
                stage: 0,
            });

            // 2. 轻量检索(沿用 legacy 的去重 / 编号 / 事件;每轮 ≤3 查询)
            let queries = concise_queries(&eval_text, CONCISE_QUERIES_PER_ROUND);
            if queries.is_empty() {
                sink.emit(ResearchEvent::Observation {
                    round,
                    content: "自评未给出查询,跳过检索直接修订".to_string(),
                    stage: 0,
                });
            } else {
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
                                let ids: Vec<String> =
                                    news.iter().map(|s| s.id.to_string()).collect();
                                obs_lines.push(format!(
                                    "查询「{q}」获得 {} 条新结果 [{}]",
                                    news.len(),
                                    ids.join(",")
                                ));
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
                        Err(e) => obs_lines.push(format!("查询「{q}」失败: {e}")),
                    }
                }
                sink.emit(ResearchEvent::Observation {
                    round,
                    content: obs_lines.join("\n"),
                    stage: 0,
                });
            }
        }

        // 3. 整稿输出(第 1 轮初稿 / 第 >=2 轮修订稿;ReportDelta 流式推报告区)
        draft = concise_write(p, sink, round, rounds, &draft, &sources).await?;
        let _ = store::update_rounds(&p.id, round as i64);
    }

    if draft.trim().is_empty() {
        return Err(RunError::Fatal("简洁研究未产出报告".into()));
    }
    // 审计技能:仅终稿执行一次(fragments.audit 在 run_report 内逐次调用,这里手动补一次)
    let ref_anchor = !p.params.free_structure && p.params.report_template().contains("{references}");
    let mut report = finalize_report(&draft, &sources, ref_anchor);
    let frags = skills::fragments(&p.params);
    if let Some(audit_sys) = frags.audit {
        if let Ok(appendix) = run_audit(p, audit_sys, &report).await {
            if !appendix.trim().is_empty() {
                report.push_str("\n\n");
                report.push_str(appendix.trim());
            }
        }
    }
    Ok(report)
}

/// 简洁模式整稿输出:复用终稿模板(引用纪律 / 语言 / 写作技能) + 简洁修订指令,
/// 流式推 ReportDelta;返回未 finalize 的正文(全部轮次完成后统一 finalize)。
async fn concise_write(
    p: &EngineParams,
    sink: &EventSink,
    round: u32,
    rounds: u32,
    prev: &str,
    sources: &[SourceItem],
) -> Result<String, RunError> {
    use futures::StreamExt;

    let (mut system, _) = p.params.report_system(&p.topic, "");
    system.push_str(&format!("\n\n【简洁研究模式 · 第 {round}/{rounds} 轮】"));
    if round == 1 {
        system.push_str("这是第 1 轮:基于你的既有知识与用户参考资料输出完整初稿;资料列表可能为空,此时不得编造 [n] 引用,后续轮次会补充检索证据。");
    } else {
        system.push_str("这是迭代修订轮:基于「上一稿 + 新检索资料」对整篇报告修订重写——补充新证据、修正错误与过时表述、删去无依据内容;输出修订后的完整报告(不是增量补丁),保留 [n] 引用标记并与资料列表编号对应。");
    }
    // 科研增强:报告写作纪律(单次装配,关闭时为空)
    let frags = skills::fragments(&p.params);
    if !frags.report.is_empty() {
        system.push_str("\n\n");
        system.push_str(&frags.report);
    }
    // 用户导入的参考资料:整稿撰写的事实依据
    let system = with_references(system, p);

    let max_total = if p.params.read_length == "full" { 24000 } else { 16000 };
    let mut user = format!("研究主题: {}\n\n", p.topic);
    if round > 1 && !prev.trim().is_empty() {
        user.push_str("【上一稿报告】\n");
        user.push_str(&prev.chars().take(CONCISE_PREV_CHARS).collect::<String>());
        user.push_str("\n\n");
    }
    user.push_str(&format!(
        "【资料列表】\n{}\n请输出完整研究报告。",
        sources_digest(sources, p.params.snippet_chars().max(400), max_total)
    ));

    let req = ChatRequest {
        model: p.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: system,
                images: vec![],
            },
            ChatMessage {
                role: "user".into(),
                content: user,
                images: vec![],
            },
        ],
        temperature: Some(0.3),
        max_tokens: None,
        top_p: None,
        reasoning: None, // 研究链路档位透传为 Phase 2,暂不启用
        stream: true,
    };
    let mut stream = chat_stream_compat(p, req).await?;
    let mut out = String::new();
    while let Some(chunk) = stream.next().await {
        check_cancel(p)?;
        match chunk {
            StreamChunk::Delta { content } => {
                out.push_str(&content);
                sink.emit(ResearchEvent::ReportDelta { content });
            }
            StreamChunk::Reasoning { .. } => {} // 研究链路只取正文,忽略思考流
            StreamChunk::Error { message } => return Err(RunError::Fatal(message)),
            StreamChunk::Usage { .. } => {}
            StreamChunk::Done { .. } => break,
        }
    }
    let out = out.trim().to_string();
    if out.is_empty() {
        return Err(RunError::Fatal(format!("第 {round} 轮整稿输出为空")));
    }
    Ok(out)
}
