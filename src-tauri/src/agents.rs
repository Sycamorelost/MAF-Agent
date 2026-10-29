//! Agent 模板(迭代 B2 → R3 A3/B4 → R4 收口):22 个内置模板 + 用户变体/自定义模板。
//!
//! 对应 idea/07-agent-templates.md(已按 2026-10-13 决策舍弃 mcp_tools 字段)。
//! 内置模板硬编码;用户创建的变体/自定义模板落盘 `data/agents.json`。
//! 记忆读写矩阵:memory_strategy 驱动注入策略(full/summary/none),
//! memory_write 决定是否参与自动提炼,memory_sections 标注写入哪个节(展示用)。

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::agents_prompts::prompt_for;

/// Agent 模板(内置与自定义共用同一结构)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTemplate {
    pub agent_id: String,
    pub name: String,
    /// 图标 key,前端映射到线性 SVG(禁 emoji)
    pub icon: String,
    pub description: String,
    /// chat / doc / dev / create / ops / meta
    pub category: String,
    pub recommended_model: String,
    /// 记忆注入策略:full / summary / none(绑定会话时写入 sessions.memory_injection_strategy)
    pub memory_strategy: String,
    pub system_prompt: String,
    #[serde(default)]
    pub input_hints: Vec<String>,
    /// 是否参与记忆自动提炼(写记忆)
    #[serde(default)]
    pub memory_write: bool,
    /// 提炼结果写入 memory.md 的哪些节(展示用)
    #[serde(default)]
    pub memory_sections: Vec<String>,
    /// 变体的基座模板 id(非变体为 None)
    #[serde(default)]
    pub base_agent: Option<String>,
    /// 是否内置(内置不可删除/覆盖);JSON 中缺省 false
    #[serde(default)]
    pub builtin: bool,
    // —— v2 字段(R3,val.md §4.3):全部有默认,旧 agents.json 兼容 ——
    /// 开场白:空会话首条展示(None = 不展示)
    #[serde(default)]
    pub opening_message: String,
    /// 输入框追问建议(chips)
    #[serde(default)]
    pub suggestions: Vec<String>,
    /// 工具策略:None=继承全局;Some(空)=禁用工具;Some(列表)=仅允许名单内工具
    #[serde(default)]
    pub tool_policy: Option<Vec<String>>,
    /// 思考档位偏好:空 = 跟随用户设置;off / low / medium / high
    #[serde(default)]
    pub thinking_level: String,
    /// 采样温度偏好(None = 跟随模型默认)
    #[serde(default)]
    pub temperature: Option<f32>,
    /// 附件文档管线:inline(直接注入,现行为)/ resident(驻留+检索,待 Phase C)/ off
    #[serde(default)]
    pub doc_pipeline: String,
    /// 触发词(Skill 层,命中时提示切换智能体;Phase A 仅存储)
    #[serde(default)]
    pub triggers: Vec<String>,
}

/// 合法分类(R3 A3 扩展:生活/学习/工作/学术四大方向)。
pub const CATEGORIES: [&str; 10] = [
    "chat", "doc", "dev", "create", "ops", "meta", "life", "study", "work", "academic",
];

/// v2 新增字段的空默认:保持 t() 原签名,内置模板仅显式覆盖需要的字段。
fn v2_defaults() -> AgentTemplate {
    AgentTemplate {
        agent_id: String::new(),
        name: String::new(),
        icon: String::new(),
        description: String::new(),
        category: String::new(),
        recommended_model: String::new(),
        memory_strategy: "full".into(),
        system_prompt: String::new(),
        input_hints: Vec::new(),
        memory_write: false,
        memory_sections: Vec::new(),
        base_agent: None,
        builtin: false,
        opening_message: String::new(),
        suggestions: Vec::new(),
        tool_policy: None,
        thinking_level: String::new(),
        temperature: None,
        doc_pipeline: String::new(),
        triggers: Vec::new(),
    }
}

