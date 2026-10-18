# nature-statistics

## 一、定位与触发场景

- **用途**：审计或改进论文的**统计报告**，
  覆盖 experimental units、replication、uncertainty、tests、figure statistics。
- **性质**：reporting / review skill，**不是统计师的替代品**；
  仅在用户提供数据且明确要求时才计算新分析。
- **description 原文摘录**（SKILL.md frontmatter）：

  > "Audit or improve manuscript statistical reporting, including experimental
  > units, replication, uncertainty, tests, and figure statistics. Use for
  > 统计审查、统计方法小节、图注统计 and reviewer concerns; compute new
  > analyses only when requested with data."

- **典型触发请求**：
  - "审查我 manuscript 的 Statistical analysis 小节和图注统计是否合规。"
  - "帮我区分 biological replicates 和 technical replicates 的 n。"
  - "审稿人质疑 pseudoreplication，帮我起草回应。"
  - "按旗舰 Nature / NMI 的要求做统计投稿审计。"
- **明确不做什么（边界）**：
  - 不替代统计师；
  - 不能仅凭图像判断实验单位；
  - 不给超出报告检查的医学/监管/临床试验统计建议；
  - 不编造任何统计量。

## 二、运作模式（加载/路由机制）

### SKILL.md 作为路由

frontmatter 仅 `name` + `description`；正文规定：

- **Default stance 6 条**：
  1. 设计透明优先于装饰性统计语言；
  2. 分开三问："测了什么 / 分析单位是什么 / 主张什么推断"；
  3. **独立实验单位是默认 n**——不得把 cells / fields / repeated readings /
     spectra / model runs / technical replicates 当独立样本；
  4. 优先 effect sizes / uncertainty intervals / sample sizes /
     exact test definitions，而非 significance-only；
  5. 缺失信息标 `AUTHOR_INPUT_NEEDED`；
  6. 期刊/研究类型/领域标准更具体时从其，并标记来源。
- **Accepted inputs**：Statistical analysis/Methods 小节、
  含检验统计量的 Results 段落、图版/图注/source-data notes、
  审稿统计意见、中英文作者笔记、比较表、原始或汇总数据（仅当用户要再分析）。
- 输入不全时做 bounded audit 并声明不可评估部分。
- 9 步工作流、输出契约与红线（见下文）。

### manifest.yaml 结构（version 1.3.0）

- **always_load**（2 项）：
  - `references/source-basis.md`：Source hierarchy 与用语分寸。
  - `references/statistical-reporting.md`：统计报告提取与起草规则。
- **axes**：无。
- **references.on_demand**（6 条，条件→路径）：
  - 旗舰 Nature 或用户要求 Nature Article 统计投稿审计
    → `references/nature-article-requirements.md`
  - NMI 目标
    → `../nature-shared/journal-formats/nature-machine-intelligence.md`
  - 嵌套/伪重复/重复测量/多比较/相关/回归/离群/小样本/过强 p 语言
    → `references/common-failure-modes.md`
  - 图注/panel n/error bars/stars/box/violin/source-data
    → `references/figure-statistics.md`
  - 终审 QA/严重度标签/审稿风险/回应起草
    → `references/reviewer-checklist.md`
  - 同一统计量多处出现或区间术语问题（两种精度、SD/Std 漂移、
    confidence vs prediction interval、重叠误差条被写成优于）
    → `../nature-shared/core/consistency-sweep.md`

### 对 nature-shared 的依赖

两条：
`../nature-shared/journal-formats/nature-machine-intelligence.md` 与
`../nature-shared/core/consistency-sweep.md`（相对路径解析）。

## 三、核心工作流（9 步）

1. **分类任务**：audit / rewrite / draft / reviewer-response /
   figure-statistics alignment / data-backed reanalysis；
   输入不全时做 bounded audit 并声明不可评估部分。
2. **提取设计**：groups / treatments / time points / endpoints / blocking /
   repeated measures / randomization / blinding / exclusions / missing-data。
3. **定义 n 与复制**：区分独立实验单位 vs biological / technical replicates vs
   repeated measures vs cells/fields/subsamples vs simulations vs pooled。
4. **映射 claims 到 analyses**：每个主张对应 comparison/model、test family、
   assumptions、correction、effect estimate、uncertainty、exact p policy。
5. **查 common-failure-modes**：对照 `common-failure-modes.md` 的
   P0/P1/P2 失效模式。
6. **查 reporting completeness**：旗舰 Nature 加查
   `nature-article-requirements.md`；NMI 加查 nature-machine-intelligence 格式文件。
7. **对齐 figure statistics**：按 `figure-statistics.md` 核对每 panel 图注。
8. **起草/修改**：保守、可直接粘贴的文本；不把关联升级为机制/因果。
9. **最终 QA**：用 `reviewer-checklist.md`。

### 关键参考规则

#### references/statistical-reporting.md

- 每项主分析**最少提取 15 项**：
  endpoint / groups / independent unit / biological+technical replicates /
  repeated measures / inclusion-exclusion / missing-data /
  randomization+blinding / transformation / test or model / assumptions /
  multiplicity / estimate+interval+p+n / software。
- Statistical analysis 段落 **7 段式**：
  software → data summary convention → sample-size/replication definition →
  test/model choice → multiplicity → thresholds/exact reporting →
  exclusions/robustness。
- Results wording Prefer/Avoid：
  避免 proved / highly significant / "n=300 cells" 实为 3 animals /
  单独写 ns / 无依据正态声称。
