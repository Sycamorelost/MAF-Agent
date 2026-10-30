//! 内置智能体系统提示词注册表。
//!
//! 每个智能体的提示词 = 独立 md 文件（`agents_prompts/<id>.md`）,
//! 本模块只负责用 `include_str!` 在编译期加载并按 id 分发。
//! 新增/修改智能体提示词:改 md 文件即可,无需改本模块。
//!
//! 统一约定:每条提示词 = 角色 / 能力边界 / 工作流程 / 输出格式 / 失败处理 五段,
//! 不写空泛的"你是一个 helpful assistant"。
//!
//! 调研依据(详见 val.md):
//! - Anthropic《Effective context engineering for AI agents》:分段组织;
//! - GitHub `x1xhlol/system-prompts-and-models-of-ai-tools` 与 CL4R1T4S 生产级 prompt 共性。

/// 按 agent_id 取内置提示词;未知 id 返回空串(调用方已有非空校验)。
///
/// 只保留 `agents::builtin_templates()` 实际注册的 22 个 id;
/// 未注册的提示词分支已删除,避免"有提示词、无入口"的漂移。
pub fn prompt_for(agent_id: &str) -> &'static str {
    match agent_id {
        "product-assistant" => include_str!("agents_prompts/product-assistant.md"),
        "chinese-assistant" => include_str!("agents_prompts/chinese-assistant.md"),
        "meeting-minutes" => include_str!("agents_prompts/meeting-minutes.md"),
        "doc-analyst" => include_str!("agents_prompts/doc-analyst.md"),
        "code-reviewer" => include_str!("agents_prompts/code-reviewer.md"),
        "bug-hunter" => include_str!("agents_prompts/bug-hunter.md"),
        "deep-researcher" => include_str!("agents_prompts/deep-researcher.md"),
        "translator" => include_str!("agents_prompts/translator.md"),
        "compliance-check" => include_str!("agents_prompts/compliance-check.md"),
        "report-writer" => include_str!("agents_prompts/report-writer.md"),
        "bare-assistant" => include_str!("agents_prompts/bare-assistant.md"),
        "feynman-coach" => include_str!("agents_prompts/feynman-coach.md"),
        "paper-reader" => include_str!("agents_prompts/paper-reader.md"),
        "email-drafter" => include_str!("agents_prompts/email-drafter.md"),
        "weekly-report" => include_str!("agents_prompts/weekly-report.md"),
        "exp-designer" => include_str!("agents_prompts/exp-designer.md"),
        "data-analyst" => include_str!("agents_prompts/data-analyst.md"),
        "lab-report-writer" => include_str!("agents_prompts/lab-report-writer.md"),
        "ai-flavor-detector" => include_str!("agents_prompts/ai-flavor-detector.md"),
        "ai-flavor-rewriter" => include_str!("agents_prompts/ai-flavor-rewriter.md"),
        "academic-voice" => include_str!("agents_prompts/academic-voice.md"),
        "style-anchor" => include_str!("agents_prompts/style-anchor.md"),
        _ => "",
    }
}
