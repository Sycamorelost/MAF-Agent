# nature-shared

## 一、定位与触发场景

- **用途**：供其他 Nature Skills 按需引用的**内部共享参考包**，
  本身不是面向用户的独立工作流。
- **内容覆盖**：引用伦理、研究合规、论文类型分类、读者工作流、术语台账、
  一致性扫描、主文纪律、Nature 系章节写法、期刊格式约束。
- **description 原文摘录**（SKILL.md frontmatter）：

  > "Internal shared-reference support package for installed Nature Skills,
  > including nature-writing, nature-polishing, nature-response, nature-reader,
  > and nature-paper2ppt. Do not invoke it as a standalone user workflow.
  > Load only the specific core or journal-format file requested by another Nature skill."

- **典型触发方式**：不直接被用户触发；
  当其他 skill 的 manifest 以相对路径（如 `../nature-shared/core/ethics.md`）
  声明依赖时，由宿主 skill 加载指定文件。
- **明确不做什么（边界）**：
  - 不作为 standalone 用户工作流被调用。
  - 只加载被引用的具体文件，**不整包预载**。
  - corpus-derived 文件（`nature-introduction.md` / `nature-abstract.md` /
    `nature-results-discussion.md`）来自语料归纳，**不是官方期刊政策**，
    使用时须与官方格式文件区分。

## 二、运作模式（加载/路由机制）

### SKILL.md 作为路由

frontmatter 说明用途与边界；正文规定加载纪律：
only-load-what-is-referenced、不整包预载、corpus-derived 文件非官方政策。

### manifest.yaml 结构（version 1.6.0）

- **always_load**：`[]`（空，全部按需加载）。
- **core.on_demand**（11 条，条件→路径）：
  - 引用伦理 + AI 风险分级 + Nature Portfolio 投稿门 → `core/ethics.md`
  - 人/动物/临床/图像/结构/化学/分类学合规门 → `core/research-compliance.md`
  - 论文类型 5 分类与检测规则 → `core/paper-type-taxonomy.md`
  - 读者视角五问 → `core/reader-workflow.md`
  - 动笔前术语台账 → `core/terminology-ledger.md`
  - 事后一致性漂移审计 → `core/consistency-sweep.md`
  - 主文 vs SI 证据分配纪律 → `core/main-text-discipline.md`
  - Nature 系 Results/Discussion 写法 → `core/nature-results-discussion.md`（corpus-derived）
  - Discussion 论证语言与情态校准 → `core/discussion-argument-language.md`
  - Nature 系 Introduction 写法 → `core/nature-introduction.md`（corpus-derived）
  - Nature 系 Abstract 写法 → `core/nature-abstract.md`（corpus-derived）
- **journal_formats.on_demand**（3 条）：
  - 旗舰 Nature 分阶段格式 → `journal-formats/nature.md`
  - Nature Communications → `journal-formats/nat-comms.md`
  - Nature Machine Intelligence → `journal-formats/nature-machine-intelligence.md`
- **quality_tools**：
  `consistency_script = scripts/check_consistency.py`
  （相对 nature-shared 包目录解析，**绝不用用户 cwd**）。
- **被引用方式**：其他 skill 的 manifest 用相对路径
  `../nature-shared/core/xxx.md` 或 `../nature-shared/journal-formats/xxx.md`。
- **README 登记的使用方**：
  - reader-workflow / paper-type-taxonomy / ethics → polishing + writing；
  - terminology-ledger → polishing / writing / reader / paper2ppt；
  - consistency-sweep → polishing / reviewer / response / statistics；
  - main-text-discipline → writing / polishing / response；
  - journal-formats/nature-machine-intelligence →
    NMI 写作/润色/图表/数据/统计工作流。

## 三、核心内容要点（按文件）

### core/ethics.md

- 引用伦理 + AI 使用风险三级：
  **Green**（assistive，辅助性）、**Amber**（evaluative，评估性）、**Red**（禁止）。
- Nature Portfolio 投稿门：**LLM 不能署名**；使用须披露；
  政策源 2026-08-08 验证。

### core/research-compliance.md

人类受试者、动物、临床、图像完整性、结构数据、化学品、分类学等合规检查门。

### core/paper-type-taxonomy.md

- 5 种论文类型：**research / methods / hypothesis / algorithmic / review**；
  附检测规则与 legacy 词到 5 型的映射。
- 各宿主 skill 的 `paper_type` fragments 是在此分类之上的 **action layer**。

### core/reader-workflow.md

读者五问：**Relevance → Novelty → Trust → Reuse → Meaning**；
其中 boundaries（边界）最常被作者跳过。

### core/terminology-ledger.md

动笔前建术语台账：canonical term / first-use / variants / decision；
锁定后只用 canonical，不造新术语。

### core/consistency-sweep.md

- 事后漂移审计，12 个 axis：
  experimental factors / physical objects / acronyms / units /
  statistical terms / number formatting / self-reference / tense /
  hyphenation / spelling / cross-references 等。
- 技术红线：confidence interval 与 prediction interval 混用是**技术错误**；
  headline counts 必须能从 Methods 推出。
