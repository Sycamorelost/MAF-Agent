//! 阶段流水线编排:资料池、阶段进度、检查点闸门与专项研究主流程。

use std::collections::HashSet;
use std::sync::atomic::Ordering;

use super::events::{EventSink, ResearchEvent, SourceItem};
use super::export::run_report;
use super::{stage, store, EngineParams};

mod concise;
mod decide;
mod legacy;
mod staged;
mod synthesis;

pub(crate) use concise::run_concise;
pub(crate) use legacy::run_legacy;

use staged::run_research_stage;
use synthesis::run_synthesis;

pub(crate) enum RunError {
    Cancelled,
    Fatal(String),
}

pub(crate) fn check_cancel(p: &EngineParams) -> Result<(), RunError> {
    if p.cancel.load(Ordering::Relaxed) {
        Err(RunError::Cancelled)
    } else {
        Ok(())
    }
}

/// 全程共享的资料池与预算消耗。
pub(crate) struct Pool {
    pub(crate) sources: Vec<SourceItem>,
    pub(crate) seen_urls: HashSet<String>,
    pub(crate) next_id: u32,
    pub(crate) searches_done: u32,
    pub(crate) est_tokens: u64,
}

impl Pool {
    pub(crate) fn new() -> Self {
        Self {
            sources: Vec::new(),
            seen_urls: HashSet::new(),
            next_id: 1,
            searches_done: 0,
            est_tokens: 0,
        }
    }
}