- `AUTHOR_INPUT_NEEDED` 标签示例；ready-to-paste skeleton（全占位符）。

#### references/nature-article-requirements.md

- 仅旗舰 Nature；初投稿即须统计信息完整。
- Methods 检查 7 项：
  每个检验及对应比较 / 单双尾 / 独立单位 / 复制定义 / paired 结构 /
  排除+随机化+盲法 / 多重比较校正。
- 数值报告检查：
  exact n 不用范围、每个 error bar 定义、重复次数、显著与非显著 exact P、
  ANOVA 报 F+df、t 检验报 t+df、结果附近写明比较/检验族/尾向。
- Reporting Summary gate：
  生命/行为社科/生态进化环境/部分物理领域。
- 审计表列：
  Analysis/panel | Test and tail | Exact n and replicate unit |
  Error/interval | Exact P | Test statistic and df | Repeat count | Status；
  状态 = pass / AUTHOR_INPUT_NEEDED / not applicable / blocked。
- 官方源 2026-08-08 验证。

#### references/common-failure-modes.md

- **P0 四项**（各含 Signal / Risk / Fix）：
  Pseudoreplication、Uncorrected multiple comparisons、
  Wrong interaction inference（"一个显著另一个不显著≠效应不同"）、
  Analysis unit mismatch。
- **P1 五项**：
  Significance-only、Small-sample overclaim、
  Normality/equal-variance 不可支持、Outlier handling 不清、
  Correlation/regression overclaim。
- **P2 五项**：
  error bars 未定义、n 跨 panel 变化未按 panel 给、star 未定义、
  software 缺失、ns 掩盖结果。
- 语气：不指责作者，只陈述风险与修法。

#### references/figure-statistics.md

- 每 panel 图注应给 9 项：
  图形元素含义 / exact n 定义 / n 的层级 / summary convention /
  test-model / paired 状态 / 多重校正 / exact p 或 star 阈值 /
  source data 是否含原始独立值。
- plot-type checks：bar / box / violin / time course / heat map /
  regression 各有 Require。
- star notation：避免只写 `*P<0.05`，给出 prefer 模板。
- source-data notes 5 项。
- Figure audit mini-format：
  Panel / Current legend problem / Risk / Required fix / Suggested legend text。

#### references/reviewer-checklist.md

- 严重度：
  P0（可推翻中心主张，6 例）/ P1（报告太弱，6 例）/ P2（清晰度，4 例）。
- Final QA 8 问。
- reviewer-risk 中性措辞 vs 禁止指控措辞（如 "This is fake significance"）。
- 未供原始数据时用 not assessable / potential risk / requires author confirmation。
- response-to-reviewer 结构：
  Reviewer concern / Author-side action / Draft response / Manuscript change；
  **不得声称做了新分析，除非用户给了数据**。

#### references/source-basis.md

- Source hierarchy 5 级：
  1. 用户材料与期刊指示；
  2. Nature Portfolio reporting standards；
  3. Nature / Nature Methods statistics guidance
     （source-basis.md 总结，Statistics for Biologists collection）；
  4. 研究类型指南（CONSORT / STROBE / PRISMA / ARRIVE 等，仅在适用时）；
  5. 保守报告实践。
- 用语分寸：
  Prefer "The manuscript should define…"；
  Avoid "Nature requires this exact test."。

## 四、输出契约与质量检查

两种输出形态：

1. **审查型**：
   - `Statistics review scope:`：
     Input reviewed / Boundary / Study design readout /
     Independent unit and replication readout；
   - Major statistical issues：
     [P0/P1/P2] Issue / Evidence / Why it matters / Fix；
   - Ready-to-paste revision；
   - AUTHOR_INPUT_NEEDED（短事实问题）；
   - Reviewer-risk note。
2. **干净起草型**：
   Draft Statistical analysis + Reporting notes
   （n definition / tests / models / multiple comparisons / software /
   version / unresolved fields）。

质量检查靠 `reviewer-checklist.md` 的 Final QA 8 问与严重度标签；
所有缺失信息以 `AUTHOR_INPUT_NEEDED` 显式列出。

## 五、关键规则与红线（6 条）

1. 不编造 p 值、样本量、df、CI、软件版本、校正方法、预注册、排除规则、power。
2. 设计或单位不清时，不定最终检验。
3. 不接受 n=细胞/图像/测量数作为独立复制而不查层级。
4. 不用 significant 当 important / large / causal 的同义词。
5. 不把非显著/弱结果改写成更强主张。
6. 不给超出报告检查的医学/监管/临床试验统计建议。

## 六、配套资源

- **references/**：
  - `source-basis.md`：source hierarchy 与用语分寸；
  - `statistical-reporting.md`：提取 15 项、7 段式、skeleton；
  - `nature-article-requirements.md`：旗舰 Nature 专项审计；
  - `common-failure-modes.md`：P0/P1/P2 失效模式；
  - `figure-statistics.md`：图注 9 项与 plot-type checks；
  - `reviewer-checklist.md`：严重度、Final QA、回应起草。
- **README.md**：
  用途清单（检查/改写/区分复制层级/识别伪重复/跨章节一致性/
  审稿回应草稿/旗舰 Nature 与 NMI 专项）；
  边界（不编造、不替代统计师、不能仅凭图像判断实验单位）；
  相关技能（nature-figure / nature-response / nature-polishing）。
- **agents/openai.yaml**：
  display_name "Nature Statistics"，
  default_prompt 用 `$nature-statistics` 审计设计/分析/报告。
