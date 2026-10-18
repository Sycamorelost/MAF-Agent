# nature-writing

## 一、定位与触发场景

- **用途**：从**作者提供的证据**出发，起草或重构科学论文的论证、章节与
  首次投稿材料。
- **性质**：**Router** 型 skill，通过五轴路由加载对应 fragment。
- **description 原文摘录**（SKILL.md frontmatter）：

  > "Draft or restructure scientific manuscript arguments, sections, and
  > initial-submission materials from author-provided evidence. Use for
  > 论文写作、章节起草、论证重构、正文压缩、首次投稿材料.
  > Use nature-polishing for language-only edits to existing prose and
  > nature-response for post-decision correspondence."

- **典型触发请求**：
  - "根据这些实验笔记起草 Results 与 Discussion。"
  - "把这篇论文的 Introduction 论证逻辑重建。"
  - "把正文从 6000 词压缩到 Nature Article 预算内。"
  - "准备投 Nature Communications 的首次投稿包
    （cover letter、title page、highlights、declarations）。"
- **明确不做什么（Submission boundary）**：
  - 句子级语言润色 → `nature-polishing`；
  - 决定后通信（revision / rebuttal / appeal）→ `nature-response`；
  - graphical abstract → `nature-figure`；
  - 模拟审稿 → `nature-reviewer`；
  - 找文献 → nature-citation / nature-academic-search；
  - 本 skill **只管 initial submission**；
  - 不虚构 results / mechanisms / references / methods / novelty /
    sample sizes / statistics / limitations。

## 二、运作模式（加载/路由机制）

### SKILL.md 作为路由：5 步路由协议

1. 读 `manifest.yaml` + `always_load` 全部文件。
2. 按 `detect:` 提示判定**五轴**取值，
   起草前**用一行声明判定结果**（progress update 而非 approval gate）。
3. 只读被选中轴值映射的 fragment；
   `task=submission-package` 或无章节语境时跳过 section 轴。
4. 按优先级起草：
   core/stance → paper_type → section → task → journal → language（最后）。
5. `references/` 按 manifest 的 `references.on_demand` 表按需打开。

### manifest.yaml 结构（version 1.5.0）

- **always_load**（7 项，含 4 个共享层文件）：
  - `../nature-shared/core/reader-workflow.md`
  - `../nature-shared/core/paper-type-taxonomy.md`
  - `../nature-shared/core/ethics.md`
  - `../nature-shared/core/terminology-ledger.md`
  - `static/core/stance.md`
  - `static/core/workflow.md`
  - `static/core/output-format.md`
- **axes**（5 个）：
  - `task`（multi: false，default: manuscript）：
    `manuscript` / `submission-package`（仅首次投稿）
    → `static/fragments/task/<值>.md`
  - `paper_type`（multi: false，default: research）：
    `research` / `methods` / `hypothesis` / `algorithmic` / `review`
    → `static/fragments/paper_type/<值>.md`；旧 taxonomy 词映射到 5 型
  - `section`（**multi: true**，无 default）：
    `abstract` / `intro` / `related-work` / `method` / `experiments` /
    `discussion` / `conclusion` / `title`
    → `static/fragments/section/<值>.md`；含糊且要紧时问用户
  - `language`（default: en）：
    `en` / `zh-to-en`（按用户笔记语言判定）
    → `static/fragments/language/<值>.md`
  - `journal`（default: generic）：
    `nature`（仅旗舰）/ `nature-family` / `nat-comms` /
    `nat-mach-intell`（NMI）/ `generic`
    → `static/fragments/journal/<值>.md`
