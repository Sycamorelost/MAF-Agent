//! 科研增强(v1.9.0 工作块 II):内置研究方法论技能的加载与注入。
//!
//! 来源声明 / Attribution:
//!   nature-skills — https://github.com/Yuan1z0825/nature-skills
//!   Copyright (c) Yuan1z0825 and contributors
//!   Licensed under the Apache License, Version 2.0 (see LICENSE / NOTICE).
//!   The documents under `resources/skills/` are localized, restructured
//!   derivative works (not verbatim copies); see NOTICE.
//!
//! 架构(improving.md §2.3):
//! - 知识内置:25 份中文重构文档(源自 nature-skills 的本地化重构)随包只读内置
//!   (`resources/skills/`),运行时不联网、不做用户编辑;
//! - 渐进式披露:研究管线各环节只注入按「研究报告」场景改写的轻量方法论片段,
//!   不整包灌入;片段为文档方法论的适配层提炼(§2.11:不逐字照搬论文模板,
//!   并已剔除原包中面向第三方服务的端点路由),单片段设字符上限;
//! - 注入点:查询构造(ReAct 决策)/ 阶段摘要 / 撰写与终稿 / 深度审计 / 图件契约;
//! - 失败静默降级:资源缺失或读取失败时注入为空,回退原研究管线(§2.12)。

use std::path::PathBuf;
use std::sync::OnceLock;

use tauri::Manager;

/// 技能文档清单(健康检查用;文件名与资源目录 `resources/skills/` 一一对应)。
const SKILL_FILES: [&str; 25] = [
    "00-overview.md",
    "README.md",
    "skills/nature-academic-search.md",
    "skills/nature-citation.md",
    "skills/nature-data.md",
    "skills/nature-downloader.md",
    "skills/nature-experiment-log.md",
    "skills/nature-figure.md",
    "skills/nature-image2ppt.md",
    "skills/nature-literature-pipeline.md",
    "skills/nature-paper-card.md",
    "skills/nature-paper-to-patent.md",
    "skills/nature-paper-trans.md",
    "skills/nature-paper-xray.md",
    "skills/nature-paper2ppt.md",
    "skills/nature-polishing.md",
    "skills/nature-proposal-writer.md",
    "skills/nature-reader.md",
    "skills/nature-ref-verifier.md",
    "skills/nature-response.md",
    "skills/nature-reviewer.md",
    "skills/nature-shared.md",
    "skills/nature-social-science.md",
    "skills/nature-statistics.md",
    "skills/nature-writing.md",
];

/// 单片段注入上限(字符):方法论片段为短检查表,超限截断到章节边界。
const FRAGMENT_CAP: usize = 2600;

/// 选项提示:"开"时正文 900 字符内的方法论正文,audit 用 2200 字符预算。
const AUDIT_CAP: usize = 2200;

static RESOURCE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 运行期资源目录解析结果(供命令层健康检查)。
#[derive(Debug, serde::Serialize)]
pub struct SkillDoc {
    /// 相对 resources/skills 的路径(`/` 分隔)
    pub file: String,
    /// 字节数(缺失为 0)
    pub bytes: u64,
    /// 是否可读
    pub ok: bool,
}

/// 由 lib.rs setup 调用一次:解析打包资源目录(dev 模式指向 target 目录旁的资源副本)。
pub fn init_resource(app: &tauri::AppHandle) {
    if let Ok(dir) = app
        .path()
        .resolve("resources/skills", tauri::path::BaseDirectory::Resource)
    {
        let _ = RESOURCE_DIR.set(dir);
    }
}

/// 资源目录:优先运行期解析,单测/未初始化时回退源码树(开发与测试数据留在 D 盘)。
pub fn dir() -> Option<PathBuf> {
    RESOURCE_DIR.get().cloned().or_else(|| {
        Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources")
                .join("skills"),
        )
    })
}

/// 健康检查:逐份文档 stat,供 `research_list_skills` 命令使用。
pub fn list_docs() -> Vec<SkillDoc> {
    let base = dir();
    SKILL_FILES
        .iter()
        .map(|f| {
            let p = base.as_ref().map(|b| b.join(f));
            match p.as_ref().map(std::fs::metadata) {
                Some(Ok(m)) if m.is_file() => SkillDoc {
                    file: (*f).to_string(),
                    bytes: m.len(),
                    ok: true,
                },
                _ => SkillDoc {
                    file: (*f).to_string(),
                    bytes: 0,
                    ok: false,
                },
            }
        })
        .collect()
}