/// 阶段摘要汇编文本(注入后续阶段决策上下文与终稿模板)。
pub(crate) fn assemble_summaries(summaries: &[(String, String)]) -> String {
    summaries
        .iter()
        .enumerate()
        .map(|(i, (name, s))| format!("### 阶段 {} · {}\n{}", i + 1, name, s))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// M4:把阶段进度(done/total)回写 db 行,列表页零文件 IO。
pub(crate) fn write_stage_progress(id: &str, file: &stage::StagesFile) {
    let total = file.stages.iter().filter(|s| s.enabled).count() as i64;
    let done = file
        .stages
        .iter()
        .filter(|s| s.enabled && s.status == "done")
        .count() as i64;
    let _ = store::update_stage_progress(id, done, total);
}

/// H1:每阶段起点热读 stages.json 的最新编排(检查点「调整下阶段」写回后生效),
/// 运行期状态按 id 保留(含已完成阶段的摘要);返回最新阶段配置列表。
pub(crate) fn hot_reload_stages(
    id: &str,
    file: &mut stage::StagesFile,
) -> Vec<stage::StageConfig> {
    if let Some(disk) = store::read_stages(id) {
        if !disk.configs.is_empty() {
            let mut states: Vec<stage::StageState> = Vec::with_capacity(disk.configs.len());
            for c in &disk.configs {
                let prev = file
                    .stages
                    .iter()
                    .find(|s| s.id == c.id)
                    .or_else(|| disk.stages.iter().find(|s| s.id == c.id));
                let mut s = match prev {
                    Some(s) => s.clone(),
                    None => stage::StageState {
                        id: c.id.clone(),
                        name: c.name.clone(),
                        kind: c.kind.clone(),
                        enabled: c.enabled,
                        status: if c.enabled { "pending".into() } else { "skipped".into() },
                        summary: String::new(),
                        sources_count: 0,
                        rounds: 0,
                        partial: false,
                    },
                };
                // 展示字段跟随最新配置;运行态(状态/摘要/轮次/partial)由引擎持有
                s.name = c.name.clone();
                s.kind = c.kind.clone();
                s.enabled = c.enabled;
                states.push(s);
            }
            file.stages = states;
            file.configs = disk.configs.clone();
            return disk.configs;
        }
    }
    file.configs.clone()
}

/// 专项研究主流程:按阶段流水线执行,每阶段结束写 stages.json 并发 StageDone。
pub(crate) async fn run_staged(p: &EngineParams, sink: &EventSink) -> Result<String, RunError> {
    let mut pool = Pool::new();
    let mut file = stage::StagesFile::from_configs("staged", &p.params.stages);
    let _ = store::save_stages(&p.id, &file);
    write_stage_progress(&p.id, &file);
    let mut summaries: Vec<(String, String)> = Vec::new();
    let mut total_rounds: i64 = 0;
    // 全局时长护栏:起始时刻,任一阶段开始/轮次检查点超限即视同全局耗尽
    let started = std::time::Instant::now();
    let dur_limit = if p.params.max_duration_minutes > 0 {
        Some(std::time::Duration::from_secs(
            p.params.max_duration_minutes as u64 * 60,
        ))
    } else {
        None
    };
    // 全局硬上限耗尽标记(附原因):后续检索阶段跳过,撰写阶段仍可消费既有资料
    let mut global_exhausted: Option<String> = None;

    // H1:阶段编排以 stages.json 为准,每阶段起点热读(检查点「调整下阶段」生效)
    let mut configs: Vec<stage::StageConfig> = p.params.stages.clone();
    let mut idx = 0usize;
    while idx < configs.len() {
        check_cancel(p)?;
        configs = hot_reload_stages(&p.id, &mut file);
        if idx >= configs.len() {
            break; // 调整后剩余阶段被删除
        }
        let st = configs[idx].clone();
        let no = (idx + 1) as u32;
        if !st.enabled {
            file.stages[idx].status = "skipped".into();
            file.updated = chrono::Local::now().to_rfc3339();
            let _ = store::save_stages(&p.id, &file);
            sink.emit(ResearchEvent::StageSkipped {
                stage: no,
                reason: "阶段已停用".into(),
            });
            idx += 1;
            continue;
        }
        // 时长护栏:阶段起点判定
        if global_exhausted.is_none()
            && let Some(lim) = dur_limit
            && started.elapsed() >= lim
        {
            global_exhausted = Some(format!(
                "已达全局时长上限 {} 分钟",
                p.params.max_duration_minutes
            ));
        }
        if let Some(reason) = &global_exhausted {
            if !st.is_synthesis() {
                file.stages[idx].status = "skipped".into();
                file.updated = chrono::Local::now().to_rfc3339();
                let _ = store::save_stages(&p.id, &file);
                sink.emit(ResearchEvent::StageSkipped {
                    stage: no,
                    reason: format!("{reason},跳过剩余检索阶段"),
                });
                idx += 1;
                continue;
            }
        }
        sink.emit(ResearchEvent::StageStart {
            stage: no,
            name: st.name.clone(),
            kind: st.kind.clone(),
        });
        file.stages[idx].status = "running".into();
        file.updated = chrono::Local::now().to_rfc3339();
        let _ = store::update_stage(&p.id, no as i64);
        let _ = store::save_stages(&p.id, &file);

        let before = pool.sources.len();
        let mut partial = false;
        let (summary, rounds) = if st.is_synthesis() {
            (run_synthesis(p, &st, &mut pool, &summaries).await?, 0)
        } else {
            // URL 去重范围 = 仅阶段内:每阶段开始清空已见集合
            if p.params.dedup_scope == "stage" {
                pool.seen_urls.clear();
            }
            let r =
                run_research_stage(p, sink, no, &st, &mut pool, &summaries, total_rounds, &started)
                    .await?;
            total_rounds += r.1 as i64;
            partial = r.2;
            // 全局预算耗尽判定(供后续阶段跳过)
            if global_exhausted.is_none() {
                if p.params.max_searches > 0 && pool.searches_done >= p.params.max_searches {
                    global_exhausted =
                        Some(format!("已达全局检索次数上限 {} 次", p.params.max_searches));
                } else if p.params.token_budget > 0
                    && pool.est_tokens > p.params.token_budget as u64
                {
                    global_exhausted = Some(format!(
                        "已达全局 Token 预算上限(约 {} tok)",
                        p.params.token_budget
                    ));
                } else if p.params.max_sources > 0
                    && pool.sources.len() >= p.params.max_sources as usize
                {
                    global_exhausted =
                        Some(format!("已达全局资料上限 {} 条", p.params.max_sources));
                } else if dur_limit.is_some_and(|lim| started.elapsed() >= lim) {
                    // H2:检索阶段内跨时长上限,立即登记耗尽原因(不再等下一阶段起点)
                    global_exhausted = Some(format!(
                        "已达全局时长上限 {} 分钟",
                        p.params.max_duration_minutes
                    ));
                }
            }
            (r.0, r.1)
        };

        let count = (pool.sources.len() - before) as u32;
        file.stages[idx].status = "done".into();
        file.stages[idx].summary = summary.clone();
        file.stages[idx].sources_count = count;
        file.stages[idx].rounds = rounds;
        file.stages[idx].partial = partial;
        file.updated = chrono::Local::now().to_rfc3339();
        let _ = store::save_stages(&p.id, &file);
        write_stage_progress(&p.id, &file);
        sink.emit(ResearchEvent::StageDone {
            stage: no,
            summary: summary.clone(),
            sources_count: count,
            partial,
        });
        summaries.push((st.name.clone(), summary));
        // 检查点模式:非末个启用阶段结束时,发 Checkpoint 并挂起等待确认
        if p.params.checkpoint
            && let Some(next) = configs[idx + 1..].iter().find(|s| s.enabled)
        {
            sink.emit(ResearchEvent::Checkpoint {
                stage: no,
                next: next.name.clone(),
            });
            p.paused.store(true, Ordering::Relaxed);
            wait_gate(p, sink, 0, no).await?;
        }
        idx += 1;
    }

    // 终稿:注入阶段摘要汇编
    check_cancel(p)?;
    wait_gate(p, sink, 0, 0).await?;
    // H2:终稿前再判一次时长(最后一个检索阶段内超时时,耗尽原因可能尚未登记)
    if global_exhausted.is_none()
        && let Some(lim) = dur_limit
        && started.elapsed() >= lim
    {
        global_exhausted = Some(format!(
            "已达全局时长上限 {} 分钟",
            p.params.max_duration_minutes
        ));
    }
    // H2/M1:护栏收尾向读者与过程观者明示
    if let Some(reason) = &global_exhausted {
        sink.emit(ResearchEvent::Observation {
            round: 0,
            content: format!("{reason},基于已收集资料生成收尾报告,资料覆盖可能不完整"),
            stage: 0,
        });
    }
    let stages_digest = assemble_summaries(&summaries);
    run_report(
        p,
        sink,
        &pool.sources,
        &stages_digest,
        global_exhausted.as_deref(),
    )
    .await
}

/// 暂停闸门:paused 置位时通知前端并异步轮询等待,直到恢复或取消。
/// M-5:改用 tokio 异步睡眠,不再用 std::thread::sleep 霸占 tokio worker 线程。
pub(crate) async fn wait_gate(
    p: &EngineParams,
    sink: &EventSink,
    round: u32,
    stage: u32,
) -> Result<(), RunError> {
    if !p.paused.load(Ordering::Relaxed) {
        return Ok(());
    }
    sink.emit(ResearchEvent::Paused { round, stage });
    let _ = store::update_status(&p.id, "paused", None);
    loop {
        if p.cancel.load(Ordering::Relaxed) {
            return Err(RunError::Cancelled);
        }
        if !p.paused.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let _ = store::update_status(&p.id, "running", None);
    sink.emit(ResearchEvent::Resumed { round, stage });
    Ok(())
}
