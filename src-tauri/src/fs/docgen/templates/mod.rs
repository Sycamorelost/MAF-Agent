//! 文档模板注册表:把「用途」升级为可选择的「文体模板」。
//!
//! 模板只描述元数据与版式覆盖,不写渲染逻辑;识别算法按标题/正文关键词打分,
//! 显式传 `template` 时优先使用显式值。
//!
//! 版式依据:
//! - 公文:GB/T 9704—2012(党政机关公文格式),正文三号仿宋、每页 22 行、页边距上 37/下 35/左 28/右 26mm;
//! - 论文/实验/实践:高校学位论文与实验报告通行版式(宋体小四、1.5 倍行距、三线表);
//! - 总结/通知/纪要:商务排版(微软雅黑/宋体、网格表)。

mod book;
mod business;
mod manual;
pub mod numbering;
mod official;
pub mod package;
mod reference_docx;
mod thesis;

use super::profile::Profile;
use super::style::{DocStyle, HeadingNumbering};

/// 一套文体模板。
pub struct Template {
    /// 稳定 id,工具参数取值
    pub id: &'static str,
    /// 中文名(回执展示)
    pub name: &'static str,
    /// 基础版式来源
    pub profile: Profile,
    /// 标题编号方案
    pub numbering: HeadingNumbering,
    /// 识别关键词(标题权重更高)
    pub keys: &'static [&'static str],
    /// 缺失章节时的建议骨架
    pub skeleton: &'static [&'static str],
    /// 写作提示(随工具回执返回,不进系统提示词)
    pub hints: &'static str,
}

/// 模板的装配参数(只声明数据,不写渲染分支)。
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // 字段为声明式数据,由各装配器按需读取
pub struct TemplateSpec {
    /// 封面字段(空表示沿用 `DocMeta` 默认四字段)
    pub cover_fields: &'static [&'static str],
    /// 是否生成原创性声明页
    pub declaration_page: bool,
    /// 标题/列表是否使用原生 `numbering.xml` + `w:numPr`
    pub native_numbering: bool,
    /// 是否按封面/前置/正文/附录分节
    pub sectioned: bool,
    /// 是否支持章清单(「# 第 1 章 …」)
    pub chapter_list: bool,
    /// 章首页是否无页眉
    pub chapter_first_page: bool,
    /// 是否把 Markdown 尾注落成 Word 尾注
    pub endnotes: bool,
}

impl TemplateSpec {
    pub const DEFAULT: TemplateSpec = TemplateSpec {
        cover_fields: &[],
        declaration_page: false,
        native_numbering: false,
        sectioned: false,
        chapter_list: false,
        chapter_first_page: false,
        endnotes: false,
    };

    /// 生效封面字段(为空中沿用 `DocMeta` 的默认行)。
    #[allow(dead_code)]
    pub fn cover_labels(&self) -> &'static [&'static str] {
        self.cover_fields
    }
}

/// 取模板装配参数;未知模板返回默认值。
pub fn spec_of(id: &str) -> TemplateSpec {
    match lookup(id).map(|t| t.id).unwrap_or("general") {
        "thesis" => thesis::SPEC,
        "book" => book::SPEC,
        _ => TemplateSpec::DEFAULT,
    }
}

