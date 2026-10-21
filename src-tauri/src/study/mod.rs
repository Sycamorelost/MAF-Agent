//! 学习规划引擎(P0):确定性流水线 ——
//!
//! ```text
//! intent(意图合并/兜底) → map(知识地图) → stages(阶段设计,校验重试≤3+降级 repaired)
//!   → [resources ∥ schedule](资料推荐 ∥ 确定性排期) → validate → export(落库)
//! ```
//!
//! - LLM 调用/事件流/暂停终止复用研究引擎同款设施(Provider + Channel + AtomicBool);
//! - 结构校验(阶段数/难度阶梯/知识点覆盖/前置引用)在 Rust 端确定性完成,不依赖模型自检;
//! - 排期为确定性计算(零 LLM 调用,零幻觉);资料推荐为单次 LLM 调用,失败静默跳过。

pub mod learn;
pub mod store;
pub mod template;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

use crate::provider::{ChatMessage, ChatRequest, Provider, StreamChunk};

// ---------------- 契约类型 ----------------

/// null → 类型默认值:模型常输出 `"field": null`,serde(default) 只兜「字段缺失」不兜显式 null。
fn null_to_default<'de, T, D>(d: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + Default,
    D: serde::Deserializer<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// 学习意图(preflight 产出 / 流水线合并结果)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Intent {
    #[serde(default, deserialize_with = "null_to_default")]
    pub topic: String,
    /// zero / beginner / intermediate / advanced / 空
    #[serde(default, deserialize_with = "null_to_default")]
    pub level: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub goal: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub daily_minutes: u32,
    pub deadline: Option<String>,
    #[serde(default, deserialize_with = "null_to_default")]
    pub lang: String,
    /// 缺失的可选字段名(preflight 用)
    #[serde(default, deserialize_with = "null_to_default")]
    pub missing: Vec<String>,
    /// 追问(preflight 用)
    #[serde(default, deserialize_with = "null_to_default")]
    pub questions: Vec<String>,
}

impl Default for Intent {
    fn default() -> Self {
        Self {
            topic: String::new(),
            level: String::new(),
            goal: String::new(),
            daily_minutes: 30,
            deadline: None,
            lang: "zh".into(),
            missing: Vec::new(),
            questions: Vec::new(),
        }
    }
}

/// 知识点(知识地图节点)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSpec {
    pub name: String,
    /// 1-5
    pub difficulty: u8,
    #[serde(default)]
    pub why: String,
    #[serde(default)]
    pub must_learn: bool,
    #[serde(default)]
    pub prereqs: Vec<String>,
}

/// 学习阶段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageSpec {
    pub index: u32,
    pub name: String,
    #[serde(default)]
    pub goal: String,
    pub difficulty: u8,
    #[serde(default = "default_weeks")]
    pub estimated_weeks: u32,
    pub topics: Vec<String>,
    #[serde(default)]
    pub milestone: String,
}

fn default_weeks() -> u32 {
    2
}

/// 周粒度排期行(确定性计算)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleWeek {
    pub week: u32,
    pub stage: u32,
    pub focus: String,
    pub minutes_per_day: u32,
}

/// 阶段资料推荐。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResource {
    pub stage: u32,
    pub items: Vec<String>,
}

/// 计划载荷(stages_json / plan.json / Done 事件的完整内容)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanPayload {
    pub topic: String,
    pub level: String,
    pub goal: String,
    pub daily_minutes: u32,
    pub deadline: Option<String>,
    pub lang: String,
    pub nodes: Vec<NodeSpec>,
    pub stages: Vec<StageSpec>,
    pub schedule: Vec<ScheduleWeek>,
    pub resources: Vec<StageResource>,
    /// true = 校验失败降级生成的默认结构
    pub repaired: bool,
}

impl Default for PlanPayload {
    fn default() -> Self {
        Self {
            topic: String::new(),
            level: String::new(),
            goal: String::new(),
            daily_minutes: 30,
            deadline: None,
            lang: "zh".into(),
            nodes: Vec::new(),
            stages: Vec::new(),
            schedule: Vec::new(),
            resources: Vec::new(),
            repaired: false,
        }
    }
}

