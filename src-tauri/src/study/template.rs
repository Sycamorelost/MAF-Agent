//! 学习规划提示词模板(移植 Deep-tutor 技能提示词包)。
//!
//! 来源声明 / Attribution:
//!   Deep-tutor — https://github.com/hershate/Deep-tutor
//!   Copyright (c) hershate and contributors
//!   Licensed under the Apache License, Version 2.0 (see LICENSE / NOTICE).
//!   This file is a derivative work: the prompt contracts were rewritten for
//!   myAgent's Rust engine and adapted to its JSON schema; see NOTICE.
//!
//! - 内置模板即引擎常量,代码内保护不可删;
//! - 用户自定义覆盖落盘 `data/study/templates.json`,同 key 覆盖式更新;
//! - 生效优先级:templates.json 自定义 > 内置;
//! - 所有模板只允许模型输出约定 Schema 的 JSON(校验在 mod.rs 确定性完成)。

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 合法模板 key(P0:意图澄清 / 知识地图 / 阶段设计 / 资料推荐;P1:自学材料 / 测验出题;
/// 排期为确定性计算,无模板)。
#[allow(dead_code)] // P1 模板编辑 UI 启用
pub const TEMPLATE_KEYS: [&str; 7] = [
    "study_intent",
    "study_map",
    "study_stages",
    "study_resources",
    "study_guide",
    "study_quiz",
    "study_grade",
];

/// 一条用户自定义覆盖(与研究模板同构)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateOverride {
    pub key: String,
    pub content: String,
    pub updated: String,
}

// ---------------- 内置模板 ----------------

pub const STUDY_INTENT: &str = r#"你是学习规划师。从用户输入中抽取学习意图,只输出一个 JSON 对象,不包代码围栏,不输出任何其他文本。

Schema:
{"topic":"学习主题","level":"zero|beginner|intermediate|advanced","goal":"学习目标(一句话)","daily_minutes":每天可投入分钟数(整数),"deadline":"目标完成日期 YYYY-MM-DD 或 null","lang":"zh|en","subject":"学科名(自由文本,如 数学/英语/物理/语文/编程/化学/…;无法判断填 null)","grade_band":"primary|junior|senior|university|adult|null","exam_target":"中考|高考|四六级|考研|兴趣 等,或 null","preferences":"用户对内容范围、结构或呈现格式的明确要求(原意转述,≤60字;没有则 null)","missing":["缺失的可选字段名"],"questions":["针对缺失字段的追问"]}

规则:
1. 只抽取用户明确表达的信息,禁止编造;无法确定时放入 missing 并给出 questions;
2. 可选字段为 level / daily_minutes / deadline;topic 与 goal 必有;
3. topic 若用户输入即主题则原样保留,不得改写;
4. lang 与用户输入语言一致;
5. subject / grade_band / exam_target 从目标描述中智能识别(如「三个月学完高中数学必修一」→ subject=数学, grade_band=senior, exam_target=高考);识别不出填 null,不硬猜;这三个字段不参与 missing 追问;
6. preferences 只在用户明确提出内容偏好或格式要求时填写(如「只要重点」「多给表格」「按章节讲」「不要练习题」),不得把普通主题描述当成偏好;没有则为 null;
7. questions 最多 2 条,每条一句话,直接问用户要缺失信息。"#;

pub const STUDY_MAP: &str = r#"你是课程设计专家。为主题生成知识地图,只输出一个 JSON 对象,不包代码围栏,不输出任何其他文本。

Schema:
{"nodes":[{"name":"知识点名","difficulty":1,"why":"为什么重要(≤30字)","must_learn":true,"prereqs":["前置知识点名"]}]}