const TEMPLATES: &[Template] = &[
    Template {
        id: "official",
        name: "公文",
        profile: Profile::General,
        numbering: HeadingNumbering::Official,
        keys: &[
            "请示", "批复", "公函", "公告", "通告", "意见", "决定", "命令",
        ],
        skeleton: &[
            "标题",
            "主送机关",
            "正文",
            "附件说明",
            "发文机关署名",
            "成文日期",
            "抄送机关",
            "印发机关和日期",
        ],
        hints: "公文体:标题二号小标宋,正文三号仿宋_GB2312,首行缩进 2 字符,成文日期右空四字。",
    },
    Template {
        id: "paper",
        name: "论文",
        profile: Profile::Paper,
        numbering: HeadingNumbering::Academic,
        keys: &["论文", "摘要", "关键词", "参考文献", "学位"],
        skeleton: &[
            "摘要",
            "关键词",
            "引言",
            "相关工作",
            "方法",
            "实验",
            "结论",
            "参考文献",
        ],
        hints: "论文体:摘要+关键词各一段,章节用 1/1.1/1.1.1 编号,表题在上、图注在下,参考文献 [1] 悬挂缩进。",
    },
    thesis::TEMPLATE,
    book::TEMPLATE,
    Template {
        id: "lab",
        name: "实验报告",
        profile: Profile::Experiment,
        numbering: HeadingNumbering::Official,
        keys: &["实验", "上机", "仿真", "实验目的", "实验步骤", "实验原理"],
        skeleton: &[
            "实验目的",
            "实验环境",
            "实验原理",
            "实验步骤",
            "数据记录",
            "结果分析",
            "结论与思考",
        ],
        hints: "实验报告:目的/环境/原理/步骤/数据/分析/结论,数据用三线表,图注写「图 N …」。",
    },
    Template {
        id: "practice",
        name: "实践调研报告",
        profile: Profile::Practice,
        numbering: HeadingNumbering::Simple,
        keys: &["实践", "实习", "调研", "调查报告", "走访"],
        skeleton: &[
            "背景与对象",
            "方法与过程",
            "调研发现",
            "问题分析",
            "建议",
            "附录",
        ],
        hints: "实践报告:背景/方法/过程/发现/问题/建议,结论要落到可执行建议。",
    },
    Template {
        id: "summary",
        name: "工作总结",
        profile: Profile::General,
        numbering: HeadingNumbering::Simple,
        keys: &["总结", "周报", "月报", "季度", "年度", "完成情况", "下一步"],
        skeleton: &["概述", "完成情况", "亮点与数据", "问题与不足", "下一步计划"],
        hints: "总结体:概述→完成情况(量化)→亮点→问题→下一步,每节控制在 3-5 条。",
    },
    Template {
        id: "notice",
        name: "通知",
        profile: Profile::General,
        numbering: HeadingNumbering::Simple,
        keys: &["通知", "特此通知", "知照"],
        skeleton: &["标题", "主送单位", "事由", "具体要求", "落款", "日期"],
        hints: "通知体:标题+主送单位+事由+要求+落款日期,时间地点要求必须具体。",
    },
    Template {
        id: "minutes",
        name: "会议纪要",
        profile: Profile::Practice,
        numbering: HeadingNumbering::Simple,
        keys: &["纪要", "会议", "出席", "议题", "决议", "参会"],
        skeleton: &[
            "会议基本信息",
            "出席人员",
            "议题",
            "讨论要点",
            "决议事项",
            "分工与时限",
        ],
        hints: "纪要体:时间地点主持+出席+议题+讨论+决议+分工,决议要写责任人与完成时间。",
    },
    Template {
        id: "whitepaper",
        name: "白皮书",
        profile: Profile::General,
        numbering: HeadingNumbering::Simple,
        keys: &["白皮书", "行业报告", "解决方案", "洞察", "趋势"],
        skeleton: &["摘要", "背景与挑战", "方案", "价值", "案例", "结语"],
        hints: "白皮书体:摘要先行,背景→方案→价值→案例,数据须标注来源,避免空泛口号。",
    },
    Template {
        id: "manual",
        name: "产品手册",
        profile: Profile::General,
        numbering: HeadingNumbering::Simple,
        keys: &["手册", "使用说明", "操作指南", "快速上手", "用户手册"],
        skeleton: &[
            "产品概述",
            "安装与准备",
            "快速上手",
            "功能说明",
            "常见问题",
            "附录",
        ],
        hints: "手册体:按任务组织步骤,每步可执行、可验证;术语首次出现给定义。",
    },
    Template {
        id: "news",
        name: "新闻稿",
        profile: Profile::General,
        numbering: HeadingNumbering::None,
        keys: &["新闻稿", "新闻", "发布", "通讯员", "媒体"],
        skeleton: &["标题", "导语", "主体", "背景", "引语", "媒体联系"],
        hints: "新闻体:导语含 5W1H,正文倒金字塔,引语注明身份,结尾给媒体联系。",
    },
    Template {
        id: "general",
        name: "通用文档",
        profile: Profile::General,
        numbering: HeadingNumbering::None,
        keys: &[],
        skeleton: &[],
        hints: "通用文档:微软雅黑、网格表,不自动编号。",
    },
];

/// 全部模板(识别顺序即此顺序,同分时靠前者优先)。
pub fn all() -> &'static [Template] {
    TEMPLATES
}

/// 按 id/别名查找模板(大小写与空格不敏感)。
pub fn lookup(id: &str) -> Option<&'static Template> {
    let key = id.trim().to_ascii_lowercase();
    let canonical = match key.as_str() {
        "公文" | "gongwen" => "official",
        "论文" | "学术论文" => "paper",
        "学位论文" | "毕业论文" | "thesis" => "thesis",
        "书稿" | "图书" | "book" => "book",
        "实验" | "实验报告" | "experiment" => "lab",
        "实践" | "调研" | "调研报告" | "practice" | "report" => "practice",
        "总结" | "工作总结" | "work_summary" => "summary",
        "通知" | "notify" => "notice",
        "纪要" | "会议纪要" | "meeting" => "minutes",
        "白皮书" | "whitepaper" => "whitepaper",
        "手册" | "产品手册" | "manual" => "manual",
        "新闻" | "新闻稿" | "news" => "news",
        "通用" | "doc" => "general",
        _ => key.as_str(),
    };
    all().iter().find(|t| t.id == canonical)
}

