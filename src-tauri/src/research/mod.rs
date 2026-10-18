//! 深度研究引擎(Phase 2):多轮搜索 + ReAct 循环 + 引用列表。
//!
//! 流程对齐 idea/02-features.md §3:
//!   1. ReAct 循环:Thought → Action(search / finish)→ Queries → 搜索 → Observation
//!   2. 达到 finish 或 max_rounds 后,基于收集的资料流式生成带 [n] 引用的研究报告
//!   3. 过程事件实时推给前端(Channel),同时落盘 trail.jsonl;报告落盘 report.md

pub mod blueprint;
pub mod preset;
pub mod search;
pub mod skills;
pub mod stage;
pub mod store;
pub mod template;

use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

use crate::provider::{ChatMessage, ChatRequest, ChatStream, Provider, StreamChunk};
use search::SearchEngine;

// ---------------- 类型 ----------------

/// 一条已分配编号的引用来源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceItem {
    pub id: u32,
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub engine: String,
    pub published: Option<String>,
}

/// 研究过程事件(推前端 + 落 trail.jsonl)。
///
/// 专项研究(步骤 2):轮次级事件新增 `stage` 字段(serde default 0,兼容旧 trail 回放);
/// 新增阶段级事件 StageStart / StageDone / StageSkipped / AngleGap / Checkpoint。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResearchEvent {
    RoundStart {
        round: u32,
        max_rounds: u32,
        #[serde(default)]
        stage: u32,
    },
    Thought {
        round: u32,
        content: String,
        #[serde(default)]
        stage: u32,
    },
    Queries {
        round: u32,
        queries: Vec<String>,
        #[serde(default)]
        stage: u32,
    },
    Searching {
        round: u32,
        query: String,
        #[serde(default)]
        stage: u32,
    },
    Sources {
        round: u32,
        sources: Vec<SourceItem>,
        #[serde(default)]
        stage: u32,
    },
    Observation {
        round: u32,
        content: String,
        #[serde(default)]
        stage: u32,
    },
    Paused {
        round: u32,
        #[serde(default)]
        stage: u32,
    },
    Resumed {
        round: u32,
        #[serde(default)]
        stage: u32,
    },
    /// 阶段开始(专项模式)
    StageStart { stage: u32, name: String, kind: String },
    /// 阶段完成,附阶段摘要与新增资料数;partial=true 表示因失败策略提前中止(M2,default 兼容旧 trail)
    StageDone {
        stage: u32,
        summary: String,
        sources_count: u32,
        #[serde(default)]
        partial: bool,
    },
    /// 阶段跳过(停用 / 全局约束耗尽)
    StageSkipped { stage: u32, reason: String },
    /// 必覆盖角度缺口提醒(校验为提醒,不阻断)
    AngleGap { stage: u32, missing: Vec<String> },
    /// 检查点:阶段结束暂停等待确认(步骤 3 启用暂停逻辑,本步仅协议)
    Checkpoint { stage: u32, next: String },
    ReportDelta { content: String },
    Done { report: String },
    Cancelled,
    Error { message: String },
}