/// 22 个内置模板(R4 决策:正式收缩为 22 个,未注册的提示词分支已删除)。
/// 分类:chat 3 + doc 4 + dev 2 + create 2 + ops 1 + meta 2 + study 2 + work 2 + academic 4 = 22。
pub fn builtin_templates() -> Vec<AgentTemplate> {
    let t = |agent_id: &str,
             name: &str,
             icon: &str,
             category: &str,
             description: &str,
             recommended_model: &str,
             memory_strategy: &str,
             system_prompt: &str,
             input_hints: &[&str],
             memory_write: bool,
             memory_sections: &[&str]| AgentTemplate {
        agent_id: agent_id.into(),
        name: name.into(),
        icon: icon.into(),
        description: description.into(),
        category: category.into(),
        recommended_model: recommended_model.into(),
        memory_strategy: memory_strategy.into(),
        system_prompt: system_prompt.into(),
        input_hints: input_hints.iter().map(|s| s.to_string()).collect(),
        memory_write,
        memory_sections: memory_sections.iter().map(|s| s.to_string()).collect(),
        base_agent: None,
        builtin: true,
        ..v2_defaults()
    };
    vec![
        // 一、Chat 类（3 个核心）
        t(
            "product-assistant",
            "产品助手",
            "sparkles",
            "chat",
            "日常对话、产品设计、通用问答",
            "gpt-5.2",
            "full",
            prompt_for("product-assistant"),
            &["继续对话…", "帮我分析这份 PDF", "查一下最新行业动态"],
            true,
            &["会话摘要", "关键事实与结论", "待办"],
        ),
        t(
            "chinese-assistant",
            "中文优化助手",
            "languages",
            "chat",
            "中文表达优化、术语统一、文风调整",
            "qwen-max",
            "full",
            prompt_for("chinese-assistant"),
            &["优化这段中文", "把这段话改得更正式", "统一术语"],
            true,
            &[],
        ),
        t(
            "meeting-minutes",
            "会议纪要生成器",
            "clipboard-list",
            "chat",
            "会议录音转写 → 结构化纪要 → 自动提炼到 memory.md",
            "claude-sonnet-4-5",
            "summary",
            prompt_for("meeting-minutes"),
            &["输入转写文本…", "生成纪要", "提取行动项"],
            true,
            &["待办", "关键事实与结论"],
        ),
        // 二、Doc 类（3 个核心 + 2 个去 AI 味）
        t(
            "doc-analyst",
            "文档分析师",
            "file-text",
            "doc",
            "上传文档 → 自动解析 → 提炼要点 → 写入 memory.md",
            "claude-sonnet-4-5",
            "full",
            prompt_for("doc-analyst"),
            &["上传文档开始分析", "这份 PDF 的核心论点"],
            true,
            &["文档摘要"],
        ),
        t(
            "report-writer",
            "报告撰写助手",
            "note",
            "doc",
            "素材 → 结构化报告(背景/方法/结论/建议)",
            "gpt-5.2",
            "full",
            prompt_for("report-writer"),
            &["根据这些素材写周报", "整理成调研报告"],
            true,
            &["文档摘要", "关键事实与结论"],
        ),
        // 三、Dev 类（2 个核心）
        t(
            "code-reviewer",
            "代码审查员",
            "git-pull-request",
            "dev",
            "diff → 逐行审查 → 结构化问题清单",
            "gpt-5.2",
            "none",
            prompt_for("code-reviewer"),
            &["审查当前分支", "对比 main 改动", "检查这段代码"],
            false,
            &[],
        ),
        t(
            "bug-hunter",
            "Bug 猎人",
            "bug",
            "dev",
            "给定报错日志 → 定位根因 → 给出修复 diff",
            "gpt-5.2",
            "none",
            prompt_for("bug-hunter"),
            &["这个报错怎么回事", "React useEffect 无限循环"],
            false,
            &[],
        ),
        // 四、Create 类（2 个核心）
        t(
            "deep-researcher",
            "深度研究员",
            "search",
            "create",
            "多轮检索 + 反思 + 综合输出(独立研究管线)",
            "deepseek-v3",
            "summary",
            prompt_for("deep-researcher"),
            &["研究主题是什么?", "竞品深挖"],
            true,
            &["资料", "待办"],
        ),
        t(
            "translator",
            "翻译专家",
            "languages",
            "create",
            "专业翻译 + 术语一致 + 格式保留",
            "glm-translate-4",
            "none",
            prompt_for("translator"),
            &["翻译成中文", "翻译成英文", "这份技术文档转英文"],
            false,
            &[],
        ),
        // 五、Ops 类（1 个核心）
        t(
            "compliance-check",
            "合规检查员",
            "shield-check",
            "ops",
            "检查敏感信息 + 合规风险",
            "ollama-llama3",
            "none",
            prompt_for("compliance-check"),
            &["检查 src/ 合规", "找硬编码密钥"],
            false,
            &[],
        ),
        // 六、Meta 类（1 个核心：裸助手 = 默认预制）
        t(
            "bare-assistant",
            "默认助手",
            "bot",
            "meta",
            "无人格预设，仅物理契约，跟随用户指令",
            "",
            "full",
            prompt_for("bare-assistant"),
            &["直接开始任务", "回答我的问题", "帮我处理这段文本"],
            true,
            &[],
        ),
        // 七、学习类（2 个核心）
        t(
            "feynman-coach",
            "费曼讲解",
            "edit",
            "study",
            "让用户先讲，纠偏+类比",
            "",
            "full",
            prompt_for("feynman-coach"),
            &["我用三句话讲讲这个概念", "帮我检查哪里讲错了", "再给个类比"],
            true,
            &["会话摘要", "待办"],
        ),
        t(
            "paper-reader",
            "文献精读",
            "file-text",
            "study",
            "逐节解读+批判性问题清单，引用定位",
            "",
            "summary",
            prompt_for("paper-reader"),
            &["精读这篇论文", "这个方法的局限是什么", "帮我列批判性问题"],
            true,
            &[],
        ),
        // 八、工作类（2 个核心）
        t(
            "email-drafter",
            "邮件起草",
            "edit",
            "work",
            "分语气版本，收件人视角检查",
            "",
            "none",
            prompt_for("email-drafter"),
            &[
                "给客户写一封催款邮件",
                "委婉拒绝这个请求",
                "把这封邮件改正式",
            ],
            false,
            &[],
        ),
        t(
            "weekly-report",
            "周报助手",
            "clipboard-list",
            "work",
            "STAR 结构，量化留白「待补充」",
            "",
            "summary",
            prompt_for("weekly-report"),
            &["把这些流水整理成周报", "补充风险与求助", "下周计划怎么写"],
            true,
            &["会话摘要", "待办"],
        ),
        // 九、学术类（3 个核心）
        t(
            "exp-designer",
            "实验设计",
            "research",
            "academic",
            "变量/对照/样本量框架，提醒伦理审查",
            "",
            "full",
            prompt_for("exp-designer"),
            &["帮我设计对照组", "样本量怎么估", "这个实验有哪些混杂因素"],
            true,
            &["实验记录", "关键事实与结论"],
        ),
        t(
            "data-analyst",
            "数据处理",
            "git-compare",
            "academic",
            "统计方法选择+代码步骤，不编造数值",
            "",
            "summary",
            prompt_for("data-analyst"),
            &["这组数据用什么检验", "帮我写清洗脚本", "结果应该怎么解读"],
            true,
            &["实验记录", "关键事实与结论"],
        ),
        t(
            "lab-report-writer",
            "实验报告撰写",
            "doc",
            "academic",
            "背景-方法-结果-讨论四段成文",
            "",
            "full",
            prompt_for("lab-report-writer"),
            &["把实验记录整理成报告", "方法节要写多细", "讨论部分怎么写"],
            true,
            &["实验记录", "关键事实与结论"],
        ),
        // —— R3 B4：去 AI 味 · AI 生成概率分析 专用智能体（4 个，val.md §5.4）——
        // tool_policy/doc_pipeline 按 §5.4 表：detector/rewriter/voice 走 inline，anchor 全离线。
        AgentTemplate {
            tool_policy: Some(vec!["fs_read".into()]),
            doc_pipeline: "inline".into(),
            opening_message:
                "把要判定的文本发我，我会基于本地统计结果逐句说明「哪里像 AI、为什么」。".into(),
            suggestions: vec![
                "解读这份检测报告".into(),
                "哪些句子最像 AI".into(),
                "改写优先级怎么排".into(),
            ],
            ..t(
                "ai-flavor-detector",
                "AI 味检测官",
                "search",
                "doc",
                "解读本地统计结果，输出证据化的句子级研判报告",
                "",
                "none",
                prompt_for("ai-flavor-detector"),
                &["解读这份检测报告", "哪些句子最像 AI", "改写优先级怎么排"],
                false,
                &[],
            )
        },
        AgentTemplate {
            tool_policy: Some(vec!["fs_read".into(), "fs_write".into()]),
            doc_pipeline: "inline".into(),
            opening_message:
                "发我一段文本，我按轻度/中度/深度三档改成人味表达，事实与观点一律不动。".into(),
            suggestions: vec![
                "轻度改写这段文字".into(),
                "按我的风格锚点深度重写".into(),
                "只去套话别动结构".into(),
            ],
            ..t(
                "ai-flavor-rewriter",
                "去 AI 味改写师",
                "edit",
                "doc",
                "三档人味化改写：轻度去套话 / 中度调节奏 / 深度按锚点重写",
                "",
                "none",
                prompt_for("ai-flavor-rewriter"),
                &[
                    "轻度改写这段文字",
                    "按我的风格锚点深度重写",
                    "只去套话别动结构",
                ],
                false,
                &[],
            )
        },
        AgentTemplate {
            tool_policy: Some(vec!["fs_read".into()]),
            doc_pipeline: "resident".into(),
            opening_message:
                "粘贴论文片段，我帮你提升学术语体，并生成可直接放进 Methods 的 AI 使用披露声明。"
                    .into(),
            suggestions: vec![
                "润色这段方法学描述".into(),
                "生成 AI 使用披露声明".into(),
                "这句表述合规吗".into(),
            ],
            ..t(
                "academic-voice",
                "学术合规润色",
                "file-text",
                "academic",
                "学术语体与术语统一 + AI 使用披露声明（对照 Nature/Science 政策）",
                "",
                "none",
                prompt_for("academic-voice"),
                &[
                    "润色这段方法学描述",
                    "生成 AI 使用披露声明",
                    "这句表述合规吗",
                ],
                false,
                &[],
            )
        },
        AgentTemplate {
            tool_policy: Some(vec![]),
            doc_pipeline: "off".into(),
            opening_message: "贴上 1-3 段你自己写过的旧文，我会提炼成一张可复用的风格锚点卡。"
                .into(),
            suggestions: vec![
                "分析这段文字的风格特征".into(),
                "提炼我的写作指纹".into(),
                "生成风格锚点卡".into(),
            ],
            ..t(
                "style-anchor",
                "风格档案员",
                "shield-check",
                "meta",
                "从你的旧作提炼个人写作指纹，作为改写锚点",
                "",
                "none",
                prompt_for("style-anchor"),
                &[
                    "分析这段文字的风格特征",
                    "提炼我的写作指纹",
                    "生成风格锚点卡",
                ],
                false,
                &[],
            )
        },
    ]
}