// ---------------- 注入片段(文档方法论的适配层提炼) ----------------
//
// 以下片段为 resources/skills/ 对应技能文档中「核心工作流 + 关键规则与红线」
// 的研究报告场景适配版:剔除论文写作专属模板与第三方端点路由,保留可执行的
// 方法论纪律。修改片段时同步核对来源文档,保持口径一致。

/// 检索构造与来源评估(nature-academic-search 适配:与具体端点无关的策略层)。
const FRAG_QUERY: &str = r#"【检索方法论(内置科研增强)】
1. 查询分解:把主题拆成「定义与背景 / 现状与规模 / 数据与对比 / 典型案例 / 争议与风险」等互补角度,一轮内多查询互不重叠;关键词用空格分隔,单条不超过 30 字。
2. 来源评估优先级:T1 权威一手来源(官方文档、原始论文、统计数据、标准文本)> T2 二手权威(知名机构与技术媒体的分析)> T3 论坛/自媒体(仅作线索,论断须回 T1/T2 核实)。
3. 来源评估三问:发布方是否可识别且可信、内容是否给出可核对的数据或出处、时效是否匹配主题;命中存疑来源时在后续轮次用更精确查询交叉验证,而不是直接引用。
4. 查询去重与升级:与历史查询语义重复的角度不再重复检索;某角度结果不足时,换关键词、换语言或收窄/放宽范围,而不是原样重试。
以上为检索策略约束,与最终报告的 [n] 引用编号体系一致。"#;

/// 资料消化与摘要纪律(nature-reader 适配)。
const FRAG_DIGEST: &str = r#"【资料消化纪律(内置科研增强)】
1. 区分层级:事实(带数据或出处)与观点(分析判断)分开陈述,不得把观点包装成事实。
2. 溯源标注:每个论断标注 [n] 引用编号;编号必须对应资料列表,不得虚构或错位。
3. 不越界:资料未覆盖的部分如实留白或标注缺口,禁止用背景知识补写结论性内容。
4. 保留量纲:引用数据时保留单位、时间范围与统计口径(如「2025 年,约 xx 万台」)。
5. 冲突处理:多来源数据不一致时并列陈述并指出差异来源,不擅自裁剪。"#;

/// 报告结构与 claim-evidence 纪律(nature-writing + nature-polishing 适配)。
const FRAG_REPORT: &str = r#"【报告写作纪律(内置科研增强)】
1. 结构先行:先列本报告的 claim-evidence 映射(每条核心结论 ↔ 支撑它的资料编号),再动笔;无证据支撑的结论降级为「待验证」并写入局限章节。
2. 每节主题句:每段首句给出该段结论,后接证据与推论;避免把资料罗列当分析。
3. 对比表达规范:多方案对比用同一维度展开(性能/成本/成熟度/适用场景),维度对齐才有可比性;可用 Markdown 表格呈现。
4. 风格护栏:书面语,句子完整;删除空洞修饰(「众所周知」「毋庸置疑」);数量表述精确(不写「大幅提升」,写「提升 x%,自 [n]」);不使用绝对化断言(「必然」「完全」)。
5. 图文互补:能用表格或图示清晰表达的对比,不堆大段文字;正文引用图示时明确「图 X 展示了…」。"#;

/// 图件契约(nature-figure 适配:五点契约 + 输出后端固定为 mermaid/echarts)。
const FRAG_FIGURE: &str = r#"【图件契约(内置科研增强)】
报告中可以嵌入两类本地渲染图示,遵守以下契约:
1. 五点契约(每个图示先自查):① 核心结论——图要回答一句话问题;② 证据链——数据来源是资料列表中的哪些 [n];③ 类型归类——流程/关系类用 mermaid,数据对比/趋势类用 echarts;④ 后端固定——只允许 ```mermaid 与 ```echarts 两种代码块,不得要求其他运行时;⑤ 导出契约——图注以「图 N:」编号,一句话说明结论。
2. echarts 输出格式:代码块语言标注为 echarts,内容为一个 JSON 对象(合法 JSON,不带注释),必须包含 xAxis/yAxis/series(柱状用 type:bar,折线用 type:line,占比用饼图 series 且不写 xAxis/yAxis);颜色克制,通常不超过 4 个系列。
3. 一图一主张:每个图示只表达一个核心结论;两图可互相概括时合并为一图。
4. 白底直标:优先直接标注数据与系列名,少用图例;配色克制,不要彩虹色。
5. 图示是分析的一部分:图后必须有一段正文解读图示揭示的结论,不允许「看图即知」。
6. mermaid 用于机制/流程/架构示意,节点文字简洁(每个节点不超过 12 字);"#;