/// 深度研究可配置参数(迭代 D4)。
///
/// 与前端 ResearchPage 配置栏一一对应;同时是研究预设(presets.json)的载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ResearchParams {
    /// 研究深度:ReAct 循环最大轮次(1-25)
    pub max_rounds: u32,
    /// 每轮查询数(1-10)
    pub queries_per_round: u32,
    /// 信息源模式:web(全网)/ academic(学术优先)/ whitelist(域名白名单)
    pub source_mode: String,
    /// source_mode = whitelist 时生效的域名列表
    pub whitelist_domains: Vec<String>,
    /// 单源阅读长度:digest(摘要 240 字)/ full(扩展 1200 字)
    pub read_length: String,
    /// 反思策略:every(每轮反思)/ every3(每 3 轮)/ final(仅开头规划)
    pub reflect: String,
    /// 最大检索次数(0 = 不限)
    pub max_searches: u32,
    /// Token 预算上限,按字符估算(0 = 不限)
    pub token_budget: u32,
    /// 输出语言:zh / en
    pub output_lang: String,
    /// 专项研究:全局决策提示覆盖(None = templates.json 自定义 > 内置)
    #[serde(default)]
    pub decide_prompt: Option<String>,
    /// 专项研究:终稿报告模板覆盖(None = templates.json 自定义 > 内置)
    #[serde(default)]
    pub report_prompt: Option<String>,
    /// 研究模式:legacy(快速研究,单层 ReAct)/ staged(专项研究,阶段流水线)
    #[serde(default = "default_mode")]
    pub mode: String,
    /// 专项模式阶段编排(mode=staged 时生效;L4:legacy 模式忽略,后端仅做归一化校验)
    #[serde(default)]
    pub stages: Vec<stage::StageConfig>,
    /// 检查点模式:每阶段结束暂停等用户确认(L4:staged 专属;legacy 模式忽略,后端仅透传)
    #[serde(default)]
    pub checkpoint: bool,
    /// 全局总时长上限(分钟,0 = 不限;超限视同预算耗尽,撰写阶段与终稿仍执行)
    #[serde(default)]
    pub max_duration_minutes: u32,
    /// 全局资料条数上限(0 = 不限)
    #[serde(default)]
    pub max_sources: u32,
    /// 同轮查询并发数 1-5(默认 1 顺序执行;429 风险场景保持 1)
    /// (L4:staged 专属;legacy 模式忽略,后端仅 clamp)
    #[serde(default = "default_concurrent_queries")]
    pub concurrent_queries: u32,
    /// 单查询失败策略:skip 跳过继续 / abort_stage 中止本阶段 / abort_research 中止研究
    /// (L4:staged 专属;legacy 模式忽略,非法值回退 skip)
    #[serde(default = "default_fail_policy")]
    pub fail_policy: String,
    /// URL 去重范围:global 全程去重 / stage 仅阶段内去重(允许跨阶段复检同一来源)
    /// (L4:staged 专属;legacy 模式忽略,非法值回退 global)
    #[serde(default = "default_dedup_scope")]
    pub dedup_scope: String,
    /// 科研增强(v1.9.0):注入内置研究方法论技能(默认开,轻量片段)
    #[serde(default = "default_true")]
    pub skill_enhance: bool,
    /// 深度审计:终稿追加引用与统计审计附录(默认关;skill_enhance 开启时生效)
    #[serde(default)]
    pub skill_audit: bool,
    /// AI 概念示意草稿:终稿追加一张生图草稿并标注(默认关;skill_enhance 开启时生效)
    #[serde(default)]
    pub skill_image_draft: bool,
    /// 草稿生图 Provider id(skill_image_draft=true 时必填)
    #[serde(default)]
    pub skill_image_provider: Option<String>,
    /// 草稿生图模型名(skill_image_draft=true 时必填)
    #[serde(default)]
    pub skill_image_model: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_concurrent_queries() -> u32 {
    1
}

fn default_fail_policy() -> String {
    "skip".into()
}

fn default_dedup_scope() -> String {
    "global".into()
}

fn default_mode() -> String {
    "legacy".into()
}

impl Default for ResearchParams {
    fn default() -> Self {
        Self {
            max_rounds: 8,
            queries_per_round: 4,
            source_mode: "web".into(),
            whitelist_domains: Vec::new(),
            read_length: "digest".into(),
            reflect: "every".into(),
            max_searches: 0,
            token_budget: 120_000,
            output_lang: "zh".into(),
            decide_prompt: None,
            report_prompt: None,
            mode: default_mode(),
            stages: Vec::new(),
            checkpoint: false,
            max_duration_minutes: 0,
            max_sources: 0,
            concurrent_queries: 1,
            fail_policy: default_fail_policy(),
            dedup_scope: default_dedup_scope(),
            skill_enhance: true,
            skill_audit: false,
            skill_image_draft: false,
            skill_image_provider: None,
            skill_image_model: None,
        }
    }
}

impl ResearchParams {
    /// 单源摘要注入的最大字符数(「单源阅读长度」参数)。
    pub fn snippet_chars(&self) -> usize {
        if self.read_length == "full" {
            1200
        } else {
            240
        }
    }

    /// 把用户的原始查询按信息源模式改写为实际下发的查询词。
    fn rewrite_query(&self, q: &str) -> String {
        match self.source_mode.as_str() {
            "academic" => format!("{q} 学术 论文 研究"),
            "whitelist" if !self.whitelist_domains.is_empty() => {
                let sites = self
                    .whitelist_domains
                    .iter()
                    .map(|d| format!("site:{d}"))
                    .collect::<Vec<_>>()
                    .join(" OR ");
                format!("{q} ({sites})")
            }
            _ => q.to_string(),
        }
    }