规则:
1. 生成 8-20 个知识点,覆盖主题全貌,由浅入深;
2. name 必须唯一、简短(≤12 字);difficulty 为 1(入门)~5(专家)整数;
3. prereqs 只能逐字引用本列表内已有的知识点名,可为空数组;禁止把目标主题本身拆成前置知识点;
4. 约 60% 的知识点 must_learn=true(核心必学),其余为可选拓展;
5. 用户明确要求优先:若用户指定了知识点范围或组织方式,优先满足用户要求;与模板默认结构冲突时以用户要求为准,但不得编造知识点;
6. 只输出 JSON。"#;

pub const STUDY_STAGES: &str = r#"你是课程设计专家。把给定的知识地图组织为学习阶段,只输出一个 JSON 对象,不包代码围栏,不输出任何其他文本。

Schema:
{"stages":[{"index":0,"name":"阶段名","goal":"阶段目标(一句话)","difficulty":1,"estimated_weeks":2,"topics":["知识点名"],"milestone":"阶段里程碑"}]}

硬规则:
1. 阶段 3-6 个,按学习顺序排列,index 从 0 递增;
2. 每阶段 topics 3-6 个,必须来自给定的知识点列表,不得编造;每个知识点只允许出现一次;
3. 相邻阶段 difficulty 差 ≤2,整体由易到难;
4. 所有 must_learn 知识点必须被覆盖;
5. estimated_weeks 为整数 ≥1;
6. milestone 只用当前及之前阶段的知识可达成,表述为可检验的成果;
7. 用户明确要求优先:若用户指定了阶段划分方式或阶段数量,优先满足用户要求;与模板默认结构冲突时以用户要求为准,但知识点只能引用给定列表;
8. 只输出 JSON。"#;

pub const STUDY_RESOURCES: &str = r#"你是学习资料顾问。为每个学习阶段推荐学习资源,只输出一个 JSON 对象,不包代码围栏,不输出任何其他文本。

Schema:
{"resources":[{"stage":0,"items":["资源建议(书/官方文档/课程/实践项目 + 一句推荐理由,≤40字)"]}]}

规则:
1. stage 为阶段序号,必须覆盖全部阶段,每阶段 2-4 条;
2. 只推荐真实存在的经典教材、官方文档与公开资源;不确定是否真实存在的一律不写;严禁编造书名、作者、URL、课程名;
3. 只给资源名称与推荐理由,不要给链接;
4. 优先经典教材、官方文档与动手实践项目;
5. 用户明确要求优先:若用户指定了资料类型(如只要视频、只要官方文档),优先满足用户要求;与模板默认偏好冲突时以用户要求为准,但不得编造资源;
6. 只输出 JSON。"#;

pub const STUDY_GUIDE: &str = r#"你是自学材料作者。为给定的一个学习知识点写一份可直接阅读的自学材料,输出 Markdown 正文(不要输出 JSON,不要把全文包进代码围栏)。