/// `profile` 参数的兼容映射。
pub fn id_for_profile(profile: Profile) -> &'static str {
    match profile {
        Profile::Experiment => "lab",
        Profile::Paper => "paper",
        Profile::Practice => "practice",
        Profile::General => "general",
    }
}

/// 自动识别:标题命中权重 3、正文前 400 字权重 1,公文文号额外加权。
pub fn detect(title: &str, body: &str) -> &'static Template {
    let probe: String = body.chars().take(400).collect();
    let mut best = &all()[all().len() - 1];
    let mut best_score = 0usize;
    for t in all() {
        let mut score = hits(title, t.keys) * 3 + hits(&probe, t.keys);
        if t.id == "official" && (title.contains('〔') || probe.contains('〔')) {
            score += 3;
        }
        if t.id == "paper" && probe.contains("摘要") && probe.contains("关键词") {
            score += 2;
        }
        if score > best_score {
            best = t;
            best_score = score;
        }
    }
    best
}

fn hits(text: &str, keys: &[&str]) -> usize {
    keys.iter().filter(|k| text.contains(*k)).count()
}

/// 模板对应的版式(基础版式 + 模板覆盖)。
pub fn style_of(id: &str) -> DocStyle {
    let t = lookup(id).unwrap_or(&TEMPLATES[TEMPLATES.len() - 1]);
    let mut style = DocStyle::for_profile(t.profile);
    style.numbering = t.numbering;
    match t.id {
        "official" => official::apply(&mut style),
        "minutes" => business::apply_minutes(&mut style),
        "thesis" => thesis::apply(&mut style),
        "book" => book::apply(&mut style),
        "whitepaper" => manual::apply_whitepaper(&mut style),
        "manual" => manual::apply_manual(&mut style),
        "news" => manual::apply_news(&mut style),
        _ => {}
    }
    style
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_supports_ids_aliases_and_chinese() {
        assert_eq!(lookup("paper").unwrap().id, "paper");
        assert_eq!(lookup("论文").unwrap().id, "paper");
        assert_eq!(lookup(" 纪要 ").unwrap().id, "minutes");
        assert!(lookup("unknown").is_none());
    }

    #[test]
    fn detects_official_by_document_number() {
        let t = detect("关于××的请示", "××〔2026〕3 号\n主送机关:…");
        assert_eq!(t.id, "official");
    }

    #[test]
    fn detects_paper_lab_and_summary() {
        assert_eq!(detect("排课问题研究", "摘要:本文…关键词:约束").id, "paper");
        assert_eq!(detect("实验三 排课 CSP", "实验目的:…").id, "lab");
        assert_eq!(detect("9 月工作总结", "完成情况:…").id, "summary");
        assert_eq!(detect("周报", "本周完成若干事项").id, "summary");
    }

    #[test]
    fn unknown_text_falls_back_to_general() {
        assert_eq!(detect("随手记", "今天天气不错").id, "general");
    }

    #[test]
    fn official_style_uses_gb9704_metrics() {
        let s = style_of("official");
        assert_eq!(s.margin_top, 2098);
        assert_eq!(s.margin_bottom, 1984);
        assert_eq!(s.margin_left, 1587);
        assert_eq!(s.margin_right, 1474);
        assert_eq!(s.body_size, 32); // 三号
        assert_eq!(s.fixed_line, Some(580)); // 每页 22 行
        assert_eq!(s.east_asia, "仿宋_GB2312");
        assert_eq!(s.numbering, HeadingNumbering::Official);
    }

    #[test]
    fn registers_thesis_and_book_with_specs() {
        assert_eq!(lookup("thesis").unwrap().id, "thesis");
        assert_eq!(lookup("学位论文").unwrap().id, "thesis");
        assert_eq!(lookup("book").unwrap().id, "book");
        let t = spec_of("thesis");
        assert!(t.native_numbering && t.sectioned && t.declaration_page);
        assert!(t.cover_fields.contains(&"指导教师"));
        let b = spec_of("book");
        assert!(b.chapter_list && b.chapter_first_page);
        assert!(!b.declaration_page);
        // 旧模板参数不受影响
        assert!(!spec_of("paper").native_numbering);
        assert!(!spec_of("general").sectioned);
    }

    #[test]
    fn paper_and_lab_styles_keep_academic_metrics() {
        let p = style_of("paper");
        assert_eq!(p.body_size, 24);
        assert_eq!(p.numbering, HeadingNumbering::Academic);
        let l = style_of("lab");
        assert!(l.three_line);
        assert_eq!(l.numbering, HeadingNumbering::Official);
    }
}