- **references.on_demand**（17 条，其中 8 条指向 nature-shared）：
  - Results/主文压缩、8 类证据分配
    → `../nature-shared/core/main-text-discipline.md`
    （Results 或主文压缩**必须加载**）
  - Nature 系目标按章节加
    → `../nature-shared/core/nature-results-discussion.md`、
    `nature-introduction.md`、`nature-abstract.md`
    （均为 corpus-derived，非官方政策）
  - 任何 Discussion
    → `../nature-shared/core/discussion-argument-language.md`
  - 研究合规门 → `../nature-shared/core/research-compliance.md`
  - 期刊格式 → `../nature-shared/journal-formats/nature.md`、
    `nat-comms.md`（经 fragment）、`nature-machine-intelligence.md`
  - 本包 14 条：
    - `references/article-architecture.md`：章节结构/论证顺序/已发表模式；
    - `references/nat-comms-2025-corpus.md`：2025 Nat Commun CS/AI 语料实证结构、
      连接词量化偏好、genre 差异；
    - `references/abstract.md`：challenge-contribution 等摘要形式；
    - `references/nature-summary-paragraph.md`：Nature 宽读者 summary paragraph；
    - `references/introduction.md`：任务框定/技术挑战/贡献框定；
    - `references/related-work.md`：topic synthesis；
    - `references/method.md`：pipeline 模块/模块动机/技术优势；
    - `references/experiments.md`：baselines/ablations/metrics；
    - `references/conclusion.md`：bounded conclusion；
    - `references/paragraph-flow.md`：reverse outlining；
    - `references/paper-review.md`：终稿自审/拒稿风险/claim-evidence 对齐；
    - `references/submission-package.md`：cover letter/title page/highlights/
      declarations/审稿人/完整性审计；
    - `references/chinese-author-workflow.md`：中文作者修复模式；
    - `references/examples/index.md`：具体例子索引。

### 对 nature-shared 的依赖

always_load 4 条 + on_demand 8 条，均以 `../nature-shared/` 相对路径引用。

## 三、核心工作流

### static/core/stance.md（立场）

- 作者证据优先；
- 先写论证，后写句子；
- claim discipline：ambitious but bounded；
  动词校准 show / demonstrate / suggest / indicate / enable / may / could；
  删 unsupported novelty 与 universal claims
  （first ever / unprecedented / revolutionary）；
- Intake 清单：section / paper type / core claim / evidence / boundary /
  target journal / terminology——**首接触材料即建 Terminology Ledger**；
- 缺字段不全面停笔，用 placeholder。

### static/core/workflow.md（9 步）

1. 一句话论证模板：
   "In [system/problem], we show [advance] using [approach],
   supported by [evidence], with [boundary]."
2. （1b）建 Terminology Ledger。
3. 选 section 架构；每段一职
   （context / gap / approach / result / comparison / mechanism /
   implication / limitation；一段两职则拆）。
4. （3a）Results 证据分配：
   加载 main-text-discipline，8 类分类，最短充分证据链，
   不把改结论的证据埋 SI。
5. （3b）scoped alignment check：
   重大问题才问、非 approval gate；
   用户要 outline 才先给 outline；
   voice 不可推断时索要写作样本，只校准风格。
6. 从证据向外写；
   动词校准：show/demonstrate=强直接证据；
   suggest/indicate=趋势/间接；may/could=未验证机制。
7. 扫除 first / unique / unprecedented / comprehensive / complete /
   always / never。
8. paragraph-flow 检查：
   一段一信息、首句 topic/claim、后续句有显式关系。
9. 返回 prose+notes；
   targeted edit 非全文重写（只改标记段、保持 Terminology Ledger、
   只重跑相关检查、新增句触发 main-text 删除检查）。

### fragments/paper_type（5 型：论证链 + drafting order）

- **research**：
  argument chain = field-scale need → unresolved bottleneck → proposed move →
  decisive evidence → broader implication → boundary；
  drafting order = **Results → Intro+Conclusion → Title → Discussion →
  Methods → Authors → Abstract**（不得先写 Intro）；hourglass 结构。
- **methods**：
  链 = task → limits → method → evaluation → reproducibility → boundary；
  Methods 先写；Results 须答可靠性/速度成本/资源/公平可复现；
  Methods 深度含 axioms / 环境 / 推导 / 评估协议 / failure modes。