fn custom_path() -> PathBuf {
    crate::paths::data_dir().join("agents.json")
}

/// 读取用户自定义/变体模板;文件缺失/损坏时返回空(不报错,本地优先)。
pub fn load_custom() -> Vec<AgentTemplate> {
    let Ok(text) = fs::read_to_string(custom_path()) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_custom(list: &[AgentTemplate]) -> Result<(), String> {
    let p = custom_path();
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {} failed: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    fs::write(&p, text).map_err(|e| format!("write {} failed: {e}", p.display()))
}

/// 全部模板 = 内置 + 自定义(自定义 id 与内置冲突时忽略自定义,防覆盖)。
pub fn list_all() -> Vec<AgentTemplate> {
    let builtins = builtin_templates();
    let ids: Vec<String> = builtins.iter().map(|t| t.agent_id.clone()).collect();
    let mut out = builtins;
    for mut c in load_custom() {
        if ids.contains(&c.agent_id) {
            continue;
        }
        c.builtin = false;
        out.push(c);
    }
    out
}

/// 按 id 查模板(内置优先)。
pub fn find(agent_id: &str) -> Option<AgentTemplate> {
    list_all().into_iter().find(|t| t.agent_id == agent_id)
}

/// 渲染 system_prompt 模板变量(迭代 B4 / F2):
/// `{date}` → 今天日期(YYYY-MM-DD);`{model}` → 会话当前模型 id;`{language}` → 中文。
/// 未知变量原样保留;绑定会话或「应用为提示词」时渲染一次后落盘。
pub fn render_prompt(template: &str, model: &str) -> String {
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    template
        .replace("{date}", &date)
        .replace("{model}", model)
        .replace("{language}", "中文")
}

/// 新增/更新自定义模板或变体。校验:id 非空且不与内置冲突;分类合法;
/// 变体的 base_agent 必须存在。
pub fn upsert_custom(agent: AgentTemplate) -> Result<(), String> {
    if agent.agent_id.trim().is_empty() {
        return Err("agent_id 不能为空".into());
    }
    if !CATEGORIES.contains(&agent.category.as_str()) {
        return Err(format!(
            "非法分类:{}(可选:{})",
            agent.category,
            CATEGORIES.join("/")
        ));
    }
    if !["full", "summary", "none"].contains(&agent.memory_strategy.as_str()) {
        return Err(format!("非法 memory_strategy:{}", agent.memory_strategy));
    }
    if builtin_templates()
        .iter()
        .any(|b| b.agent_id == agent.agent_id)
    {
        return Err(format!("不能覆盖内置模板:{}", agent.agent_id));
    }
    if let Some(base) = &agent.base_agent
        && find(base).is_none()
    {
        return Err(format!("变体基座不存在:{base}"));
    }
    let mut list = load_custom();
    let mut agent = agent;
    agent.builtin = false;
    if let Some(x) = list.iter_mut().find(|x| x.agent_id == agent.agent_id) {
        *x = agent;
    } else {
        list.push(agent);
    }
    save_custom(&list)
}

/// 删除自定义模板;内置模板拒绝删除。
pub fn remove_custom(agent_id: &str) -> Result<(), String> {
    if builtin_templates().iter().any(|b| b.agent_id == agent_id) {
        return Err(format!("内置模板不可删除:{agent_id}"));
    }
    let list = load_custom();
    let next: Vec<_> = list
        .into_iter()
        .filter(|t| t.agent_id != agent_id)
        .collect();
    save_custom(&next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_has_22_unique_agents() {
        let list = builtin_templates();
        assert_eq!(list.len(), 22);
        let mut ids: Vec<&str> = list.iter().map(|t| t.agent_id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 22, "agent_id 必须唯一");
        for t in &list {
            assert!(
                CATEGORIES.contains(&t.category.as_str()),
                "非法分类:{}",
                t.category
            );
            assert!(["full", "summary", "none"].contains(&t.memory_strategy.as_str()));
            assert!(t.builtin);
            assert!(
                !t.system_prompt.trim().is_empty(),
                "{} 缺 system_prompt",
                t.agent_id
            );
            assert!(!t.input_hints.is_empty(), "{} 缺 input_hints", t.agent_id);
        }
        // R4:内置模板收缩为 22 个后,不再要求每个分类至少 4 个;
        // 只校验分类分布与设计一致,防止后续增删时悄悄漂移。
        let expect = [
            ("chat", 3),
            ("doc", 4),
            ("dev", 2),
            ("create", 2),
            ("ops", 1),
            ("meta", 2),
            ("study", 2),
            ("work", 2),
            ("academic", 4),
        ];
        for (c, want) in expect {
            let n = list.iter().filter(|t| t.category == c).count();
            assert_eq!(n, want, "分类 {c} 应有 {want} 个内置模板");
        }
        let bare = list
            .iter()
            .find(|t| t.agent_id == "bare-assistant")
            .expect("缺少 bare-assistant");
        assert_eq!(bare.category, "meta");
        assert_eq!(bare.memory_strategy, "full");
    }

    /// R4:每个内置 id 都必须有对应提示词,且提示词表不再有多余分支。
    #[test]
    fn builtin_prompts_match_templates() {
        for t in builtin_templates() {
            assert!(
                !crate::agents_prompts::prompt_for(&t.agent_id).is_empty(),
                "{} 缺少提示词",
                t.agent_id
            );
        }
        assert!(
            crate::agents_prompts::prompt_for("diff-reader").is_empty(),
            "未注册的 diff-reader 不应再有提示词"
        );
    }

    /// R3 B4:去 AI 味 4 专用智能体,tool_policy/doc_pipeline 按 val.md §5.4 表。
    #[test]
    fn builtin_aiflavor_agents_policy() {
        let list = builtin_templates();
        let expect = [
            ("ai-flavor-detector", vec!["fs_read"], "inline"),
            ("ai-flavor-rewriter", vec!["fs_read", "fs_write"], "inline"),
            ("academic-voice", vec!["fs_read"], "resident"),
            ("style-anchor", vec![], "off"),
        ];
        for (id, tools, pipeline) in expect {
            let t = list
                .iter()
                .find(|t| t.agent_id == id)
                .unwrap_or_else(|| panic!("缺少 {id}"));
            let want: Vec<String> = tools.iter().map(|s| s.to_string()).collect();
            assert_eq!(
                t.tool_policy.as_deref(),
                Some(want.as_slice()),
                "{id} tool_policy 不符"
            );
            assert_eq!(t.doc_pipeline, pipeline, "{id} doc_pipeline 不符");
            assert_eq!(t.memory_strategy, "none");
            assert!(!t.memory_write, "{id} 不应写记忆");
            // v2 字段消费收口(R3-D):四个入口智能体必须有开场白与追问建议
            assert!(!t.opening_message.trim().is_empty(), "{id} 缺少开场白");
            assert!(!t.suggestions.is_empty(), "{id} 缺少追问建议");
        }
    }

    /// 记忆读写矩阵:strategy=none 的模板不允许写记忆;full/summary 必须可写。
    #[test]
    fn builtin_memory_matrix_consistent() {
        for t in builtin_templates() {
            if t.memory_strategy == "none" {
                assert!(!t.memory_write, "{} strategy=none 不应写记忆", t.agent_id);
            } else {
                assert!(
                    t.memory_write,
                    "{} strategy={} 应可写记忆",
                    t.agent_id, t.memory_strategy
                );
            }
        }
    }

    #[test]
    fn custom_json_roundtrip() {
        let v = AgentTemplate {
            agent_id: "product-assistant--deep".into(),
            name: "产品助手-深度版".into(),
            icon: "sparkles".into(),
            description: "变体".into(),
            category: "chat".into(),
            recommended_model: "deepseek-v3".into(),
            memory_strategy: "summary".into(),
            system_prompt: "base prompt\n\n追加后缀".into(),
            input_hints: vec!["hi".into()],
            memory_write: true,
            memory_sections: vec!["会话摘要".into()],
            base_agent: Some("product-assistant".into()),
            builtin: false,
            ..v2_defaults()
        };
        let text = serde_json::to_string(&[v]).unwrap();
        let back: Vec<AgentTemplate> = serde_json::from_str(&text).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].base_agent.as_deref(), Some("product-assistant"));
        assert!(!back[0].builtin);
    }

    /// 旧版/缺字段 JSON 能解析为默认(本地优先不报错)。
    #[test]
    fn custom_json_missing_fields_default() {
        let back: Vec<AgentTemplate> = serde_json::from_str(
            r#"[{"agent_id":"x","name":"x","icon":"bot","description":"","category":"chat",
                "recommended_model":"m","memory_strategy":"full","system_prompt":"p"}]"#,
        )
        .unwrap();
        assert!(back[0].input_hints.is_empty());
        assert!(!back[0].memory_write);
        assert!(back[0].base_agent.is_none());
    }

    /// 变体解析:基座字段继承 + system_prompt 后缀拼接(前端创建时已解析,此处验证拼接约定)。
    #[test]
    fn variant_prompt_suffix_concat() {
        let base = find("product-assistant").unwrap();
        let variant_prompt = format!("{}\n\n所有回复使用简体中文。", base.system_prompt);
        assert!(variant_prompt.starts_with(&base.system_prompt));
        assert!(variant_prompt.ends_with("简体中文。"));
    }

    /// B4 / F2:模板变量渲染({date}/{model}/{language}),未知变量保留。
    #[test]
    fn render_prompt_substitutes_known_vars() {
        let out = render_prompt(
            "今天是{date},模型{model},请用{language}回复。{unknown}",
            "gpt-x",
        );
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(
            out,
            format!("今天是{today},模型gpt-x,请用中文回复。{{unknown}}")
        );
    }

    #[test]
    fn render_prompt_without_vars_unchanged() {
        assert_eq!(render_prompt("普通提示词", "m"), "普通提示词");
    }
}
