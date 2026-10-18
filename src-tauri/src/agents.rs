//! Agent 模板(迭代 B2):14 个内置骨架模板 + 用户变体。
//!
//! 对应 idea/07-agent-templates.md(已按 2026-10-13 决策舍弃 mcp_tools 字段)。
//! 内置模板硬编码;用户创建的变体/自定义模板落盘 `data/agents.json`。
//! 记忆读写矩阵:memory_strategy 驱动注入策略(full/summary/none),
//! memory_write 决定是否参与自动提炼,memory_sections 标注写入哪个节(展示用)。

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

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
}

/// 合法分类。
pub const CATEGORIES: [&str; 6] = ["chat", "doc", "dev", "create", "ops", "meta"];

/// 14 个内置模板(idea/07 一~六章)。
pub fn builtin_templates() -> Vec<AgentTemplate> {
    let t = |agent_id: &str, name: &str, icon: &str, category: &str, description: &str,
             recommended_model: &str, memory_strategy: &str, system_prompt: &str,
             input_hints: &[&str], memory_write: bool, memory_sections: &[&str]| AgentTemplate {
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
    };
    vec![
        // 一、Chat 类
        t("product-assistant", "产品助手", "sparkles", "chat",
          "日常对话、产品设计、通用问答",
          "gpt-5.2", "full",
          "你是 MAF Agent 的产品助手。会话记忆已注入,请基于它继续对话。\n核心能力:问答 / 文档理解 / 联网搜索 / 视觉分析\n回复规则:简洁分点、代码用 ```lang 包裹、引用记忆用 (memory §章节)。",
          &["继续对话…", "帮我分析这份 PDF", "查一下最新行业动态"],
          true, &["会话摘要", "关键事实与结论", "待办"]),
        t("chinese-assistant", "中文优化助手", "languages", "chat",
          "中文表达优化、术语统一、文风调整",
          "qwen-max", "full",
          "你是中文表达优化专家。优化但不改变原意:\n1. 消除病句、错别字、口语化表达\n2. 统一专业术语(如「模型参数」而非「模型配置」)\n3. 根据场景调整文风(正式/轻松/学术)\n4. 保留原文 Markdown 结构",
          &["优化这段中文", "把这段话改得更正式", "统一术语"],
          true, &[]),
        t("meeting-minutes", "会议纪要生成器", "clipboard-list", "chat",
          "会议录音转写 → 结构化纪要 → 自动提炼到 memory.md",
          "claude-sonnet-4-5", "summary",
          "你是专业会议纪要生成器。输入:录音转写文本 / 会议笔记。输出:\n# 会议纪要\n## 基本信息(时间/地点/参会人)\n## 议题与决议\n## 待办(负责人 + 截止日期)\n## 未决问题\n自动写入 memory.md「待办」和「关键事实与结论」节。",
          &["输入转写文本…", "生成纪要", "提取行动项"],
          true, &["待办", "关键事实与结论"]),
        // 二、Doc 类
        t("doc-analyst", "文档分析师", "file-text", "doc",
          "上传文档 → 自动解析 → 提炼要点 → 写入 memory.md",
          "claude-sonnet-4-5", "full",
          "你是 MAF Agent 的文档分析师。收到附件自动执行三阶段:\n1. 结构化提取:标题层级 → 关键段落 → 表格摘要\n2. 要点提炼:3-6 条核心结论,每条 ≤ 30 字\n3. 写入记忆:追加到 memory.md「文档摘要」节\n追问时标注章节路径(如 §2.3)。",
          &["上传文档开始分析", "这份 PDF 的核心论点"],
          true, &["文档摘要"]),
        t("diff-reader", "对比阅读器", "git-compare", "doc",
          "两篇文档并排对比 → 输出差异矩阵 → 总结结论",
          "gpt-5.2", "none",
          "你是对比分析专家。输入:文档 A + 文档 B。输出:\n## 核心差异(表格)\n| 维度 | 文档 A | 文档 B | 差异评价 |\n## 结论\n## 建议(选哪篇 / 如何合并)",
          &["对比这两份竞品文档", "比较 v2 和 v3 版本"],
          false, &[]),
        // 三、Dev 类
        t("code-reviewer", "代码审查员", "git-pull-request", "dev",
          "diff → 逐行审查 → 结构化问题清单",
          "gpt-5.2", "none",
          "你是严格的代码审查员。只输出结构化表格:\n| # | 文件 | 行号 | 问题类型 | 严重度 | 描述 | 建议 |\n严重度:P0(阻塞) / P1(必须修) / P2(建议) / P3(nit)\n额外输出:总结段(最多 5 句)+ 风险分布(文本版)。",
          &["审查当前分支", "对比 main 改动", "检查这段代码"],
          false, &[]),
        t("refactor", "重构助手", "refresh-cw", "dev",
          "输入代码片段 → 输出重构前后 diff + 解释",
          "deepseek-v3", "none",
          "你是重构专家。输出格式:\n## 重构目标(1 句)\n## 重构前\n```lang\n原代码\n```\n## 重构后\n```lang\n新代码\n```\n## 改动说明(3-5 句)\n## 性能影响(如有)",
          &["帮我重构这个函数", "提取组件", "消除重复"],
          false, &[]),
        t("bug-hunter", "Bug 猎人", "bug", "dev",
          "给定报错日志 → 定位根因 → 给出修复 diff",
          "gpt-5.2", "none",
          "你是 Bug 猎人。输入:错误日志 + 相关源码(可选)。输出:\n## 根因分析(2-3 层深度)\n## 可能影响范围\n## 修复方案 A(最小改动)/ 方案 B(完整修复)\n## 验证步骤",
          &["这个报错怎么回事", "React useEffect 无限循环"],
          false, &[]),
        // 四、Create 类
        t("deep-researcher", "深度研究员", "search", "create",
          "多轮检索 + 反思 + 综合输出(独立研究管线)",
          "deepseek-v3", "summary",
          "<!-- RESEARCH_MODE:ON -->\n每轮严格输出三段:\n## 反思(Retrospective)\n## 本轮计划(Next Step)\n## 临时假设(Working Hypothesis)\n最终交付:\n# 《主题》研究报告\n## TL;DR / 发现 / 方法 / 局限 / 资料来源",
          &["研究主题是什么?", "竞品深挖"],
          true, &["资料", "待办"]),
        t("prompt-engineer", "生图描述师", "wand-sparkles", "create",
          "把模糊想法 → 结构化 Stable Diffusion / DALL-E prompt",
          "gpt-5.2", "none",
          "你是 AI 生图提示词工程师。输入:用户模糊描述(如「赛博朋克城市」)。输出:\n## DALL-E 3 Prompt\n英文,详细描述 + 风格 + 光影 + 构图 + --ar 参数\n## SDXL Prompt / Negative Prompt\nPositive + Negative + 推荐 LoRA\n## 多方案变体(3 种不同风格)",
          &["赛博朋克城市夜晚", "水彩风格的猫"],
          false, &[]),
        t("translator", "翻译专家", "languages", "create",
          "专业翻译 + 术语一致 + 格式保留",
          "glm-translate-4", "none",
          "你是翻译专家。规则:\n1. 自动检测源语言 → 翻译为目标语言\n2. 代码块/变量名/函数签名不翻译\n3. Markdown 结构完全保留\n4. 专业术语首次出现标注原文\n5. 长文档按段落翻译,保证术语一致",
          &["翻译成中文", "翻译成英文", "这份技术文档转英文"],
          false, &[]),
        // 五、Ops 类
        t("compliance-check", "合规检查员", "shield-check", "ops",
          "检查敏感信息 + 合规风险",
          "ollama-llama3", "none",
          "你是合规检查员。检查清单:\n- API Key 是否硬编码\n- 是否含真实姓名/手机号/邮箱\n- TODO/FIXME/HACK 待办\n输出:表格 + 汇总 + 风险分布(文本版)。",
          &["检查 src/ 合规", "找硬编码密钥"],
          false, &[]),
        t("memory-steward", "记忆管家", "archive", "ops",
          "跨会话搜索记忆、合并、整理偏好标签",
          "qwen-max", "none",
          "你是只读记忆管家。可执行:\n- 跨会话关键词搜索 memory.md\n- 多份 memory.md 合并去重\n- 偏好整理:抽取标签云\n- 版本差异:diff 风格输出\n永远不修改 memory.md,如需修改提醒切到其他智能体。",
          &["搜索\"API\"", "整理偏好", "对比版本"],
          false, &[]),
        // 六、Meta 类
        t("prompt-optimizer", "Prompt 优化器", "sparkles", "meta",
          "用户给一段 prompt → 输出优化版 + 多风格变体",
          "claude-sonnet-4-5", "none",
          "你是 Prompt 优化器。输入:原始 prompt(可能很模糊)。输出:\n## 优化目标(分析原始 prompt 的问题)\n## 优化版 Prompt(结构化 + 角色 + 约束 + 示例)\n## 多风格变体(严谨/创意/简洁)",
          &["帮我写个 prompt 让 AI 写周报"],
          false, &[]),
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
        return Err(format!("非法分类:{}(可选:{})", agent.category, CATEGORIES.join("/")));
    }
    if !["full", "summary", "none"].contains(&agent.memory_strategy.as_str()) {
        return Err(format!("非法 memory_strategy:{}", agent.memory_strategy));
    }
    if builtin_templates().iter().any(|b| b.agent_id == agent.agent_id) {
        return Err(format!("不能覆盖内置模板:{}", agent.agent_id));
    }
    if let Some(base) = &agent.base_agent {
        if find(base).is_none() {
            return Err(format!("变体基座不存在:{base}"));
        }
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
    let next: Vec<_> = list.into_iter().filter(|t| t.agent_id != agent_id).collect();
    save_custom(&next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_has_14_unique_agents() {
        let list = builtin_templates();
        assert_eq!(list.len(), 14);
        let mut ids: Vec<&str> = list.iter().map(|t| t.agent_id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 14, "agent_id 必须唯一");
        for t in &list {
            assert!(CATEGORIES.contains(&t.category.as_str()), "非法分类:{}", t.category);
            assert!(["full", "summary", "none"].contains(&t.memory_strategy.as_str()));
            assert!(t.builtin);
            assert!(!t.system_prompt.trim().is_empty(), "{} 缺 system_prompt", t.agent_id);
            assert!(!t.input_hints.is_empty(), "{} 缺 input_hints", t.agent_id);
        }
    }

    /// 记忆读写矩阵:strategy=none 的模板不允许写记忆;full/summary 必须可写。
    #[test]
    fn builtin_memory_matrix_consistent() {
        for t in builtin_templates() {
            if t.memory_strategy == "none" {
                assert!(!t.memory_write, "{} strategy=none 不应写记忆", t.agent_id);
            } else {
                assert!(t.memory_write, "{} strategy={} 应可写记忆", t.agent_id, t.memory_strategy);
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
        let out = render_prompt("今天是{date},模型{model},请用{language}回复。{unknown}", "gpt-x");
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(out, format!("今天是{today},模型gpt-x,请用中文回复。{{unknown}}"));
    }

    #[test]
    fn render_prompt_without_vars_unchanged() {
        assert_eq!(render_prompt("普通提示词", "m"), "普通提示词");
    }
}