- **hypothesis**：
  链 = phenomenon → candidate explanations → explicit hypothesis →
  falsification path → evidence → rival explanations → bounded conclusion；
  假设显式可定位；预先写证伪观察；
  区分 supporting vs consistent-but-non-discriminating；
  Results 只观察，Discussion 才因果解释。
- **algorithmic**：
  链 = task definition → what → why works → how well → ablations →
  failure modes+cost → applicability boundary；
  三者不混段；每个性能声称带 dataset/metric/baseline/conditions；
  避 marketing verbs（leverages / enables / empowers）；
  Discussion 须命名失败模式；
  模块写作三要素 motivation / mechanism / evidence。
- **review**：
  链 = scope → organizing principle → synthesis by argument →
  disagreements/gaps → author stance with reasoning → outlook；
  不是 survey list；
  连接词用 in contrast / building on this，避 furthermore / additionally；
  结尾给领域地图。

### fragments/section（8 节，要点）

- **abstract**：最短证据链而非压缩 Intro，**最后写**；
  默认 Nature 模式：precise problem/gap → answer-enabling design →
  main discovery → decisive support/boundary → implication；
  技术文三变体见 `references/abstract.md`（V1 challenge→contribution /
  V2 challenge→insight→contribution / V3 multiple contributions）；
  诊断：Here, we 开头=缺背景、宽承诺结尾=需收 scope；
  one main claim + 至多 1–2 支撑；数字仅在定义/支撑/限定核心 claim 时使用。
- **intro**：漏斗 important problem → specific phenomenon → prior work →
  exact unresolved gap → research question → present study；
  pipeline 变体 7 种（task-then-application / application-first /
  general-to-specific-setting / open-with-challenge /
  novel-task-challenge-decomposition / technical-challenge 1-3 /
  pipeline-version 1-4，详见 `references/introduction.md`，须告知用户选了哪个）；
  规则：不详述结果、引用为定位非炫耀、gap 是 unknown 而非"作者方法缺失"、
  从 Results 倒推对齐；
  **警告**：不要先摆 naive solution 再说改进（显得像低分增量补丁）。
- **related-work**：topic scope → grouped by mechanism →
  limitation tied to this paper → distinction；
  按主题非年份/作者分组；每小节结尾连回本文 limitation；
  不贬不捧；不藏最强 baseline；引用实际读过的源（`references/related-work.md`）。
- **method**：task formulation → overview → per-module detail →
  implementation notes → assumptions/boundary；
  **三要素 motivation / mechanism / evidence，缺一即黑箱**；
  pre-writing questions（有哪些模块、各模块如何跑/为何需要/为何有效）；
  先画 pipeline figure sketch；
  先写 module design（结构定义 + input→steps→output 前向过程），
  再补 motivation 与 technical advantages；
  禁模糊语 under standard conditions / using routine methods /
  data were analyzed statistically / the method was validated /
  samples were randomly assigned（`references/method.md`）。
- **experiments**：证据梯 establish → stress-test → rule out alternatives →
  broaden → interpret → bound；每小节 claim-first；
  三个核心问题：优于强 baseline 吗 / 哪些模块带来增益 / 更难设定下泛化多远；
  图表规则：caption 在表上方、booktabs 风格、标 metric 方向（`PSNR ↑`）、
  数值精度一致、一表一信息、克制高亮；
  Recommended Ablation Package：一张核心消融表 + 若干 mini-ablation + 配套定性图；
  过去时为主；核心发现在主文、robustness 入 SI 除非改结论
  （`references/experiments.md`）。
- **discussion**：加载 discussion-argument-language；
  结构 central advance → what evidence means → prior work →
  limitations → future；综合非逐图重复；先 rival explanations 再推广；
  hedging 匹配证据；句法 may reflect / suggests that / could indicate /
  is likely due to / may facilitate；
  失败模式 7 条含滥用 must/cannot/demonstrates；
  口诀 **Results=证据链，Discussion=意义综合**。
