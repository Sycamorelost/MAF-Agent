//! 终稿生成、审计附录与引用列表收尾。

use crate::provider::{ChatMessage, ChatRequest, StreamChunk};

use super::EngineParams;
use super::events::{EventSink, ResearchEvent, SourceItem};
use super::llm::{chat_stream_compat, llm_collect};
use super::params::with_references;
use super::pipeline::{RunError, check_cancel};
use super::prompts::sources_digest;
use super::skills;

pub(crate) async fn run_report(
    p: &EngineParams,
    sink: &EventSink,
    sources: &[SourceItem],
    stages_digest: &str,
    guard_reason: Option<&str>,
) -> Result<String, RunError> {
    use futures::StreamExt;

    // 终稿 system:自由结构开关生效(free_structure 忽略模板);模板含 {references} 时留给 finalize
    let (mut system, ref_anchor) = p.params.report_system(&p.topic, stages_digest);
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
    // 用户导入的参考资料:终稿撰写的事实依据
    let system = with_references(system, p);
    let max_total = if p.params.read_length == "full" {
        24000
    } else {
        16000
    };
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
                ..Default::default()
            },
            ChatMessage {
                role: "user".into(),
                content: user,
                images: vec![],
                ..Default::default()
            },
        ],
        temperature: Some(0.3),
        max_tokens: None,
        top_p: None,
        reasoning: None, // 研究链路档位透传为 Phase 2,暂不启用
        stream: true,
        ..Default::default()
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
            _ => {}
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
pub(crate) async fn run_audit(
    p: &EngineParams,
    audit_system: String,
    report: &str,
) -> Result<String, RunError> {
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
    Ok(text)
}

/// 清洗模型输出:去代码块包裹、截断模型自写的资料来源节,追加系统生成的引用列表。
/// ref_anchor=true 且模型原样输出了 {references} 时,引用列表插入锚点位置;否则追加到文末。
pub(crate) fn finalize_report(raw: &str, sources: &[SourceItem], ref_anchor: bool) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalize_report_replaces_sources_section() {
        let raw =
            "# 《x》研究报告\n\n## TL;DR\n\n结论 [1]\n\n## 资料来源\n\n1. [假](http://fake)\n";
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
        assert!(
            out.contains("## 正文\n\n结论 [1]\n\n## 资料来源\n\n1. [来源甲](https://a.example)")
        );
        // 模板无锚点 → 维持追加到文末
        let out2 = finalize_report(raw, &sources, false);
        assert!(out2.contains("{references}\n\n## 资料来源"));
    }
}