/// 过程事件(推前端 + 落 trail.jsonl)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StudyEvent {
    /// 流水线步骤开始(step: intent/map/stages/resources/schedule/validate/export)
    StepStart { step: String, label: String },
    /// 步骤结果摘要行
    StepNote { step: String, content: String },
    /// 校验失败带反馈重试
    Retry { step: String, attempt: u32, reason: String },
    Paused,
    Resumed,
    Done { plan: Option<PlanPayload> },
    Cancelled,
    Error { message: String },
}

/// 引擎运行参数(命令层装配)。
pub struct StudyParams {
    pub id: String,
    pub topic: String,
    /// preflight 产出的意图 JSON(None = 流水线内跑一次 LLM 意图抽取)
    pub intent_json: Option<String>,
    /// 面板兜底:每天可投入分钟(intent 缺省时用)
    pub daily_minutes: u32,
    /// 面板兜底:目标完成日期
    pub deadline: Option<String>,
    pub lang: String,
    pub provider: Box<dyn Provider>,
    pub model: String,
    pub cancel: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
    pub channel: Channel<StudyEvent>,
    /// H3 同款:stream_options 被拒时重建 Provider 的入参快照
    pub rebuild: Option<crate::research::ProviderRebuild>,
    pub fallback: std::sync::Mutex<Option<Arc<dyn Provider>>>,
}

enum RunError {
    Cancelled,
    Fatal(String),
}

struct Sink {
    id: String,
    channel: Channel<StudyEvent>,
}

impl Sink {
    fn emit(&self, ev: StudyEvent) {
        let _ = store::append_trail(&self.id, &ev);
        let _ = self.channel.send(ev);
    }
}

fn check_cancel(p: &StudyParams) -> Result<(), RunError> {
    if p.cancel.load(Ordering::Relaxed) {
        Err(RunError::Cancelled)
    } else {
        Ok(())
    }
}