- **conclusion**：contribution → decisive evidence → implication → boundary；
  无新数据、无空头承诺；
  limitation 偏好 tied to task goal/setting boundaries
  （data regime / assumption / deployment scope），
  区分 technical defect vs scope limitation；
  future-work 须具体；overclaim 4 问检查（`references/conclusion.md`）。
- **title**：模式 system/object + action/capability + application/consequence；
  可搜索、被数据支撑、curiosity 不失信；
  避 Toward… / A study of… / A new era… / Revolutionizing… / 疑问式 / jargon；
  备选 3–5 个，标最 defensible。

### fragments/language

- **en**：
  句 10–30 词、一句一命题、SVO、草稿避破折号；
  diction 校准动词、避 marketing/universal。
- **zh-to-en**：
  **译意不译句法**——把中文长句拆成
  claim / evidence / condition / comparison / implication / limitation，
  再按章节顺序写；
  修复表 8 行：
  先重要性后对象→早命名系统；
  方法列在缺口前→缺口前移；
  "显著提高"无基线→加 comparator 或软化动词；
  首次/创新性无范围→bounded novelty；
  相关推机制→suggests / is consistent with；
  结果含义混杂→Results/Discussion 分离；
  主题名词重复→代词；
  逗号短句串→拆分或加连接词；
  输出先英文后中文简注；
  深度见 `references/chinese-author-workflow.md`
  （补五类边缘修复：无时态 / 无复数 / `的…的`嵌套 /
  主题-评论开头 / 句句"本文/我们"开头）。

### fragments/journal

- **nature**（仅旗舰）：宽读者；摘要与 intro 首句向非专家传 significance
  但不过 claim；summary paragraph 漏斗（详见
  `references/nature-summary-paragraph.md`：
  broad field → sharper background → exact problem → here we show →
  what the result changes → broader context；
  ~190 词无结尾展望 / ~250 词含之；压缩靠删重复背景，
  而非删 problem 句或 here-we-show 铰链）；
  初投稿格式灵活，长度/display 预算作 readiness 检查而非拒稿理由；
  正文无破折号；hedging 双避 proves/definitive 与 might possibly suggest；
  Articles 以 fully referenced unstructured summary paragraph 开头；
  Methods 在图注后 ~3000 词；refs ~50 早规划；
  配套 `../nature-shared/journal-formats/nature.md` 与 research-compliance；
  专业机制过深则标出，不沉默修复。
- **nature-family**（其他子刊）：**不得自动借用**旗舰 75 字符 / 2500 / 4300 词 /
  Extended Data / 图注 / 引用预算；查该刊现行指引并记录来源与日期。
- **nat-comms**：先读 `../nature-shared/journal-formats/nat-comms.md`；
  ~5000 词含 Methods 的预算表：Intro ~700 / Results ~2000 /
  Discussion ~800 / Methods ~1500；Methods>1800 标预算风险；
  早规划 Data Availability（有效公开 accession）、
  Code Availability（Zenodo DOI）、Reporting Summary；
  cover letter 三句 what / what's new / why cross-disciplinary；
  display ≤10 无 Extended Data；abstract 150 词无引用、finding 先行、
  尽量含量化；从 Nature 转移需三 rework；
  `references/nat-comms-2025-corpus.md` 提供 2025 语料实证
  （五体裁 skeleton、摘要 5 moves、连接词频率
  However 51 ≫ Furthermore 22 > Therefore 19…、
  hedge 只用于意义句、中文迁移要点）。