    /// 生效决策模板:逐次覆盖(decide_prompt)> templates.json 自定义 > 内置。
    pub fn decide_template(&self) -> String {
        self.decide_prompt
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or_else(|| template::effective("decide", &self.output_lang))
    }

    /// 生效终稿模板:逐次覆盖(report_prompt)> templates.json 自定义 > 内置。
    pub fn report_template(&self) -> String {
        self.report_prompt
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or_else(|| template::effective("report", &self.output_lang))
    }
}

/// 引擎运行参数。
pub struct EngineParams {
    pub id: String,
    pub topic: String,
    pub provider: Box<dyn Provider>,
    pub model: String,
    pub engine: Box<dyn SearchEngine>,
    pub params: ResearchParams,
    pub cancel: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
    pub channel: Channel<ResearchEvent>,
    /// H3:stream_options 被中转站拒绝时,无该字段重建 Provider 所需的入参
    pub rebuild: Option<ProviderRebuild>,
    /// H3:回退重建后的 Provider(重建一次,后续 LLM 调用复用)
    pub fallback: std::sync::Mutex<Option<Arc<dyn Provider>>>,
}

/// H3:重建 Provider(关闭 stream_options)所需的启动入参快照。
pub struct ProviderRebuild {
    pub provider_id: String,
    pub model: String,
    pub custom_base_url: Option<String>,
    pub custom_display_name: Option<String>,
}

/// 事件出口:同时落盘 + 推前端。发送失败不中断研究(便于事后查 trail)。
struct EventSink {
    id: String,
    channel: Channel<ResearchEvent>,
}

impl EventSink {
    fn emit(&self, ev: ResearchEvent) {
        let _ = store::append_trail(&self.id, &ev);
        let _ = self.channel.send(ev);
    }
}

enum RunError {
    Cancelled,
    Fatal(String),
}

fn check_cancel(p: &EngineParams) -> Result<(), RunError> {
    if p.cancel.load(Ordering::Relaxed) {
        Err(RunError::Cancelled)
    } else {
        Ok(())
    }
}

// ---------------- 主入口 ----------------

/// 运行一次研究(在后台任务中被 spawn)。负责终态状态落库。
pub async fn run(p: EngineParams) {
    let sink = EventSink {
        id: p.id.clone(),
        channel: p.channel.clone(),
    };
    match run_inner(&p, &sink).await {
        Ok(report) => {
            // 科研增强:AI 概念示意草稿(默认关;失败静默降级,不影响报告交付)
            let report = attach_image_draft(&p, report).await;
            let _ = store::save_report(&p.id, &report);
            let _ = store::update_status(&p.id, "done", None);
            sink.emit(ResearchEvent::Done { report });
        }
        Err(RunError::Cancelled) => {
            let _ = store::update_status(&p.id, "cancelled", None);
            sink.emit(ResearchEvent::Cancelled);
        }
        Err(RunError::Fatal(msg)) => {
            let _ = store::update_status(&p.id, "error", Some(&msg));
            sink.emit(ResearchEvent::Error { message: msg });
        }
    }
}

/// 模式分发:staged 且阶段非空 → 阶段流水线;否则走原单层 ReAct(legacy)。
async fn run_inner(p: &EngineParams, sink: &EventSink) -> Result<String, RunError> {
    if p.params.mode == "staged" && !p.params.stages.is_empty() {
        run_staged(p, sink).await
    } else {
        run_legacy(p, sink).await
    }
}