呈现原则:
1. 按学科选择呈现方式:优先用结构化列表、对比表格、引用块(>)与加粗重点,让读者一眼看到重点;不要把全部内容写成代码块;
2. 只有编程类内容才用围栏代码块;确需逐字对齐的文本(如程序输出、命令行)才用代码块,其余一律用普通 Markdown;
3. 数学、物理等含公式的内容必须用 LaTeX:行内公式 $...$,独立公式 $$...$$;严禁把公式放进代码块或写成纯文本;
4. 概念关系较复杂时可用 mermaid 流程图(```mermaid)表达,节点文字只能来自本知识点内容,不得虚构事实或数据;
5. 用户明确要求优先:若用户指定了内容范围、结构或呈现格式,优先满足用户要求,其次才遵循本模板默认结构;冲突时以用户要求为准,但不得违反内容真实性与客观正确性。

结构要求:
1. 以 `# <知识点名>` 一级标题开头;
2. 概念讲解:讲清是什么 / 为什么 / 怎么用,按学科附录给出的教学法组织(如数学的概念→例题→变式,英语的分项训练,语文的积累→拆解→迁移);
3. 重点标记:核心定义、易混点、关键步骤用加粗或表格强调,不要平铺直叙;
4. 示例:按学科给出合适形式的示例(数学/物理给例题与推导步骤,英语给例句与对话,语文给可模仿片段,编程给可运行代码);
5. 常见误区:列出 2-3 个初学者易错点并说明原因;
6. 小结自测:文末给 3 条自测问题,只列问题不给答案。

规则:
1. 内容完整:涉及可枚举清单(高频词、公式表、语法点、固定搭配、名句、常用命令等)时,按指定范围一次性列全并逐条给出释义或用法,禁止只写「若干」「例如」「常见的有」再给两三个例子;范围未指定时按本学段常用范围列全并标注数量;
2. 只列有把握的内容:清单中每条都须真实、准确、可核对;无法确认的条目直接省略并在文末注明「未收录部分依据不足」,不得编造凑数;
3. 内容自洽,不引用外部链接;按给定难度(1-5)调整深浅;
4. 只讲当前知识点,不展开其他知识点;需引用前置知识时一句话带过,不展开讲解;
5. 知识点表述须符合学界共识与国家课程标准(中小学内容);不确定的史实、公式、定义宁可不写,禁止编造;示例必须真实可验证;
6. 篇幅一般约 800-1500 字;清单型知识点以内容完整为先,可超出该范围;
7. 只输出 Markdown 正文。"#;

pub const STUDY_QUIZ: &str = r#"你是出题老师。针对给定的知识点出恰好 5 道测验题,只输出一个 JSON 对象,不包代码围栏,不输出任何其他文本。

Schema:
{"questions":[{"idx":0,"type":"choice","stem":"题干","options":["A. 选项一","B. 选项二","C. 选项三","D. 选项四"],"answer":"A","explanation":"一句话解析"},{"idx":1,"type":"judge","stem":"判断题题干","answer":"对","explanation":"一句话解析"},{"idx":2,"type":"calc","stem":"计算题题干","answer":"可核对的数值或最简表达式","explanation":"一句话解析"},{"idx":4,"type":"short","stem":"简答题题干","answer":"参考答案要点","explanation":"一句话解析"}]}

规则:
1. 恰好 5 题,idx 从 0 递增;默认结构为至少 2 道 choice、1 道 judge、至多 2 道 short;
2. 当且仅当学科附录指定学科题型时,可用学科题型替代 short/choice 名额,但至少保留 1 道 choice 或 judge 客观题;学科题型:calc(计算题,answer 为可核对的数值或最简表达式)、proof(证明题,answer 为证明要点)、cloze(填空题,题干用____标出空位,answer 为所填词语)、translate(翻译题,answer 为参考译文)、reading(阅读理解题,answer 为答题要点)、recite(背诵默写题,answer 为原文);
3. choice 的 answer 必须是选项字母(A/B/C/D);judge 的 answer 只能是「对」或「错」;short 的 answer 为参考答案要点;
4. 题目覆盖该知识点的核心概念,难度显式分档:2 题熟悉级(概念识记)、2 题综合级(理解应用)、1 题挑战级(迁移综合),按由易到难排列;
5. 答案与解析必须正确可复核;客观事实以国家课程标准与权威教材为准,禁止编造;
6. 用户明确要求优先:若用户指定了题型、题量或考查重点,优先满足用户要求;与模板默认结构冲突时以用户要求为准,但答案仍须正确可复核;
7. 只输出 JSON。"#;

pub const STUDY_GRADE: &str = r#"你是严格的学习评价员。根据题干、参考答案与学习者作答,对一道主观题做出可解释的批改,只输出一个 JSON 对象,不包代码围栏,不输出任何其他文本。

Schema:
{"score":0到1之间的小数,"feedback":"逐条判定与依据(≤200字)"}

规则:
1. 只依据给定的题干与参考答案判定,不得引入题目之外的知识,不得编造学习者没有写出的内容;
2. 先把参考答案拆成若干要点,逐条标注「命中 / 部分命中 / 未命中」,再按命中比例给出 score;
3. 语义等价即可得分,不要求逐字一致;涉及公式、数值、单位时必须正确;
4. score ≥ 0.6 视为答对;作答空白或与题目无关时 score=0;
5. 信息不足以判定时,在 feedback 中明确写「依据不足」,score 取保守值,不得臆断;
6. feedback 用中文,最多 5 条要点,每条都要引用题干或参考答案中的依据;
7. 只输出 JSON。"#;

// ---------------- 学科附录(官方课标口径,严禁捏造) ----------------

/// 数学学科附录(教学法 + 《普通高中数学课程标准(2017年版2020年修订)》六素养)。
pub const SUBJECT_APPENDIX_MATH: &str = r#"[学科附录 · 数学]
1. 教学方法:严格遵循「概念讲解 → 典型例题 → 变式训练 → 错题归因」的递进结构;每个核心概念至少配 1 道例题与 1 道变式题,变式题与原题同知识点不同题面;
2. 素养导向:目标与成果表述对接数学学科核心素养——数学抽象、逻辑推理、数学建模、直观想象、数学运算、数据分析,明确指出所培养的具体素养维度;
3. 目标表述:凡涉及「学到什么程度」的描述(为什么重要、阶段目标、里程碑等),采用「素养维度 × 学段 × 可观测行为」三元表述(如「数学运算素养:能独立完成一元二次方程求根,含判别式讨论」),不写笼统口号;
4. 测验题型:可出 calc(计算题)与 proof(证明题);
5. 材料呈现:公式一律用 LaTeX(行内 $...$,独立 $$...$$)并给出推导步骤;例题与变式题按编号步骤书写;概念对比、公式对照用表格;严禁把公式放进代码块或写成纯文本。"#;

/// 英语学科附录(《普通高中英语课程标准(2017年版2020年修订)》四素养)。
pub const SUBJECT_APPENDIX_ENGLISH: &str = r#"[学科附录 · 英语]
1. 教学方法:按学段控制语言难度(词汇量、句式复杂度、语篇长度与学段匹配);听、说、读、写分项训练,各阶段明确本阶段侧重分项并给出对应练习;
2. 素养导向:目标与成果表述对接英语学科核心素养——语言能力、文化意识、思维品质、学习能力;
3. 目标表述:凡涉及「学到什么程度」的描述,采用「素养维度 × 学段 × 可观测行为」三元表述(如「语言能力:能读懂四级难度的说明文并概括主旨」),不写笼统口号;
4. 测验题型:可出 cloze(填空题)与 translate(翻译题);
5. 材料呈现:词汇、语法规则与搭配用表格;例句、对话与语篇片段用引用块(>);不使用代码块;听力与口语练习以文字指令加示例对话给出。"#;

/// 物理学科附录(《普通高中物理课程标准(2017年版2020年修订)》四素养)。
pub const SUBJECT_APPENDIX_PHYSICS: &str = r#"[学科附录 · 物理]
1. 教学方法:严格遵循「概念建立 → 公式推导 → 情境/实验分析 → 解题应用」的递进结构;公式必须给出推导过程,情境必须具体真实;
2. 素养导向:目标与成果表述对接物理学科核心素养——物理观念、科学思维、科学探究、科学态度与责任;
3. 目标表述:凡涉及「学到什么程度」的描述,采用「素养维度 × 学段 × 可观测行为」三元表述(如「科学思维:能对匀变速直线运动情境建立模型并求解」),不写笼统口号;
4. 测验题型:可出 calc(计算题)与 proof(证明题);
5. 材料呈现:公式一律用 LaTeX 并给出推导步骤,量纲与单位写清;实验情境、数据用表格;现象描述用引用块;严禁把公式放进代码块。"#;

/// 语文学科附录(《普通高中语文课程标准(2017年版2020年修订)》四素养)。
pub const SUBJECT_APPENDIX_CHINESE: &str = r#"[学科附录 · 语文]
1. 教学方法:遵循「素材积累 → 结构拆解 → 写作迁移」的递进结构;阅读与写作联动,每个单元给出可模仿的片段与迁移练习;
2. 素养导向:目标与成果表述对接语文学科核心素养——语言建构与运用、思维发展与提升、审美鉴赏与创造、文化传承与理解;
3. 目标表述:凡涉及「学到什么程度」的描述,采用「素养维度 × 学段 × 可观测行为」三元表述(如「语言建构与运用:能写出结构完整的议论文,论据与论点匹配」),不写笼统口号;
4. 测验题型:可出 reading(阅读理解题)与 recite(背诵默写题);
5. 材料呈现:原文、范文与名句用引用块(>);结构与手法拆解用表格;写作迁移给出可模仿的片段与改写练习;不使用代码块。"#;

/// 编程学科附录(保持现状:可运行示例优先)。
pub const SUBJECT_APPENDIX_PROGRAMMING: &str = r#"[学科附录 · 编程]
1. 教学方法:概念讲解必须配可运行示例;示例代码完整、可直接执行,并给出预期输出;练习以动手任务为主,避免纯理论罗列。
2. 材料呈现:示例代码用围栏代码块并标注语言,关键行用注释说明;预期输出用独立的代码块或表格;概念与 API 对比用表格。"#;

/// 义务教育阶段总要求附录(《义务教育课程方案和课程标准(2022年版)》)。
pub const SUBJECT_APPENDIX_COMPULSORY2022: &str = r#"[学段附录 · 义务教育阶段]
依据《义务教育课程方案和课程标准(2022年版)》总要求组织教学:坚持素养导向,强化学科实践(做中学、真实情境),推进综合学习,落实因材施教;学习成果描述参照学业质量标准,明确「学到什么程度」的具体表现。"#;

/// 学科附录 key(templates.json 可按同 key 覆盖)。
#[allow(dead_code)] // P1 模板编辑 UI 启用
pub const APPENDIX_KEYS: [&str; 7] = [
    "subject_appendix_math",
    "subject_appendix_english",
    "subject_appendix_physics",
    "subject_appendix_chinese",
    "subject_appendix_programming",
    "subject_appendix_other",
    "subject_appendix_compulsory2022",
];

// ---------------- 学习者配置 ----------------

/// 学习者配置:由意图/计划字段装配,产出配置块(拼用户输入开头)与学科附录(拼模板末尾)。
#[derive(Debug, Clone, Default)]
pub struct LearnerProfile {
    /// 学科展示名(原文,可空)
    pub subject: String,
    /// 归一附录 key(math/english/physics/chinese/programming/other)
    pub subject_key: String,
    /// 学段归一值(primary/junior/senior/university/adult,可空)
    pub grade_band: String,
    /// 考试目标(可空)
    pub exam_target: String,
    /// 每天可投入分钟
    pub daily_minutes: u32,
    /// 用户对内容范围/结构/呈现格式的明确要求(空 = 无,按模板默认)
    pub preferences: String,
    /// 知识点难度(单元/测验生成时有)
    pub difficulty: Option<i64>,
}

impl LearnerProfile {
    /// 从意图装配(流水线三步骤用)。
    pub fn from_intent(intent: &crate::study::Intent) -> Self {
        let key = crate::study::subject::normalize_subject(&intent.subject);
        Self {
            subject: intent.subject.trim().to_string(),
            subject_key: key.as_str().to_string(),
            grade_band: intent.grade_band.clone(),
            exam_target: intent.exam_target.clone().unwrap_or_default(),
            daily_minutes: intent.daily_minutes,
            preferences: intent.preferences.trim().to_string(),
            difficulty: None,
        }
    }

    /// 未识别出任何学科/学段/考试目标,且用户无明确要求 → 走通用模板,行为与旧版完全一致。
    pub fn is_generic(&self) -> bool {
        (self.subject_key.is_empty() || self.subject_key == "other")
            && self.grade_band.is_empty()
            && self.exam_target.is_empty()
            && self.preferences.trim().is_empty()
    }

    /// 学习者配置块;generic 时返回空串(不拼接)。
    pub fn config_block(&self) -> String {
        if self.is_generic() {
            return String::new();
        }
        let mut parts = Vec::new();
        if !self.subject.is_empty() || !self.subject_key.is_empty() {
            let subject = if self.subject.is_empty() {
                crate::study::subject::subject_label(&self.subject_key)
            } else {
                self.subject.as_str()
            };
            if !subject.is_empty() {
                parts.push(format!("学科={subject}"));
            }
        }
        if !self.grade_band.is_empty() {
            parts.push(format!(
                "学段={}",
                crate::study::subject::grade_band_label(&self.grade_band)
            ));
        }
        if !self.exam_target.is_empty() {
            parts.push(format!("考试目标={}", self.exam_target));
        }
        if !self.preferences.trim().is_empty() {
            parts.push(format!("用户明确要求={}", self.preferences.trim()));
        }
        if let Some(d) = self.difficulty {
            parts.push(format!("难度={d}"));
        }
        parts.push(format!("每日投入={}分钟", self.daily_minutes));
        format!("[学习者配置] {}\n\n", parts.join(" "))
    }

    /// 学科附录文本(追加在 system 模板末尾);other/空 key 返回空串。
    /// 义教学段(primary/junior)追加 2022 版课标总要求。
    pub fn appendix(&self) -> String {
        let key = self.subject_key.as_str();
        if key.is_empty() || key == "other" {
            return String::new();
        }
        let mut out = format!("\n\n{}", effective(&format!("subject_appendix_{key}")));
        if matches!(self.grade_band.as_str(), "primary" | "junior") {
            out.push_str("\n\n");
            out.push_str(&effective("subject_appendix_compulsory2022"));
        }
        out
    }
}

/// 内置模板内容。
pub fn builtin_content(key: &str) -> Option<&'static str> {
    match key {
        "study_intent" => Some(STUDY_INTENT),
        "study_map" => Some(STUDY_MAP),
        "study_stages" => Some(STUDY_STAGES),
        "study_resources" => Some(STUDY_RESOURCES),
        "study_guide" => Some(STUDY_GUIDE),
        "study_quiz" => Some(STUDY_QUIZ),
        "study_grade" => Some(STUDY_GRADE),
        "subject_appendix_math" => Some(SUBJECT_APPENDIX_MATH),
        "subject_appendix_english" => Some(SUBJECT_APPENDIX_ENGLISH),
        "subject_appendix_physics" => Some(SUBJECT_APPENDIX_PHYSICS),
        "subject_appendix_chinese" => Some(SUBJECT_APPENDIX_CHINESE),
        "subject_appendix_programming" => Some(SUBJECT_APPENDIX_PROGRAMMING),
        "subject_appendix_other" => Some(""),
        "subject_appendix_compulsory2022" => Some(SUBJECT_APPENDIX_COMPULSORY2022),
        _ => None,
    }
}