- **nat-mach-intell**：先读 nature-machine-intelligence.md 与三个
  corpus-derived core 文件；读者跨 ML/机器人/AI 应用；
  不以更大模型/更多算力/小幅 benchmark 提升当 NMI 级 significance；
  Article Intro+Results+Discussion ≤3500 词、abstract ≤150 无引用、
  display ≤6、refs ~50、无标题 Intro、Discussion 不用小标题；
  建议预算 Intro 550–700 / Results 2100–2350 / Discussion 500–750；
  **MI evidence gates 6 项**：独立测试数据与泄漏控制 / baseline parity /
  不确定性与重复跑与消融 / robustness 与 OOD 限制 /
  算力软硬件环境可复现 / 真实世界主张的人群与前瞻验证；
  中心新代码需 Code availability + 审稿访问 + Software Submission Checklist；
  cover letter 必需；会议扩展须明示实质新结果；
  双盲时作者信息移 cover letter 并审计仓库/自引/致谢/元数据；
  不提供 presubmission enquiry；
  **数字不发明规则**（NMI 无公开 title/Methods 字数/现行图注上限，
  2018 的 300 词图注建议仅历史参考，建议 150–250）。
- **generic**：Nature 倾向结构但不强制最严限制；可选上下文。

### task=submission-package 流程

（fragments/task/submission-package.md + references/submission-package.md；
仅首次编辑决定前；9 步）

1. **Intake**：收集或标缺失（期刊/文章类型/作者与 ORCID/一句话 claim/
   经费/伦理批号/仓库链接/preprint 与关联稿件/建议与回避审稿人等）；
   **不推断未知行政事实**，用 `[AUTHOR_INPUT_NEEDED: ...]`。
2. **Deliverable matrix**：required / optional / N/A 逐项列表。
3. **Initial cover letter**：先判定 required / optional / not accepted；
   250–400 词；不重复 abstract；不用 "the first" / "unprecedented"；
   不 name-drop；fit 须连接期刊 scope；
   骨架：consider 句 → 研究什么+主发现 1-2 句 →
   新在哪+证据 1-2 句 → 为何适合本刊读者 1 句 → required declarations。
4. **Title page**：双盲时识别信息不进匿名稿。
5. **Highlights**（3–5 条，一条一发现，具体结果优先）与 editorial summary。
6. **Declarations**：CRediT / competing interests / funding / data / code /
   ethics / consent / AI 披露等；
   **never 把 "available on request" 写成公开仓库声明，
   never 编造角色/基金号/批号/accession/license**。
7. **Reviewer suggestions**：Name | Institution | Email | Expertise fit |
   Conflict check | Rationale；要求作者确认无利益冲突。
8. **Completeness audit**：readiness =
   `ready` / `ready_with_author_checks` / `blocked`。
9. **LaTeX 模板与输出六段**：Submission readiness: / Deliverable matrix: /
   Draft materials: / AUTHOR_INPUT_NEEDED: / Cross-file consistency checks: /
   Next actions:。

## 四、输出契约与质量检查

### 默认输出六段（static/core/output-format.md）

- `Draft:`
- `Section outline:`（3–7 bullets，全章节时）
- `Assumptions or missing inputs:`
- `Claim-evidence map:`（Claim | Evidence | Status:
  supported / needs evidence / inferred）
- `Why this structure:`（2–4 bullets）
- `To redirect me:`（一行邀请定点反馈）

其他规则：

- 中文笔记：先英文成稿，后中文简注；
- Results/主文重构加 `Main-text discipline audit:`
  （result allocation / relocated-replaced-compressed-deleted /
  主统计留主文次级入 SI / 前后字数）；
- 缺证据写 `[Evidence needed: …]` placeholder；
- task=submission-package 换六段（见上节）。

### 质量检查资源

- `references/paper-review.md`：对抗性自审——
  Critical Rule：Abstract/Introduction 每个 major claim 须
  technically correct 且有实验证据支撑，否则加证据或弱化/删除；
  5 个拒稿维度表（贡献不足 / 写作不清 / 实验效果弱 / 评估不完整 /
  方法设计有问题）；
  End-of-Paper 自审 5 组 25 问；
  Adversarial workflow（pass / needs revision / needs new experiment
  三态循环）。
