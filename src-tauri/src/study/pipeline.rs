//! 学习规划流水线编排与检查点(intent → map → stages → resources ∥ schedule → validate → export)。
//!
//! LLM 调用/事件流/暂停终止复用研究引擎同款设施(Provider + Channel + AtomicBool)。

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tauri::ipc::Channel;

use crate::provider::{ChatMessage, ChatRequest, Provider, StreamChunk};

use super::schedule::{
    calc_schedule, default_questions, fix_intent, missing_fields, salvage_intent,
};
use super::template;
use super::validate::{
    extract_json, fallback_repaired, fix_stages, parse_nodes, parse_resources, parse_stages,
    uncovered_nodes, validate_stages,
};
use super::{
    Intent, IntentHints, NodeSpec, PlanPayload, StageResource, StageSpec, StudyEvent, store,
};

mod util;

pub(crate) use util::*;

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
    pub rebuild: Option<crate::provider::rebuild::ProviderRebuild>,
    pub fallback: std::sync::Mutex<Option<Arc<dyn Provider>>>,
    /// P2:生成每日任务(创建入口勾选,默认开)
    pub daily_tasks: bool,
    /// P3:预生成每个知识点的完整自学材料(创建入口勾选,默认关)
    pub full_content: bool,
}

pub(crate) enum RunError {
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

pub(crate) fn check_cancel(p: &StudyParams) -> Result<(), RunError> {
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

// ---------------- 意图 ----------------

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
    let intent: Intent =
        match extract_json(&text).and_then(|j| serde_json::from_str::<Intent>(&j).ok()) {
            Some(i) => i,
            None => salvage_intent(&text),
        };
    let mut intent = fix_intent(
        intent,
        hints.daily_minutes.unwrap_or(30),
        hints.deadline.clone(),
    );
    intent.topic = if intent.topic.is_empty() {
        topic.to_string()
    } else {
        intent.topic
    };
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
    // 追问与缺失字段必须一一对应(前端按 missing[i] 决定控件类型):
    // 模型的 questions 措辞/数量不可靠,统一用确定性生成覆盖。
    intent.questions = if intent.missing.is_empty() {
        Vec::new()
    } else {
        default_questions(&intent.missing)
    };

    Ok(intent)
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

// ---------------- 主流程 ----------------

/// 运行一次规划流水线(后台任务)。负责终态落库与事件收尾。
pub async fn run(p: StudyParams) {
    let sink = Sink {
        id: p.id.clone(),
        channel: p.channel.clone(),
    };
    match run_inner(&p, &sink).await {
        Ok(payload) => {
            let _ = store::save_payload(&p.id, &payload);
            let _ = store::finish_plan(&p.id, &payload);
            // P2 每日任务:确定性切分(失败不阻塞计划就绪)
            if p.daily_tasks {
                match crate::study::learn::tasks::generate_for_plan(&p.id, &payload) {
                    Ok(n) => sink.emit(StudyEvent::StepNote {
                        step: "export".into(),
                        content: format!("已按每天 {} 分钟生成 {n} 个每日任务", payload.daily_minutes),
                    }),
                    Err(e) => sink.emit(StudyEvent::StepNote {
                        step: "export".into(),
                        content: format!("每日任务生成失败(不影响计划):{e}"),
                    }),
                }
            }
            // P3 完整教学内容:逐节点生成自学材料(用户创建时勾选)
            // 逐条串行、失败不中断;取消/暂停沿用流水线控制,已完成部分保留。
            if p.full_content {
                if let Ok(nodes) = store::node_ids(&p.id) {
                    if !nodes.is_empty() {
                        sink.emit(StudyEvent::StepStart {
                            step: "content".into(),
                            label: format!("生成完整教学内容({} 个知识点)", nodes.len()),
                        });
                        sink.emit(StudyEvent::StepNote {
                            step: "content".into(),
                            content: format!(
                                "开始为 {} 个知识点生成完整教学内容(逐条生成,可随时取消,已完成部分会保留)",
                                nodes.len()
                            ),
                        });
                        let done = generate_all_units(&p, &sink, &nodes).await;
                        sink.emit(StudyEvent::StepNote {
                            step: "content".into(),
                            content: format!(
                                "完整教学内容已生成 {done}/{} 个知识点;其余可在计划详情中逐个生成",
                                nodes.len()
                            ),
                        });
                    }
                }
            }
            sink.emit(StudyEvent::Done {
                plan: Some(payload),
            });
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

/// P3:逐节点生成完整自学材料。暂停会挂起,取消则停止;单节点失败不中断整体。
/// 返回值 = 实际成功生成的节点数。
async fn generate_all_units(
    p: &StudyParams,
    sink: &Sink,
    nodes: &[(i64, String, i64)],
) -> usize {
    let total = nodes.len();
    let mut done = 0usize;
    for (i, (node_id, name, difficulty)) in nodes.iter().enumerate() {
        if p.cancel.load(Ordering::Relaxed) {
            break;
        }
        while p.paused.load(Ordering::Relaxed) && !p.cancel.load(Ordering::Relaxed) {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        if p.cancel.load(Ordering::Relaxed) {
            break;
        }
        let est = 20 + (*difficulty).clamp(0, 5) as u32 * 10;
        sink.emit(StudyEvent::StepNote {
            step: "content".into(),
            content: format!(
                "正在生成第 {}/{} 个知识点的完整内容:{}(约 {est} 分钟)…",
                i + 1,
                total,
                name
            ),
        });
        match crate::study::learn::generate_unit(
            &*p.provider,
            &p.model,
            &p.id,
            *node_id,
            false,
            false,
        )
        .await
        {
            Ok(_) => done += 1,
            Err(e) => sink.emit(StudyEvent::StepNote {
                step: "content".into(),
                content: format!("「{name}」生成失败,可稍后在计划详情中单独重新生成:{e}"),
            }),
        }
    }
    done
}

async fn run_inner(p: &StudyParams, sink: &Sink) -> Result<PlanPayload, RunError> {
    let lang_label = if p.lang == "en" { "English" } else { "中文" };

    // ① intent
    sink.emit(StudyEvent::StepStart {
        step: "intent".into(),
        label: "对齐学习意图".into(),
    });
    gate(p, sink).await?;
    let intent = resolve_intent(p)?;
    sink.emit(StudyEvent::StepNote {
        step: "intent".into(),
        content: format!(
            "主题:{} · 水平:{} · 每天 {} 分钟{}{}",
            intent.topic,
            if intent.level.is_empty() {
                "未指定"
            } else {
                &intent.level
            },
            intent.daily_minutes,
            intent
                .deadline
                .as_ref()
                .map(|d| format!(" · 截止 {d}"))
                .unwrap_or_default(),
            if intent.subject.is_empty() {
                String::new()
            } else {
                let band = super::subject::grade_band_label(&intent.grade_band);
                format!(
                    " · 学科:{}{}",
                    intent.subject,
                    if band == "未指定" {
                        String::new()
                    } else {
                        format!("({band})")
                    }
                )
            },
        ),
    });

    // ② map
    sink.emit(StudyEvent::StepStart {
        step: "map".into(),
        label: "生成知识地图".into(),
    });
    gate(p, sink).await?;
    let nodes = run_map(p, sink, &intent, lang_label).await?;

    // ③ stages(校验重试 ≤3 + 降级)
    sink.emit(StudyEvent::StepStart {
        step: "stages".into(),
        label: "设计学习阶段".into(),
    });
    gate(p, sink).await?;
    let (mut stages, repaired) = run_stages(p, sink, &intent, &nodes, lang_label).await?;

    // ④ resources ∥ schedule
    sink.emit(StudyEvent::StepStart {
        step: "resources".into(),
        label: "推荐学习资料".into(),
    });
    sink.emit(StudyEvent::StepStart {
        step: "schedule".into(),
        label: "计算周排期".into(),
    });
    gate(p, sink).await?;
    let resources_fut = run_resources(p, sink, &stages, &intent, lang_label);
    let schedule_fut = {
        let stages = stages.clone();
        async move { calc_schedule(&stages, intent.daily_minutes) }
    };
    let (resources, schedule) = futures::join!(resources_fut, schedule_fut);
    sink.emit(StudyEvent::StepNote {
        step: "schedule".into(),
        content: format!(
            "共 {} 周,每天约 {} 分钟",
            schedule.len(),
            intent.daily_minutes
        ),
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
    sink.emit(StudyEvent::StepStart {
        step: "validate".into(),
        label: "校验计划结构".into(),
    });
    gate(p, sink).await?;
    // 兜底:万一分组仍有遗漏(理论上 run_stages 已保证),把未覆盖节点追加到末阶段
    if let Some(extra) = uncovered_nodes(&nodes, &stages)
        && let Some(last) = stages.last_mut()
    {
        last.topics.extend(extra.iter().cloned());
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
    sink.emit(StudyEvent::StepStart {
        step: "export".into(),
        label: "保存学习计划".into(),
    });
    gate(p, sink).await?;
    let payload = PlanPayload {
        topic: intent.topic.clone(),
        level: intent.level.clone(),
        goal: intent.goal.clone(),
        daily_minutes: intent.daily_minutes,
        deadline: intent.deadline.clone(),
        lang: intent.lang.clone(),
        subject: intent.subject.clone(),
        grade_band: intent.grade_band.clone(),
        exam_target: intent.exam_target.clone(),
        preferences: intent.preferences.clone(),
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

/// 步骤 ②:知识地图。
async fn run_map(
    p: &StudyParams,
    sink: &Sink,
    intent: &Intent,
    lang_label: &str,
) -> Result<Vec<NodeSpec>, RunError> {
    let profile = template::LearnerProfile::from_intent(intent);
    let sys = format!("{}{}", template::effective("study_map"), profile.appendix());
    let user = format!(
        "{}学习主题:{}\n当前水平:{}\n学习目标:{}\n输出语言:{lang_label}",
        profile.config_block(),
        intent.topic,
        intent.level,
        intent.goal
    );
    let mut last_err = String::new();
    for attempt in 1..=3u32 {
        gate(p, sink).await?;
        // P3 反馈:调用模型前发进行中文案,避免静默期
        sink.emit(StudyEvent::StepNote {
            step: "map".into(),
            content: format!("正在调用模型绘制知识地图(第 {attempt}/3 次尝试)…"),
        });
        let text = llm_collect(p, vec![sys_msg(sys.clone()), user_msg(user.clone())]).await?;
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
            sink.emit(StudyEvent::Retry {
                step: "map".into(),
                attempt,
                reason: last_err.clone(),
            });
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
    let profile = template::LearnerProfile::from_intent(intent);
    let sys = format!(
        "{}{}",
        template::effective("study_stages"),
        profile.appendix()
    );
    let nodes_digest = serde_json::to_string(
        &nodes
            .iter()
            .map(|n| {
                serde_json::json!({
                    "name": n.name,
                    "difficulty": n.difficulty,
                    "must_learn": n.must_learn,
                    "prereqs": n.prereqs,
                })
            })
            .collect::<Vec<_>>(),
    )
    .map_err(|e| RunError::Fatal(e.to_string()))?;
    let base_user = format!(
        "{}学习主题:{}\n输出语言:{lang_label}\n知识地图(只能引用其中的 name):\n{nodes_digest}",
        profile.config_block(),
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
        // P3 反馈:调用模型前发进行中文案,避免静默期
        sink.emit(StudyEvent::StepNote {
            step: "stages".into(),
            content: format!("正在调用模型设计学习阶段(第 {attempt}/3 次尝试)…"),
        });
        let text = llm_collect(p, vec![sys_msg(sys.clone()), user_msg(user)]).await?;
        if let Some(json) = extract_json(&text)
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&json)
        {
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
        feedback = "- 输出不是合法的 {\"stages\":[...]} JSON,必须只输出该结构".into();
        if attempt < 3 {
            sink.emit(StudyEvent::Retry {
                step: "stages".into(),
                attempt,
                reason: feedback.clone(),
            });
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
    sink: &Sink,
    stages: &[StageSpec],
    intent: &Intent,
    lang_label: &str,
) -> Vec<StageResource> {
    // P3 反馈:资料匹配期间给出进行中文案
    sink.emit(StudyEvent::StepNote {
        step: "resources".into(),
        content: format!("正在为 {} 个阶段匹配学习资料…", stages.len()),
    });
    let profile = template::LearnerProfile::from_intent(intent);
    let sys = format!(
        "{}{}",
        template::effective("study_resources"),
        profile.appendix()
    );
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
    let user = format!(
        "{}学习阶段列表:\n{digest}\n输出语言:{lang_label}",
        profile.config_block()
    );
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

/// 非流式便捷调用(preflight / P1 学习单元与测验生成用)。
pub(crate) async fn llm_plain(
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
        ..Default::default()
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
            _ => {}
        }
    }
    Ok(out)
}