/// 引用纪律(nature-citation 适配;注入终稿提示词,finalize 的引用列表为系统生成)。
const FRAG_CITE: &str = r#"【引用纪律(内置科研增强)】
1. claim↔文献双向对应:正文中每个 [n] 必须在资料列表中存在且语义匹配;一条资料可支撑多个论断,但不得反向虚构。
2. 引用密度:核心结论句必须带引用;同一段落连续论断可合并引用,如 [1][3]。
3. 转述优先:用转述而非大段摘抄;数字、时间、专名保持与来源一致。
4. 「资料来源」章节由系统生成,正文中不要自写参考文献列表。"#;

/// 深度审计检查表(nature-ref-verifier 三级严重度 + nature-statistics P0/P1/P2 适配)。
pub(crate) const AUDIT_SYSTEM: &str = r#"你是研究报告审计器。对给定报告做一次只读审计,输出「审计附录」。不发起新检索,不改写报告正文。

审计维度(逐项检查,发现问题才列出):
一、引用核查(严重度三级):
- [S1] 严重:[n] 编号不存在、指向与论断明显无关的资料,或论断超出资料支持范围;
- [S2] 建议核对:数据/时间与引用资料的口径可能不一致,需人工复核;
- [S3] 参考:措辞与来源的细微差异,不影响结论。
二、统计与数据审计(失效模式分级):
- [P0] 可推翻结论:伪重复(把多次测量当独立样本)、未声明多重比较、把「一个显著一个不显著」说成「效应不同」、统计单位错配;
- [P1] 证据不足:只报 p 值/显著性不报效应量、小样本过度外推、误差线含义未定义、离群值处理不清、相关性被表述为因果;
- [P2] 表述清晰度:n 值缺失、单位/口径缺失、百分比无基数。
三、结论与证据一致性:TL;DR 与正文的结论是否被证据支撑;对比维度是否对齐;局限章节是否如实。

输出格式(只输出以下 Markdown):
## 审计附录
(若未发现任何问题:一句话说明「未发现 S1/P0 级问题」,并给出 S2/S3/P1/P2 层面的改进建议;)
发现问题条目格式:`- [级别] 位置:问题描述 → 修改建议`
结尾一行:「审计性质:本地静态审计,基于报告文本与引用编号,不构成对事实本身的真实性担保。」"#;

// ---------------- 注入装配 ----------------

/// 按研究参数装配的注入片段集合(空串 = 该环节不注入)。
pub struct SkillFragments {
    /// ReAct 决策/查询构造注入
    pub query: String,
    /// 阶段摘要注入
    pub digest: String,
    /// 撰写阶段(专项 synthesis)注入
    pub synthesis: String,
    /// 终稿报告注入(写作纪律 + 图件契约 + 引用纪律)
    pub report: String,
    /// 深度审计 pass 的 system 提示(Some = 执行审计)
    pub audit: Option<String>,
}

/// 裁剪到上限:在片段长度内截断,尽量落在段落边界,保证提示词完整可读。
fn clamp(s: &str, cap: usize) -> String {
    if s.chars().count() <= cap {
        return s.to_string();
    }
    let cut: String = s.chars().take(cap).collect();
    match cut.rfind("\n\n") {
        Some(i) if i > cap / 2 => cut[..i].to_string(),
        _ => cut,
    }
}