- `references/paragraph-flow.md`：
  Reader test 5 问 + reverse outlining
  （section thesis → topic sentence → evidence 双向映射）+
  修复手法（拆双信息段、定义前移、过渡词显式化
  therefore / however / by contrast / for example / as a result、
  claim-first 开头）。
- `tests/scoped-drafting.md`：4 个行为 fixture——
  清晰段落请求不索要 outline approval、
  局部 follow-up 复用上下文、
  显式 outline 批准门等待、
  材料证据冲突时停下问权威来源。
- `references/article-architecture.md`：全论证链与各章 move order——
  abstract 6 步 movement + 3 诊断；
  intro 5 步漏斗；
  Results 证据梯 6 级 + 小节开头 "To test [question], we [action]."；
  Discussion 6 步 widening；
  Conclusion 4 段收束；
  Title 逻辑 + 避 vague prestige 词
  （novel / advanced / powerful / green / efficient）。

## 五、关键规则与红线

1. **不虚构**：results / mechanisms / references / methods / novelty /
   sample sizes / statistics / limitations；
   行政事实（身份 / ORCID / 经费 / 伦理批号 / 仓库链接 / 审稿人身份）
   只用户提供的。
2. **claim-evidence 对齐**：unsupported claim 要么加证据、要么弱化/删除；
   动词校准匹配证据强度。
3. **不沉默修复**：专业机制过深或结构问题须 flag 给用户，不掩盖。
4. **submission boundary**：只管 initial submission；
   revision/rebuttal 归 nature-response；
   **绝不复用** `nature-response/templates/cover-letter.tex`
   （那是返修信）。
5. **轴纪律**：不读所有 fragment，只读选中轴值；
   五轴判定结果先一行声明。
6. **期刊纪律**：nature 仅限旗舰；nature-family 不借用旗舰数字；
   NMI 数字不发明规则。
7. **scoped drafting**：targeted edit 只改标记段，非全文重写；
   材料冲突时停下问权威来源，不擅自选方向。

## 六、配套资源

- **static/core/**：
  `stance.md`（立场与 Intake）、`workflow.md`（9 步起草流）、
  `output-format.md`（输出六段）。
- **static/fragments/**：
  task 2 个、paper_type 5 个、section 8 个、language 2 个、
  journal 5 个（要点见第三节）。
- **references/**：
  `article-architecture.md`（章节 move order）、
  `abstract.md`（摘要三 Version + Example Bank）、
  `nature-summary-paragraph.md`（Nature summary paragraph 七段式）、
  `introduction.md`（任务框定 4 Version + 技术挑战 3 Version +
  pipeline 变体）、
  `related-work.md`、`method.md`（模块三要素）、
  `experiments.md`（三核心问题 + 图表规则 + 消融包）、
  `conclusion.md`、`paragraph-flow.md`、`paper-review.md`、
  `submission-package.md`、
  `nat-comms-2025-corpus.md`（2025 语料实证）、
  `chinese-author-workflow.md`（中文作者深度修复）、
  `examples/index.md`（例子总索引：abstract-examples /
  introduction-examples / method-examples 三个 index +
  abstract 3 个 template + introduction 12 个 + method 9 个具体例子文件；
  用法 = Pick template → 开对应 examples 文件 →
  复用句法逻辑而非原文措辞 → 保留引用链）。
- **templates/submission/**（4 个 LaTeX 模板，仅 initial submission）：
  - `initial-cover-letter.tex`：编辑用投稿信；
  - `title-page.tex`：署名标题页与 declarations；
  - `highlights-and-summary.tex`：highlights 与 editorial/significance summary；
  - `declarations-and-reviewers.tex`：投稿声明与审稿人建议；
  - 全部用 `AUTHOR_INPUT_NEEDED` 占位符，仅替换为作者确认事实。
- **tests/scoped-drafting.md**：scoped drafting 行为 fixture。
- **agents/openai.yaml**：display_name "Nature Writing"。