- 执行顺序：numeric → terminology → redundancy → recompile。
- 配套脚本：
  `python scripts/check_consistency.py manuscript.tex --term-group 'a=x|y'`，
  支持 `--json` / `--fail-on-findings`。

### core/main-text-discipline.md

- 8 类结果分类：
  core_discovery / necessary_support / qualification / robustness /
  heterogeneity / provenance_detail / alternative_inference / edge_case
  → 分别归 main text / caption / SI。
- 最短证据链原则；
  revision accretion（返修堆积）删除检查；
  主文只放主推断统计；
  paragraph necessity test；
  claim-repetition map；
  compression record 六件套。

### core/nature-results-discussion.md（corpus-derived）

- Results = claim escalation：
  observation → question → experiment → perturbation →
  local interpretation → stronger claim。
- 3 种 evidence-chain archetype：
  discovery loop / core capability + validation envelope / capability ladder。
- diagnostic perturbation；local-interpretation gate 四条件。
- Discussion = synthesis：
  anchor → cross-Results synthesis → prior work → importance → boundary → implications。

### core/discussion-argument-language.md

- 反向漏斗；
- 四功能：Anchor / Position / Interpret and contribute
  （三层 established / supported / open）/ Bound and extend；
- 情态动词与证据强度对照：
  shows / establishes vs suggests vs may / might vs must / cannot；
- modal safeguards；limitations 写法；sentence-function audit 8 项。

### core/nature-introduction.md（corpus-derived）

- 问题漏斗；
- exact knowledge gap 句式
  "It remains unclear whether, why, or under what conditions ___"；
- 文献按论证功能组织；
- question-first novelty；
- 结尾 compact research route；
- Introduction–Results 对齐 reverse-outline 表。

### core/nature-abstract.md（corpus-derived）

- 摘要 = 最短证据链：gap → design → main discovery → support/boundary → payoff；
- discovery-centred 六步 move order；
- one main claim 层级表；
- 数字按必要使用；
- 结尾 conceptual payoff；
- audit 表。

### journal-formats/nature.md

- 旗舰 Nature Article 分阶段：
  initial_submission / revision / accepted_in_principle / proof；
- 结构 16 项序列；
- title ≤75 字符；summary paragraph ~200 词；
- 6 页 Article ~2500 词 + 4 display items；
- 8 页 ~4300 词 + 5–6 items；
- ~50 references；Methods ≤~3000 词；
- Extended Data ≤10；
- SI 规则（SIGuide.doc / ≤10 文件 / ≤150MB）；
- cover letter 可选；preprint 政策；
- 官方源 2026-08-08 验证。

### journal-formats/nat-comms.md

- Article ~5000 词含 Methods；abstract 150 词；refs ~60；
- display ≤10 且无 Extended Data；
- 分辨率 1200/300/600 dpi、RGB、89/183mm、Arial 5–7pt；
- mandatory statements；
- transparent peer review 默认开启；
- 常见 desk-reject 模式；
- 从 Nature 转移的注意事项。

### journal-formats/nature-machine-intelligence.md

- Article main text ≤3500 词；abstract ≤150；display ≤6；refs ≤50；
- Analysis 摘要 100–150 词；
- 结构 Intro（无标题）/ Results / Discussion / Methods；
- cover letter **必需**；不接受 presubmission enquiry；
- Data/Code availability 含 Software Submission Checklist；
- figure legend 300 词为 2018 历史建议上限（非现行硬限，建议 150–250）；
- production 300dpi / 180mm / 5–7pt sans-serif；
- 2026-08-14 验证。

## 四、输出契约与质量检查

- 本包不产出用户可见交付物；输出契约由宿主 skill 定义。
- **质量工具**：`scripts/check_consistency.py`——机械初筛：
  - 内置 self-reference 与 standard-deviation 变体组；
  - 用户自定义 `--term-group`；
  - 同值不同精度；
  - 等值长度单位混用（如 mm/cm）；
  - Finding / Occurrence dataclass；
  - **只警告不改稿**。
- 脚本路径相对 nature-shared 包目录解析，never user cwd。

## 五、关键规则与红线

1. **只加载被引用文件**，不整包预载。
2. **corpus-derived ≠ 官方政策**：
   nature-introduction / nature-abstract / nature-results-discussion
   三个文件不得当作 Nature 官方政策引用；
   官方数字以 journal-formats 三个文件为准（各自标注验证日期）。
3. **伦理红线**：LLM 不能署名；AI 使用按 Green/Amber/Red 三级判断；
   Red 级禁止。
4. **一致性红线**：confidence vs prediction interval 混用是技术错误；
   headline counts 必须可从 Methods 推出。
5. **脚本只警告不改稿**。

## 六、配套资源

- `core/`：11 个核心文件（用途见第三节逐条）。
- `journal-formats/`：3 个期刊格式文件（见第三节）。
- `scripts/check_consistency.py`：一致性机械初筛脚本（用法见第四节）。
- `README.md`：包定位、加载纪律与各文件被哪些宿主 skill 使用的登记。