/// 按参数装配注入片段。`skill_enhance` 关闭时全部为空(静默降级为原管线)。
pub fn fragments(params: &super::ResearchParams) -> SkillFragments {
    if !params.skill_enhance {
        return SkillFragments {
            query: String::new(),
            digest: String::new(),
            synthesis: String::new(),
            report: String::new(),
            audit: None,
        };
    }
    let query = clamp(FRAG_QUERY, FRAGMENT_CAP);
    let digest = clamp(FRAG_DIGEST, FRAGMENT_CAP);
    let synthesis = clamp(FRAG_REPORT, FRAGMENT_CAP);
    // 终稿注入:写作纪律 + 图件契约 + 引用纪律;图件契约使报告具备 mermaid/echarts 输出能力
    let mut report = String::new();
    report.push_str(&clamp(FRAG_REPORT, FRAGMENT_CAP));
    report.push_str("\n\n");
    report.push_str(&clamp(FRAG_FIGURE, FRAGMENT_CAP));
    report.push_str("\n\n");
    report.push_str(&clamp(FRAG_CITE, FRAGMENT_CAP));
    let audit = if params.skill_audit {
        Some(clamp(AUDIT_SYSTEM, AUDIT_CAP))
    } else {
        None
    };
    SkillFragments {
        query,
        digest,
        synthesis,
        report,
        audit,
    }
}

/// AI 概念示意草稿的生图提示词(nature-figure AI 草稿纪律:概念草稿非证据)。
pub fn image_draft_prompt(topic: &str, tldr: &str) -> String {
    let tldr: String = tldr.chars().take(400).collect();
    format!(
        "Concept illustration draft for a research report on \"{topic}\". \
Clean minimal technical schematic on white background, muted professional palette \
(centered on deep blue #0F4D92 with one accent color), clear flat vector style, \
labeled conceptual blocks with simple arrows showing relationships, generous whitespace. \
The illustration must convey only the core idea: {tldr} \
Strictly conceptual: no fabricated numbers, no charts with invented data values, \
no logos, no photorealism, no dense text labels."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::ResearchParams;

    #[test]
    fn fragments_empty_when_enhance_off() {
        let mut p = ResearchParams::default();
        p.skill_enhance = false;
        p.skill_audit = true;
        let f = fragments(&p);
        assert!(f.query.is_empty());
        assert!(f.report.is_empty());
        assert!(f.audit.is_none(), "总开关关闭时审计不执行");
    }

    #[test]
    fn fragments_present_when_enhance_on() {
        let p = ResearchParams::default();
        // 默认:总开关开、深度审计关
        assert!(p.skill_enhance);
        assert!(!p.skill_audit);
        let f = fragments(&p);
        assert!(f.query.contains("检索方法论"));
        assert!(f.digest.contains("资料消化"));
        assert!(f.synthesis.contains("报告写作纪律"));
        assert!(f.report.contains("图件契约"));
        assert!(f.report.contains("echarts"));
        assert!(f.report.contains("引用纪律"));
        assert!(f.audit.is_none(), "深度审计默认关");
    }

    #[test]
    fn fragments_audit_optional() {
        let mut p = ResearchParams::default();
        p.skill_audit = true;
        let f = fragments(&p);
        let a = f.audit.expect("开启后应有审计提示词");
        assert!(a.contains("审计附录"));
        assert!(a.contains("[S1]"));
        assert!(a.contains("[P0]"));
    }

    #[test]
    fn fragment_size_bounded() {
        let p = ResearchParams::default();
        let f = fragments(&p);
        // 单次注入总量受上限约束(避免 token 成本失控)
        assert!(f.query.chars().count() <= FRAGMENT_CAP);
        assert!(f.report.chars().count() <= FRAGMENT_CAP * 3 + 4);
    }

    #[test]
    fn clamp_truncates_at_paragraph_boundary() {
        let s = "a\n\nb\n\nc";
        assert_eq!(clamp(s, 4), "a\n\nb");
        assert_eq!(clamp(s, 100), s);
    }

    #[test]
    fn list_docs_reports_files() {
        let docs = list_docs();
        assert_eq!(docs.len(), SKILL_FILES.len());
        // 源码树回退路径在单测环境必然存在
        assert!(docs.iter().all(|d| d.ok), "所有技能文档应可读");
        assert!(
            docs.iter()
                .filter(|d| d.file.starts_with("skills/"))
                .count()
                == 23
        );
    }

    #[test]
    fn image_draft_prompt_has_discipline() {
        let s = image_draft_prompt("测试主题", "核心结论一句话");
        assert!(s.contains("#0F4D92"));
        assert!(s.contains("no fabricated numbers"));
    }
}