/// 快速研究(legacy):原单层 ReAct 循环,事件 stage 字段恒为 0。
async fn run_legacy(p: &EngineParams, sink: &EventSink) -> Result<String, RunError> {
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
                obs_lines.push(format!("已达最大检索次数 {} 次,停止检索", p.params.max_searches));
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

// ---------------- 专项研究(staged):阶段流水线 ----------------

/// 全程共享的资料池与预算消耗。
struct Pool {
    sources: Vec<SourceItem>,
    seen_urls: HashSet<String>,
    next_id: u32,
    searches_done: u32,
    est_tokens: u64,
}

impl Pool {
    fn new() -> Self {
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
fn assemble_summaries(summaries: &[(String, String)]) -> String {
    summaries
        .iter()
        .enumerate()
        .map(|(i, (name, s))| format!("### 阶段 {} · {}\n{}", i + 1, name, s))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn lang_label(code: &str) -> String {
    match code {
        "zh" => "中文",
        "en" => "English",
        _ => "自动",
    }
    .to_string()
}

/// M4:把阶段进度(done/total)回写 db 行,列表页零文件 IO。
fn write_stage_progress(id: &str, file: &stage::StagesFile) {
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
fn hot_reload_stages(id: &str, file: &mut stage::StagesFile) -> Vec<stage::StageConfig> {
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
async fn run_staged(p: &EngineParams, sink: &EventSink) -> Result<String, RunError> {
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

/// 单个检索阶段:ReAct 内层循环 + 阶段内约束护栏 + min_sources 补轮 + 角度校验 + 阶段摘要。
/// 返回 (阶段摘要, 执行轮次, partial);partial=true 表示因失败策略提前中止(M2)。
#[allow(clippy::too_many_arguments)]
async fn run_research_stage(
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
            if p.params.max_searches > 0 && pool.searches_done + todo.len() as u32 >= p.params.max_searches {
                obs_lines.push(format!("已达全局最大检索次数 {} 次,停止检索", p.params.max_searches));
                break;
            }
            if eff.max_searches > 0
                && pool.searches_done + todo.len() as u32 - searches0 >= eff.max_searches
            {
                obs_lines.push(format!("已达本阶段检索次数上限 {} 次,停止检索", eff.max_searches));
                break;
            }
            if p.params.max_sources > 0 && pool.sources.len() >= p.params.max_sources as usize {
                obs_lines.push(format!("已达全局资料上限 {} 条,停止检索", p.params.max_sources));
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
                content: format!("已达本阶段 Token 预算上限(约 {} tok),阶段收尾", eff.token_budget),
                stage: no,
            });
            break;
        }
        // 全局 Token 预算护栏
        if p.params.token_budget > 0 && pool.est_tokens > p.params.token_budget as u64 {
            sink.emit(ResearchEvent::Observation {
                round,
                content: format!("已达全局 Token 预算上限(约 {} tok),阶段收尾", p.params.token_budget),
                stage: no,
            });
            break;
        }
    }

    let rounds_done = round;
    // 必覆盖角度校验(提醒不阻断):角度词在本阶段资料与观察中均未出现 → 缺失
    let missing = verify_angles(&eff.must_cover_angles, &pool.sources[stage_start..], &observations);
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

/// 必覆盖角度校验:角度关键词在资料标题/摘要与观察文本中均未出现视为缺失(提醒,不阻断)。
fn verify_angles(angles: &[String], sources: &[SourceItem], observations: &[String]) -> Vec<String> {
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

/// 阶段摘要系统提示(内置;可被 stage.summary_prompt 覆盖)。
const STAGE_SUMMARY_SYSTEM: &str = r#"你是研究阶段摘要撰写器。基于本阶段收集的资料与检索观察,输出该阶段的结构化摘要。

要求:
1. 3-6 句,围绕阶段目标提炼核心结论;论断用 [n] 引用编号标注来源,编号对应资料列表。
2. 若给出了必覆盖角度缺口或资料缺口,在末尾一句话如实说明。
3. 只输出摘要正文,不要标题、不要代码块、不要前言。"#;

/// 生成阶段摘要(检索阶段收尾;custom summary_prompt 覆盖内置)。
/// H3:拿到真实 usage 时计入预算(不支持的渠道保持原「摘要不计费」行为)。
async fn run_stage_summary(
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
    let max_total = if eff.read_length == "full" { 12000 } else { 8000 };
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
            },
            ChatMessage {
                role: "user".into(),
                content: user,
                images: vec![],
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
async fn run_synthesis(
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
    let mut user = format!("研究主题: {}\n阶段: {}(目标:{})\n", p.topic, st.name, st.goal);
    if !prev.is_empty() {
        user.push_str("\n【前阶段摘要】\n");
        user.push_str(&prev);
    }
    user.push_str(&format!("\n\n【资料列表】\n{digest}\n请输出本阶段分析文本。"));

    let (text, usage) = llm_collect(
        p,
        vec![
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
    )
    .await?;
    if let Some(u) = usage {
        pool.est_tokens += u;
    }
    Ok(text.trim().to_string())
}

/// 暂停闸门:paused 置位时通知前端并异步轮询等待,直到恢复或取消。
/// M-5:改用 tokio 异步睡眠,不再用 std::thread::sleep 霸占 tokio worker 线程。
async fn wait_gate(p: &EngineParams, sink: &EventSink, round: u32, stage: u32) -> Result<(), RunError> {
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

/// 粗略 token 估算:中英文混合按 3 字符 ≈ 1 tok。
fn est_tok(s: &str) -> u64 {
    (s.chars().count() / 3) as u64
}

/// 非反思轮沿用上轮查询时的角度轮换词(避免与历史查询完全重复)。
const ANGLES: [&str; 4] = ["最新进展", "数据与规模", "典型案例", "争议与风险"];

/// 决策步上下文:legacy 与 staged 共用一套 ReAct 决策逻辑,差异全部由本结构注入。
struct DecideCtx<'a> {
    /// 阶段序号(legacy = 0)
    stage_no: u32,
    /// 阶段名(legacy = "")
    name: &'a str,
    /// 阶段目标
    goal: &'a str,
    max_rounds: u32,
    queries_per_round: u32,
    reflect: &'a str,
    read_length: &'a str,
    snippet_chars: usize,
    /// 生效决策模板(阶段覆盖 > 逐次覆盖 > templates.json > 内置,调用方已合并)
    decide_template: String,
    /// 允许模型判定「资料足够」提前 finish
    early_stop: bool,
    /// 前阶段摘要汇编(无则为 "")
    prev_summaries: &'a str,
    /// 前阶段资料明细摘要块(inherit_sources=true 时注入)
    inherit_digest: &'a str,
    must_cover_angles: &'a [String],
    query_language: &'a str,
    /// 科研增强:检索方法论片段(空 = 关闭)
    skill_query: String,
}

impl<'a> DecideCtx<'a> {
    fn legacy(p: &'a EngineParams) -> Self {
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

/// 按反思策略产出本轮决策。
#[allow(clippy::too_many_arguments)]
async fn decide(
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

// ---------------- ReAct 决策步 ----------------

enum Decision {
    Finish,
    Search(Vec<String>),
}

enum StepAction {
    Search(Vec<String>),
    Finish,
}

struct Step {
    thought: String,
    action: StepAction,
}

/// ReAct 系统提示(轮次信息运行时注入)。
const REACT_SYSTEM: &str = r#"你是深度研究 Agent,使用 ReAct 循环(思考 → 检索 → 观察)围绕主题收集资料。

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

fn build_step_messages(
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
        user.push_str(&format!("查询语言倾向:{}\n", lang_label(ctx.query_language)));
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
        let max_total = if ctx.read_length == "full" { 16000 } else { 6000 };
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
        },
        ChatMessage {
            role: "user".into(),
            content: user,
            images: vec![],
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

fn build_plan_all_messages(p: &EngineParams, total: u32) -> Vec<ChatMessage> {
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
    vec![
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
    ]
}

/// 资料摘要块:`[id] title — url\nsnippet`,整块字符数受 max_total 限制。
fn sources_digest(sources: &[SourceItem], max_snippet: usize, max_total: usize) -> String {
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

/// H3:发起一次流式对话请求(研究链路统一入口)。
/// stream_options 被中转站拒绝(400 含 stream_option)时,自动用无该字段重建的
/// Provider 重试一次并记忆回退实例,后续调用直接走回退,与对话链路回退语义一致。
async fn chat_stream_compat(p: &EngineParams, req: ChatRequest) -> Result<ChatStream, RunError> {
    // 已发生过回退:直接复用回退实例(不持锁跨 await,先取出 Arc)
    let fb = p.fallback.lock().ok().and_then(|g| g.clone());
    if let Some(prov) = fb {
        return prov
            .chat_stream(req)
            .await
            .map_err(|e| RunError::Fatal(e.to_string()));
    }
    match p.provider.chat_stream(req.clone()).await {
        Ok(s) => Ok(s),
        Err(e) => {
            if !crate::commands::provider::is_stream_options_rejected(&e) {
                return Err(RunError::Fatal(e.to_string()));
            }
            let Some(rb) = &p.rebuild else {
                return Err(RunError::Fatal(e.to_string()));
            };
            // 中转站不认 stream_options:关闭该字段重建 Provider,重试一次
            let prov = crate::commands::provider::build_provider_with_stream_options(
                &rb.provider_id,
                &rb.model,
                rb.custom_base_url.as_deref(),
                rb.custom_display_name.as_deref(),
                false,
            )
            .map_err(RunError::Fatal)?;
            let prov: Arc<dyn Provider> = Arc::from(prov);
            if let Ok(mut g) = p.fallback.lock() {
                *g = Some(prov.clone());
            }
            prov.chat_stream(req)
                .await
                .map_err(|e| RunError::Fatal(e.to_string()))
        }
    }
}

/// 流式收全量文本(逐 chunk 检查取消)。
/// 返回 (正文, 真实 total_tokens):H3 优先取 stream_options 收尾 chunk 的 usage,
/// 其次 Done 的 prompt+completion;均不可用为 None(调用方回退字符估算)。
async fn llm_collect(
    p: &EngineParams,
    messages: Vec<ChatMessage>,
) -> Result<(String, Option<u64>), RunError> {
    use futures::StreamExt;

    let req = ChatRequest {
        model: p.model.clone(),
        messages,
        temperature: Some(0.4),
        max_tokens: None,
        top_p: None,
        reasoning: None, // 研究链路档位透传为 Phase 2,暂不启用
        stream: true,
    };
    let mut stream = chat_stream_compat(p, req).await?;
    let mut out = String::new();
    let mut usage: Option<u64> = None;
    while let Some(chunk) = stream.next().await {
        check_cancel(p)?;
        match chunk {
            StreamChunk::Delta { content } => out.push_str(&content),
            StreamChunk::Reasoning { .. } => {} // 研究链路只取正文,忽略思考流
            StreamChunk::Error { message } => return Err(RunError::Fatal(message)),
            StreamChunk::Usage { total_tokens, .. } => {
                if let Some(t) = total_tokens {
                    usage = Some(t as u64);
                }
            }
            StreamChunk::Done {
                prompt_tokens,
                completion_tokens,
                ..
            } => {
                if usage.is_none() {
                    usage = match (prompt_tokens, completion_tokens) {
                        (Some(a), Some(b)) => Some((a + b) as u64),
                        (Some(a), None) => Some(a as u64),
                        (None, Some(b)) => Some(b as u64),
                        (None, None) => None,
                    };
                }
                break;
            }
        }
    }
    Ok((out, usage))
}

/// 解析模型输出为 Step(Thought + Action)。
/// 容错:找不到 Action 行时按「空查询的 search」处理(上层再兜底)。
fn parse_step(text: &str, max_queries: u32) -> Step {
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
fn strip_list_marker(line: &str) -> Option<String> {
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

// ---------------- 报告生成 ----------------

const REPORT_SYSTEM: &str = r#"你是研究报告撰写器。基于给定的资料列表,输出 Markdown 研究报告。

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
3. 只输出 Markdown 正文,不要用代码块包裹,不要输出解释性前言。"#;

/// 英文报告模板(输出语言 = English 时使用)。
const REPORT_SYSTEM_EN: &str = r#"You are a research report writer. Based on the given source list, output a Markdown research report in English.

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
3. Output Markdown body only; no code fences, no preamble."#;

async fn run_report(
    p: &EngineParams,
    sink: &EventSink,
    sources: &[SourceItem],
    stages_digest: &str,
    guard_reason: Option<&str>,
) -> Result<String, RunError> {
    use futures::StreamExt;

    // 终稿模板:逐次覆盖 / templates.json 自定义 / 内置;{references} 为引用锚点(不替换,留给 finalize)
    let tpl = p.params.report_template();
    let ref_anchor = tpl.contains("{references}");
    let mut system = template::render(
        &tpl,
        &[
            ("topic", p.topic.clone()),
            (
                "output_lang",
                if p.params.output_lang == "en" {
                    "English".to_string()
                } else {
                    "中文".to_string()
                },
            ),
            ("stages_digest", stages_digest.to_string()),
            ("prev_summaries", stages_digest.to_string()),
        ],
    );
    // M1:护栏提前收尾 → 告知模型在局限说明中如实注明
    if let Some(reason) = guard_reason {
        system.push_str(&format!(
            "\n\n注意:本研究因「{reason}」提前收尾,资料覆盖可能不完整,请在报告的局限说明章节如实注明。"
        ));
    }
    // 科研增强:报告写作纪律 + 图件契约 + 引用纪律(单次装配,关闭时全部为空)
    let frags = skills::fragments(&p.params);
    if !frags.report.is_empty() {
        system.push_str("\n\n");
        system.push_str(&frags.report);
    }
    let max_total = if p.params.read_length == "full" { 24000 } else { 16000 };
    let mut user = format!("研究主题: {}\n\n", p.topic);
    if !stages_digest.is_empty() {
        user.push_str("【阶段摘要】\n");
        user.push_str(stages_digest);
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

    let report = finalize_report(&out, sources, ref_anchor);
    if report.trim().is_empty() {
        return Err(RunError::Fatal("报告生成为空".into()));
    }
    // M1:护栏提前收尾 → 报告首行提示条,读者一眼可见
    let mut report = if let Some(reason) = guard_reason {
        format!("> ⚠ 本研究因「{reason}」提前收尾,资料覆盖可能不完整。\n\n{report}")
    } else {
        report
    };
    // 科研增强:深度审计 pass(生成后一次本地提示词调用,产出审计附录;失败静默降级)
    if let Some(audit_sys) = frags.audit {
        match run_audit(p, audit_sys, &report).await {
            Ok(appendix) if !appendix.trim().is_empty() => {
                report.push_str("\n\n");
                report.push_str(appendix.trim());
            }
            _ => {}
        }
    }
    Ok(report)
}

/// 深度审计:对已生成的报告做一次引用与统计审计,返回审计附录 Markdown。
async fn run_audit(p: &EngineParams, audit_system: String, report: &str) -> Result<String, RunError> {
    // 报告本体截断,审计输入与资料注入同量级,避免超长报告拖垮上下文
    let body: String = report.chars().take(24000).collect();
    let user = format!("【待审计报告】\n{body}\n\n请输出审计附录。");
    let (text, _usage) = llm_collect(
        p,
        vec![
            ChatMessage {
                role: "system".into(),
                content: audit_system,
                images: vec![],
            },
            ChatMessage {
                role: "user".into(),
                content: user,
                images: vec![],
            },
        ],
    )
    .await?;
    Ok(text)
}

/// 科研增强:终稿追加 AI 概念示意草稿(默认关)。任何一步失败都静默降级,不影响报告交付。
async fn attach_image_draft(p: &EngineParams, report: String) -> String {
    if !p.params.skill_enhance || !p.params.skill_image_draft {
        return report;
    }
    let (Some(provider_id), Some(model)) = (
        p.params.skill_image_provider.as_deref(),
        p.params.skill_image_model.as_deref(),
    ) else {
        return report;
    };
    if provider_id.is_empty() || model.is_empty() {
        return report;
    }
    // 从报告 TL;DR 提炼草稿主题(缺 TL;DR 时用主题句)
    let tldr = extract_tldr_section(&report).unwrap_or_else(|| p.topic.clone());
    let req = crate::provider::ImageRequest {
        model: model.to_string(),
        prompt: skills::image_draft_prompt(&p.topic, &tldr),
        size: Some("1024x1024".into()),
        n: Some(1),
        style: None,
    };
    let resp = match p.provider.generate_image(req).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[research] AI 草稿生成失败(静默降级): {e}");
            return report;
        }
    };
    let fname = match save_figure(p, &resp.url).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[research] AI 草稿落盘失败(静默降级): {e}");
            return report;
        }
    };
    // 追加草稿章节:图注明确标注 AI 生成、不构成证据
    let mut out = report;
    out.push_str("\n\n## 概念示意草稿\n\n");
    out.push_str(&format!(
        "![AI 生成概念草稿](figures/{fname})\n\n\
> 上图为 AI 生成的概念示意草稿,仅辅助理解研究核心结论,不构成证据,不包含真实数据。\n"
    ));
    out
}

/// 从报告中提取 TL;DR 节正文(首个二级节,截到下一个二级标题)。
fn extract_tldr_section(report: &str) -> Option<String> {
    let start = report.find("## TL;DR")?;
    let rest = &report[start + "## TL;DR".len()..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    let body: String = rest[..end]
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('>'))
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = body.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// 图示字节上限,与图片下载器的 20MB 护栏一致(防止模型返回超大 data: URI 撑爆内存/磁盘,M-6)。
const FIGURE_MAX_BYTES: usize = 20 * 1024 * 1024;

/// 保存研究图示到 `data/research/{id}/figures/`,返回文件名。
/// 按字节内容嗅探真实格式(智谱等厂家可能返回 JPEG),统一走 base64/URL 双通道。
async fn save_figure(p: &EngineParams, url: &str) -> Result<String, String> {
    use base64::Engine as _;
    let bytes: Vec<u8> = if let Some(rest) = url
        .strip_prefix("data:")
        .and_then(|r| r.split_once(',').map(|(_, payload)| payload))
    {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(rest)
            .map_err(|e| format!("base64 解码失败: {e}"))?;
        if decoded.len() > FIGURE_MAX_BYTES {
            return Err(format!("图示超过 {}MB 上限", FIGURE_MAX_BYTES / 1024 / 1024));
        }
        decoded
    } else {
        // 外部 URL 下载复用带 SSRF 校验的受控下载器(禁自动重定向、逐跳校验、20MB 上限)
        crate::commands::provider::download_image_checked(url).await?
    };
    let ext = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "jpg"
    } else {
        "png"
    };
    let dir = store::research_dir(&p.id).join("figures");
    std::fs::create_dir_all(&dir).map_err(|e| format!("figures 目录创建失败: {e}"))?;
    let fname = format!(
        "draft-{}.{}",
        // 无点毫秒格式,与图示文件名「单点白名单」约束一致
        chrono::Local::now().format("%Y%m%d-%H%M%S%3f"),
        ext
    );
    std::fs::write(dir.join(&fname), &bytes).map_err(|e| format!("图示写入失败: {e}"))?;
    Ok(fname)
}

/// 清洗模型输出:去代码块包裹、截断模型自写的资料来源节,追加系统生成的引用列表。
/// ref_anchor=true 且模型原样输出了 {references} 时,引用列表插入锚点位置;否则追加到文末。
fn finalize_report(raw: &str, sources: &[SourceItem], ref_anchor: bool) -> String {
    let mut text = raw.trim().to_string();

    // 去整段代码块包裹
    if text.starts_with("```") {
        if let Some(nl) = text.find('\n') {
            text = text[nl + 1..].to_string();
        }
        if let Some(s) = text.strip_suffix("```") {
            text = s.trim_end().to_string();
        }
    }

    // 截断模型自写的资料来源节(以系统生成为准)
    for marker in ["## 资料来源", "## 参考资料", "## 参考文献"] {
        if let Some(idx) = text.find(marker) {
            text = text[..idx].trim_end().to_string();
            break;
        }
    }

    let mut refs = String::from("## 资料来源\n\n");
    for s in sources {
        refs.push_str(&format!("{}. [{}]({})\n", s.id, s.title, s.url));
    }
    // 引用锚点:模板含 {references} 且模型原样输出时,插入锚点位置
    if ref_anchor && text.contains("{references}") {
        return text.replace("{references}", refs.trim_end());
    }
    format!("{}\n\n{}", text.trim_end(), refs)
}

// ---------------- 测试 ----------------

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

    #[test]
    fn finalize_report_replaces_sources_section() {
        let raw = "# 《x》研究报告\n\n## TL;DR\n\n结论 [1]\n\n## 资料来源\n\n1. [假](http://fake)\n";
        let sources = vec![SourceItem {
            id: 1,
            title: "真标题".into(),
            url: "https://real.example".into(),
            snippet: String::new(),
            engine: "searxng".into(),
            published: None,
        }];
        let out = finalize_report(raw, &sources, false);
        assert!(!out.contains("http://fake"));
        assert!(out.contains("1. [真标题](https://real.example)"));
        assert!(out.ends_with("1. [真标题](https://real.example)\n"));
    }

    #[test]
    fn finalize_report_strips_code_fence() {
        let raw = "```markdown\n# 《x》研究报告\n\n正文\n```";
        let out = finalize_report(raw, &[], false);
        assert!(out.starts_with("# 《x》研究报告"));
        assert!(!out.contains("```"));
    }

    #[test]
    fn global_constraints_serde_defaults() {
        // 旧 presets.json 无全局约束字段时 serde default 兜底(步骤 3)
        let p: ResearchParams = serde_json::from_str(r#"{"max_rounds":8}"#).unwrap();
        assert_eq!(p.concurrent_queries, 1);
        assert_eq!(p.fail_policy, "skip");
        assert_eq!(p.dedup_scope, "global");
        assert_eq!(p.max_duration_minutes, 0);
        assert_eq!(p.max_sources, 0);
    }

    #[test]
    fn finalize_report_inserts_refs_at_anchor() {
        let sources = vec![SourceItem {
            id: 1,
            title: "来源甲".into(),
            url: "https://a.example".into(),
            snippet: String::new(),
            engine: "bocha".into(),
            published: None,
        }];
        // 模型原样输出锚点 → 引用列表插入锚点位置
        let raw = "# 报告\n\n## 正文\n\n结论 [1]\n\n{references}\n";
        let out = finalize_report(raw, &sources, true);
        assert!(!out.contains("{references}"));
        assert!(out.contains("## 正文\n\n结论 [1]\n\n## 资料来源\n\n1. [来源甲](https://a.example)"));
        // 模板无锚点 → 维持追加到文末
        let out2 = finalize_report(raw, &sources, false);
        assert!(out2.contains("{references}\n\n## 资料来源"));
    }
}