/// 步骤间闸门:取消即退出;暂停则挂起等待(进入/退出各发一次事件)。
async fn gate(p: &StudyParams, sink: &Sink) -> Result<(), RunError> {
    check_cancel(p)?;
    let mut notified_pause = false;
    while p.paused.load(Ordering::Relaxed) {
        if !notified_pause {
            notified_pause = true;
            sink.emit(StudyEvent::Paused);
        }
        if p.cancel.load(Ordering::Relaxed) {
            return Err(RunError::Cancelled);
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    if notified_pause {
        sink.emit(StudyEvent::Resumed);
    }
    Ok(())
}

// ---------------- LLM 调用 ----------------

/// 流式收全量文本(逐 chunk 检查取消;暂停在 chunk 间不挂起,步骤间统一挂起)。
async fn llm_collect(
    p: &StudyParams,
    messages: Vec<ChatMessage>,
) -> Result<String, RunError> {
    use futures::StreamExt;

    let req = ChatRequest {
        model: p.model.clone(),
        messages,
        temperature: Some(0.4),
        max_tokens: None,
        top_p: None,
        reasoning: None,
        stream: true,
    };
    // H3 同款:已回退过则直接复用回退实例;stream_options 被拒时回退重建一次
    let cached = p.fallback.lock().ok().and_then(|g| g.clone());
    let mut stream = if let Some(prov) = cached {
        prov.chat_stream(req).await.map_err(|e| RunError::Fatal(e.to_string()))?
    } else {
        match p.provider.chat_stream(req.clone()).await {
            Ok(s) => s,
            Err(e) => {
                let rejected = crate::commands::provider::is_stream_options_rejected(&e);
                let has_rb = p.rebuild.is_some();
                if !rejected || !has_rb {
                    return Err(RunError::Fatal(e.to_string()));
                }
                let rb = p.rebuild.as_ref().unwrap();
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
                    .map_err(|e| RunError::Fatal(e.to_string()))?
            }
        }
    };

    let mut out = String::new();
    while let Some(chunk) = stream.next().await {
        check_cancel(p)?;
        match chunk {
            StreamChunk::Delta { content } => out.push_str(&content),
            StreamChunk::Reasoning { .. } => {}
            StreamChunk::Error { message } => return Err(RunError::Fatal(message)),
            StreamChunk::Usage { .. } => {}
            StreamChunk::Done { .. } => break,
        }
    }
    Ok(out)
}

/// 从模型输出中提取 JSON 对象(容忍代码围栏与前后闲话)。
pub fn extract_json(text: &str) -> Option<String> {
    let s = text.trim();
    let s = s.strip_prefix("```json").or_else(|| s.strip_prefix("```")).unwrap_or(s);
    let s = s.strip_suffix("```").unwrap_or(s).trim();
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end < start {
        return None;
    }
    Some(s[start..=end].to_string())
}

pub(super) fn sys_msg(content: String) -> ChatMessage {
    ChatMessage { role: "system".into(), content, images: Vec::new() }
}

pub(super) fn user_msg(content: String) -> ChatMessage {
    ChatMessage { role: "user".into(), content, images: Vec::new() }
}

// ---------------- 意图 ----------------

/// 意图修复:clamp 分钟数、goal 兜底、deadline 格式兜底。
fn fix_intent(mut i: Intent, fb_minutes: u32, fb_deadline: Option<String>) -> Intent {
    i.topic = i.topic.trim().to_string();
    if i.daily_minutes == 0 {
        i.daily_minutes = fb_minutes.max(1);
    }
    i.daily_minutes = i.daily_minutes.clamp(5, 600);
    if i.goal.trim().is_empty() {
        i.goal = i.topic.clone();
    }
    if let Some(d) = &i.deadline {
        if d.trim().is_empty() || d == "null" {
            i.deadline = None;
        }
    }
    if i.deadline.is_none() {
        i.deadline = fb_deadline.filter(|d| !d.trim().is_empty());
    }
    if i.lang != "en" {
        i.lang = "zh".into();
    }
    i
}

/// 面板已确认的意图字段(preflight 前用户已手工填写,不再追问)。
#[derive(Debug, Default, Clone)]
pub struct IntentHints {
    pub level: Option<String>,
    pub daily_minutes: Option<u32>,
    pub deadline: Option<String>,
}

/// preflight:单独跑一次意图抽取,返回意图(含 missing/questions)。
pub async fn preflight(
    topic: &str,
    lang: &str,
    hints: IntentHints,
    provider: Box<dyn Provider>,
    model: &str,
) -> Result<Intent, String> {
    let sys = template::effective("study_intent");
    let user = format!("用户输入:{topic}");
    let text = llm_plain(provider.as_ref(), model, vec![sys_msg(sys), user_msg(user)]).await?;
    // 严格解析失败(模型输出截断/畸形 JSON)时打捞已知字段兜底,不再整体报错
    let intent: Intent = match extract_json(&text)
        .and_then(|j| serde_json::from_str::<Intent>(&j).ok())
    {
        Some(i) => i,
        None => salvage_intent(&text),
    };
    let mut intent = fix_intent(intent, hints.daily_minutes.unwrap_or(30), hints.deadline.clone());
    intent.topic = if intent.topic.is_empty() { topic.to_string() } else { intent.topic };
    intent.lang = lang.to_string();
    // 面板已确认的字段直接采纳,不让模型再追问
    if let Some(l) = hints.level.as_deref().filter(|s| !s.trim().is_empty()) {
        intent.level = l.trim().to_string();
    }
    if let Some(m) = hints.daily_minutes {
        intent.daily_minutes = m.clamp(5, 600);
    }
    if let Some(d) = hints.deadline.as_deref().filter(|s| !s.trim().is_empty()) {
        intent.deadline = Some(d.trim().to_string());
    }
    // missing 由确定性计算,不信任模型自报
    intent.missing = missing_fields(&intent);
    if !intent.missing.is_empty() && intent.questions.is_empty() {
        intent.questions = default_questions(&intent.missing);
    }
    if intent.missing.is_empty() {
        intent.questions.clear();
    }
    Ok(intent)
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
fn salvage_intent(text: &str) -> Intent {
    let mut i = Intent::default();
    i.topic = salvage_str(text, "topic");
    i.level = salvage_str(text, "level");
    i.goal = salvage_str(text, "goal");
    if let Some(m) = salvage_field(text, "daily_minutes") {
        i.daily_minutes = m.trim_matches('"').parse::<u32>().unwrap_or(0);
    }
    let dl = salvage_str(text, "deadline");
    if !dl.is_empty() && dl != "null" {
        i.deadline = Some(dl);
    }
    i
}

fn missing_fields(i: &Intent) -> Vec<String> {
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

fn default_questions(missing: &[String]) -> Vec<String> {
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

/// 流水线步骤 1:意图合并(preflight 结果 + 面板兜底),不调 LLM。
fn resolve_intent(p: &StudyParams) -> Result<Intent, RunError> {
    let base = match &p.intent_json {
        Some(txt) => serde_json::from_str::<Intent>(txt)
            .map_err(|e| RunError::Fatal(format!("意图数据损坏:{e}")))?,
        None => Intent::default(),
    };
    let mut i = fix_intent(base, p.daily_minutes.max(1), p.deadline.clone());
    if i.topic.is_empty() {
        i.topic = p.topic.clone();
    }
    if i.goal.trim().is_empty() || i.goal == i.topic {
        i.goal = p.topic.clone();
    }
    i.lang = p.lang.clone();
    Ok(i)
}

// ---------------- 校验 ----------------

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
                issues.push(format!("阶段 {}「{}」引用了地图外知识点「{}」", si + 1, s.name, t));
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
    let per = (n + 2) / 3;
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

// ---------------- 排期(确定性) ----------------

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
            let prereqs =
                serde_json::to_string(&n.prereqs).map_err(|e| e.to_string())?;
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

// ---------------- 主流程 ----------------

/// 运行一次规划流水线(后台任务)。负责终态落库与事件收尾。
pub async fn run(p: StudyParams) {
    let sink = Sink { id: p.id.clone(), channel: p.channel.clone() };
    match run_inner(&p, &sink).await {
        Ok(payload) => {
            let _ = store::save_payload(&p.id, &payload);
            let _ = store::finish_plan(&p.id, &payload);
            sink.emit(StudyEvent::Done { plan: Some(payload) });
        }
        Err(RunError::Cancelled) => {
            let _ = store::update_status(&p.id, "cancelled", None);
            sink.emit(StudyEvent::Cancelled);
        }
        Err(RunError::Fatal(msg)) => {
            let _ = store::update_status(&p.id, "error", Some(&msg));
            sink.emit(StudyEvent::Error { message: msg });
        }
    }
}

async fn run_inner(p: &StudyParams, sink: &Sink) -> Result<PlanPayload, RunError> {
    let lang_label = if p.lang == "en" { "English" } else { "中文" };

    // ① intent
    sink.emit(StudyEvent::StepStart { step: "intent".into(), label: "对齐学习意图".into() });
    gate(p, sink).await?;
    let intent = resolve_intent(p)?;
    sink.emit(StudyEvent::StepNote {
        step: "intent".into(),
        content: format!(
            "主题:{} · 水平:{} · 每天 {} 分钟{}",
            intent.topic,
            if intent.level.is_empty() { "未指定" } else { &intent.level },
            intent.daily_minutes,
            intent.deadline.as_ref().map(|d| format!(" · 截止 {d}")).unwrap_or_default(),
        ),
    });

    // ② map
    sink.emit(StudyEvent::StepStart { step: "map".into(), label: "生成知识地图".into() });
    gate(p, sink).await?;
    let nodes = run_map(p, sink, &intent, lang_label).await?;

    // ③ stages(校验重试 ≤3 + 降级)
    sink.emit(StudyEvent::StepStart { step: "stages".into(), label: "设计学习阶段".into() });
    gate(p, sink).await?;
    let (mut stages, repaired) = run_stages(p, sink, &intent, &nodes, lang_label).await?;

    // ④ resources ∥ schedule
    sink.emit(StudyEvent::StepStart { step: "resources".into(), label: "推荐学习资料".into() });
    sink.emit(StudyEvent::StepStart { step: "schedule".into(), label: "计算周排期".into() });
    gate(p, sink).await?;
    let resources_fut = run_resources(p, &stages, lang_label);
    let schedule_fut = {
        let stages = stages.clone();
        async move { calc_schedule(&stages, intent.daily_minutes) }
    };
    let (resources, schedule) = futures::join!(resources_fut, schedule_fut);
    sink.emit(StudyEvent::StepNote {
        step: "schedule".into(),
        content: format!("共 {} 周,每天约 {} 分钟", schedule.len(), intent.daily_minutes),
    });
    if resources.is_empty() {
        sink.emit(StudyEvent::StepNote {
            step: "resources".into(),
            content: "未获取到资料推荐(已跳过,不影响计划)".into(),
        });
    } else {
        sink.emit(StudyEvent::StepNote {
            step: "resources".into(),
            content: format!("为 {} 个阶段给出资料建议", resources.len()),
        });
    }

    // ⑤ validate(结构校验已在 run_stages 内完成,此处做终检与标记)
    sink.emit(StudyEvent::StepStart { step: "validate".into(), label: "校验计划结构".into() });
    gate(p, sink).await?;
    // 兜底:万一分组仍有遗漏(理论上 run_stages 已保证),把未覆盖节点追加到末阶段
    if let Some(extra) = uncovered_nodes(&nodes, &stages) {
        if let Some(last) = stages.last_mut() {
            last.topics.extend(extra.iter().cloned());
        }
    }
    let final_issues = validate_stages(&stages, &nodes);
    if !final_issues.is_empty() && !repaired {
        stages = fallback_repaired(&nodes, &intent);
    }
    if repaired || !final_issues.is_empty() {
        sink.emit(StudyEvent::StepNote {
            step: "validate".into(),
            content: "校验未完全通过,已降级为默认三阶段结构(计划内标注 repaired)".into(),
        });
    } else {
        sink.emit(StudyEvent::StepNote {
            step: "validate".into(),
            content: format!(
                "通过:{} 个阶段 / {} 个知识点 / 难度递增合规",
                stages.len(),
                nodes.len()
            ),
        });
    }

    // ⑥ export
    sink.emit(StudyEvent::StepStart { step: "export".into(), label: "保存学习计划".into() });
    gate(p, sink).await?;
    let payload = PlanPayload {
        topic: intent.topic.clone(),
        level: intent.level.clone(),
        goal: intent.goal.clone(),
        daily_minutes: intent.daily_minutes,
        deadline: intent.deadline.clone(),
        lang: intent.lang.clone(),
        nodes,
        stages,
        schedule,
        resources,
        repaired: repaired || !final_issues.is_empty(),
    };
    sink.emit(StudyEvent::StepNote {
        step: "export".into(),
        content: "计划已保存,可在列表中随时打开".into(),
    });
    Ok(payload)
}

/// 未被任何阶段覆盖的知识点。
fn uncovered_nodes(nodes: &[NodeSpec], stages: &[StageSpec]) -> Option<Vec<String>> {
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

/// 步骤 ②:知识地图。
async fn run_map(
    p: &StudyParams,
    sink: &Sink,
    intent: &Intent,
    lang_label: &str,
) -> Result<Vec<NodeSpec>, RunError> {
    let sys = template::effective("study_map");
    let user = format!(
        "学习主题:{}\n当前水平:{}\n学习目标:{}\n输出语言:{lang_label}",
        intent.topic, intent.level, intent.goal
    );
    let mut last_err = String::new();
    for attempt in 1..=3u32 {
        gate(p, sink).await?;
        let text = llm_collect(
            p,
            vec![sys_msg(sys.clone()), user_msg(user.clone())],
        )
        .await?;
        if let Some(json) = extract_json(&text) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                let nodes = parse_nodes(&v);
                if nodes.len() >= 3 {
                    sink.emit(StudyEvent::StepNote {
                        step: "map".into(),
                        content: format!("{} 个知识点,难度 1-5 递进", nodes.len()),
                    });
                    return Ok(nodes);
                }
                last_err = format!("知识点过少({} 个,至少 3 个)", nodes.len());
            } else {
                last_err = "JSON 解析失败".into();
            }
        } else {
            last_err = "输出中未找到 JSON".into();
        }
        if attempt < 3 {
            sink.emit(StudyEvent::Retry { step: "map".into(), attempt, reason: last_err.clone() });
        }
    }
    Err(RunError::Fatal(format!("知识地图生成失败:{last_err}")))
}

/// 步骤 ③:阶段设计(带确定性校验与重试)。
async fn run_stages(
    p: &StudyParams,
    sink: &Sink,
    intent: &Intent,
    nodes: &[NodeSpec],
    lang_label: &str,
) -> Result<(Vec<StageSpec>, bool), RunError> {
    let sys = template::effective("study_stages");
    let nodes_digest = serde_json::to_string(
        &nodes
            .iter()
            .map(|n| serde_json::json!({
                "name": n.name,
                "difficulty": n.difficulty,
                "must_learn": n.must_learn,
                "prereqs": n.prereqs,
            }))
            .collect::<Vec<_>>(),
    )
    .map_err(|e| RunError::Fatal(e.to_string()))?;
    let base_user = format!(
        "学习主题:{}\n输出语言:{lang_label}\n知识地图(只能引用其中的 name):\n{nodes_digest}",
        intent.topic
    );

    let mut feedback = String::new();
    for attempt in 1..=3u32 {
        gate(p, sink).await?;
        let user = if feedback.is_empty() {
            base_user.clone()
        } else {
            format!("{base_user}\n\n上一轮输出存在以下问题,必须修正:\n{feedback}")
        };
        let text = llm_collect(p, vec![sys_msg(sys.clone()), user_msg(user)]).await?;
        if let Some(json) = extract_json(&text) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                let mut stages = parse_stages(&v);
                fix_stages(&mut stages, nodes);
                let issues = validate_stages(&stages, nodes);
                if issues.is_empty() {
                    sink.emit(StudyEvent::StepNote {
                        step: "stages".into(),
                        content: format!("{} 个阶段,难度递增无跳级", stages.len()),
                    });
                    return Ok((stages, false));
                }
                feedback = issues
                    .iter()
                    .take(6)
                    .map(|s| format!("- {s}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                if attempt < 3 {
                    sink.emit(StudyEvent::Retry {
                        step: "stages".into(),
                        attempt,
                        reason: feedback.clone(),
                    });
                }
                continue;
            }
        }
        feedback = "- 输出不是合法的 {\"stages\":[...]} JSON,必须只输出该结构".into();
        if attempt < 3 {
            sink.emit(StudyEvent::Retry { step: "stages".into(), attempt, reason: feedback.clone() });
        }
    }
    // 降级:默认三阶段,repaired 标记
    let stages = fallback_repaired(nodes, intent);
    sink.emit(StudyEvent::StepNote {
        step: "stages".into(),
        content: "重试后仍未通过校验,已降级为默认三阶段结构".into(),
    });
    Ok((stages, true))
}

/// 步骤 ④a:资料推荐(失败静默跳过,返回空)。
async fn run_resources(
    p: &StudyParams,
    stages: &[StageSpec],
    lang_label: &str,
) -> Vec<StageResource> {
    let sys = template::effective("study_resources");
    let digest = stages
        .iter()
        .map(|s| {
            format!(
                "阶段{}「{}」:{}(知识点:{})",
                s.index,
                s.name,
                s.goal,
                s.topics.join("、")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let user = format!("学习阶段列表:\n{digest}\n输出语言:{lang_label}");
    let text = match llm_collect(p, vec![sys_msg(sys), user_msg(user)]).await {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let Some(json) = extract_json(&text) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) else {
        return Vec::new();
    };
    parse_resources(&v, stages.len())
}

// ---------------- 解析与修复 ----------------

/// 解析知识地图输出,做确定性修复(clamp 难度、去重、修剪悬空前置)。
fn parse_nodes(v: &serde_json::Value) -> Vec<NodeSpec> {
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
        let must_learn = item.get("must_learn").and_then(|x| x.as_bool()).unwrap_or(false);
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
        out.push(NodeSpec { name, difficulty, why, must_learn, prereqs });
    }
    // 悬空前置引用修剪
    let known: std::collections::HashSet<String> = out.iter().map(|n| n.name.clone()).collect();
    for n in &mut out {
        n.prereqs.retain(|p| known.contains(p) && *p != n.name);
    }
    out
}

/// 解析阶段输出(裸解析,不修复)。
fn parse_stages(v: &serde_json::Value) -> Vec<StageSpec> {
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
fn fix_stages(stages: &mut [StageSpec], nodes: &[NodeSpec]) {
    let known: std::collections::HashSet<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, s) in stages.iter_mut().enumerate() {
        s.index = i as u32;
        if s.estimated_weeks == 0 {
            s.estimated_weeks = 2;
        }
        s.topics.retain(|t| {
            known.contains(t.as_str())
                && !seen.contains(t)
                && {
                    seen.insert(t.clone());
                    true
                }
        });
    }
}

/// 解析资料推荐,clamp 阶段序号与条数。
fn parse_resources(v: &serde_json::Value, stage_count: usize) -> Vec<StageResource> {
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
        if let Some(existing) = out.iter_mut().find(|r: &&mut StageResource| r.stage == stage) {
            existing.items.extend(items);
        } else {
            out.push(StageResource { stage, items });
        }
    }
    out.sort_by_key(|r| r.stage);
    out
}

/// 非流式便捷调用(preflight / P1 学习单元与测验生成用)。
pub(super) async fn llm_plain(
    provider: &dyn Provider,
    model: &str,
    messages: Vec<ChatMessage>,
) -> Result<String, String> {
    use futures::StreamExt;
    let req = ChatRequest {
        model: model.to_string(),
        messages,
        temperature: Some(0.3),
        max_tokens: None,
        top_p: None,
        reasoning: None,
        stream: true,
    };
    let mut stream = provider.chat_stream(req).await.map_err(|e| e.to_string())?;
    let mut out = String::new();
    while let Some(chunk) = stream.next().await {
        match chunk {
            StreamChunk::Delta { content } => out.push_str(&content),
            StreamChunk::Reasoning { .. } => {}
            StreamChunk::Error { message } => return Err(message),
            StreamChunk::Usage { .. } => {}
            StreamChunk::Done { .. } => break,
        }
    }
    Ok(out)
}

// ---------------- 导出 Markdown ----------------

/// 学习计划 → Markdown(导出用,纯函数可测)。
pub fn export_markdown(payload: &PlanPayload) -> String {
    let mut md = String::new();
    md.push_str(&format!("# 学习计划:{}\n\n", payload.topic));
    md.push_str(&format!("- **学习目标**:{}\n", payload.goal));
    md.push_str(&format!(
        "- **当前水平**:{}\n",
        level_label(&payload.level)
    ));
    md.push_str(&format!("- **每日投入**:约 {} 分钟\n", payload.daily_minutes));
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

pub(super) fn level_label(level: &str) -> &'static str {
    match level {
        "zero" => "零基础",
        "beginner" => "初学者",
        "intermediate" => "中级",
        "advanced" => "高级",
        _ => "未指定",
    }
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
        assert!(serde_json::from_str::<Intent>(raw).is_err(), "前置:严格解析必须失败");
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
        let raw = r#"{"topic":"我想入门学习 java 的前端设计,"level":"beginner","daily_minutes":45}"#;
        assert!(serde_json::from_str::<Intent>(raw).is_err(), "前置:严格解析必须失败");
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
    fn validate_rejects_bad_structures() {
        let nodes = vec![node("A", 1, true), node("B", 2, false), node("C", 3, true), node("D", 4, false)];
        // 阶段过少
        let st = vec![stage(0, "s", 1, &["A"])];
        assert!(validate_stages(&st, &nodes).iter().any(|i| i.contains("阶段数")));
        // 难度跳级(相邻差 >2)
        let st = vec![
            stage(0, "s1", 1, &["A"]),
            stage(1, "s2", 2, &["B"]),
            stage(2, "s3", 5, &["C", "D"]),
        ];
        assert!(validate_stages(&st, &nodes).iter().any(|i| i.contains("跳级")));
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
        assert!(validate_stages(&st, &nodes).iter().any(|i| i.contains("重复")));
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
        let intent = Intent { topic: "Rust".into(), ..Default::default() };
        let nodes: Vec<NodeSpec> = (0..9)
            .map(|i| node(&format!("N{i}"), (i % 5 + 1) as u8, i % 2 == 0))
            .collect();
        let stages = fallback_repaired(&nodes, &intent);
        assert_eq!(stages.len(), 3);
        let total: usize = stages.iter().map(|s| s.topics.len()).sum();
        assert_eq!(total, 9, "降级方案必须覆盖全部知识点");
        assert!(!validate_stages(&stages, &nodes).iter().any(|i| i.contains("必学")));
    }

    #[test]
    fn schedule_expands_weeks_with_cap() {
        let mut stages = vec![
            stage(0, "a", 1, &["A"]),
            stage(1, "b", 2, &["B"]),
        ];
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
        assert_eq!(r[0].items, vec!["The Book".to_string(), "Rustlings".to_string()]);
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
            resources: vec![StageResource { stage: 0, items: vec!["The Book".into()] }],
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