fn path() -> PathBuf {
    super::lc::templates_file()
}

/// 读取全部自定义覆盖(文件缺失/损坏返回空表)。
pub fn load_overrides() -> Vec<TemplateOverride> {
    let Ok(text) = fs::read_to_string(path()) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// 查某 key 的自定义覆盖。
pub fn find_override(key: &str) -> Option<TemplateOverride> {
    load_overrides().into_iter().find(|t| t.key == key)
}

/// 生效模板:自定义覆盖 > 内置。
pub fn effective(key: &str) -> String {
    if let Some(o) = find_override(key)
        && !o.content.trim().is_empty()
    {
        return o.content;
    }
    builtin_content(key).unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_content_available_for_all_keys() {
        for k in TEMPLATE_KEYS {
            assert!(!builtin_content(k).unwrap_or_default().is_empty());
        }
    }

    #[test]
    fn effective_falls_back_to_builtin() {
        assert_eq!(effective("study_map"), STUDY_MAP);
        assert_eq!(effective("no_such_key"), "");
    }

    #[test]
    fn appendix_selected_by_subject_key() {
        let p = LearnerProfile {
            subject: "数学".into(),
            subject_key: "math".into(),
            grade_band: "senior".into(),
            daily_minutes: 60,
            ..Default::default()
        };
        let a = p.appendix();
        assert!(a.contains("数学抽象"), "数学附录含六素养");
        assert!(a.contains("变式"), "数学附录含变式训练");
        assert!(!a.contains("义务教育"), "高中学段不追加义教附录");

        // 义教学段追加 2022 版课标总要求
        let mut p2 = p.clone();
        p2.grade_band = "junior".into();
        let a2 = p2.appendix();
        assert!(a2.contains("义务教育课程方案和课程标准(2022年版)"));

        // other / 空 key 无附录(行为同现状)
        let g = LearnerProfile::default();
        assert!(g.appendix().is_empty());
        let mut g2 = LearnerProfile::default();
        g2.subject_key = "other".into();
        assert!(g2.appendix().is_empty());
    }

    #[test]
    fn config_block_format_and_generic_skip() {
        let p = LearnerProfile {
            subject: "数学".into(),
            subject_key: "math".into(),
            grade_band: "senior".into(),
            exam_target: "高考".into(),
            daily_minutes: 45,
            preferences: String::new(),
            difficulty: Some(3),
        };
        let block = p.config_block();
        assert!(
            block.starts_with("[学习者配置] 学科=数学 学段=高中 考试目标=高考 难度=3 每日投入=45分钟"),
            "配置块格式:{block}"
        );
        assert!(block.ends_with("\n\n"));

        // 未识别学科 → 空块(行为同现状)
        assert!(LearnerProfile::default().config_block().is_empty());

        // 无学科但有学段 → 仍出配置块,学科用展示名兜底
        let q = LearnerProfile {
            subject_key: "other".into(),
            grade_band: "primary".into(),
            daily_minutes: 30,
            ..Default::default()
        };
        let b = q.config_block();
        assert!(b.contains("学科=通用"), "空学科用展示名:{b}");
        assert!(b.contains("学段=小学"));

        // 用户明确要求 → 即使无学科也进配置块
        let r = LearnerProfile {
            preferences: "多给表格和重点".into(),
            daily_minutes: 30,
            ..Default::default()
        };
        let rb = r.config_block();
        assert!(rb.contains("用户明确要求=多给表格和重点"), "偏好注入:{rb}");
    }

    #[test]
    fn guide_is_subject_aware_not_code_only() {
        let g = effective("study_guide");
        assert!(!g.contains("代码用围栏代码块"), "不得把所有示例固定为代码块");
        assert!(g.contains("LaTeX"), "数学/物理公式须要求 LaTeX 渲染");
        assert!(g.contains("用户明确要求优先"), "用户格式要求须优先");
        // 英语/语文明确禁用代码块,数学/物理要求 LaTeX
        assert!(effective("subject_appendix_english").contains("不使用代码块"));
        assert!(effective("subject_appendix_chinese").contains("不使用代码块"));
        assert!(effective("subject_appendix_math").contains("严禁把公式放进代码块"));
        assert!(effective("subject_appendix_programming").contains("围栏代码块"));
    }

    #[test]
    fn appendix_builtin_available_for_all_keys() {
        for k in APPENDIX_KEYS {
            assert!(builtin_content(k).is_some(), "缺附录:{k}");
        }
        // other 附录内置为空串(走通用模板)
        assert_eq!(effective("subject_appendix_other"), "");
        assert!(effective("subject_appendix_math").contains("数学"));
    }
}
