# nature-paper-card

## 一、定位与触发场景

**用途**：精读一篇科研论文，生成有来源约束（source-grounded）、可复核的固定 **Sections 01–16 Paper Card**——强调研究问题、方法逻辑、实验—结论证据链、结论边界、批判性分析和可检验研究想法，而不是翻译摘要或写通用总结。

**description 原文摘录**（SKILL.md frontmatter）：

> "Build a structured deep-reading Paper Card for one scientific paper, analysing methods, experiment-to-claim evidence, limitations, and research ideas. Use for 论文精读卡、方法拆解、证据链分析; not full-paper bilingual translation or formal peer review."

**典型触发请求**（README）：

- "使用 `nature-paper-card` 精读这篇 PDF，生成完整 Paper Card。"
- "分析这篇论文的方法模块、关键公式和实验—结论证据链。"
- "这篇论文真正证明了什么、没有证明什么？"
- "基于论文中的限制，提出几个可以验证的后续研究方向。"

**明确不做什么（边界）**：

- 不生成全文双语翻译（`nature-reader`）、正式审稿报告（`nature-reviewer`）、批量文献监控（`nature-literature-pipeline`）、汇报 PPT（`nature-paper2ppt`）。
- 不产出 academic-English 短语集、理解测验/self-quiz、公众号/社交媒体文章。
- **不生成第 17、18 节**（脚本硬检查 `^##\s+(17|18)\b`）。
- 不把 Paper Card 静默切换成上述任何相邻 skill 的产物。
- 来源不足时明确写 `Not assessable`，不补写不可见的实验或页码。

## 二、运作模式（加载/路由机制）

**SKILL.md 作为路由**（标题即 "Nature Paper Card - Router"）：frontmatter 仅 name + description；正文给出 6 步 Routing protocol（加载清单→界定来源→分类论文类型→建证据基座→生成 01–16 节→跑 groundedness QA）+ Script red lines + 相邻技能分工。

**manifest.yaml 结构**（version 1.2.0）：

- `always_load`：
  - `../nature-shared/core/terminology-ledger.md`（共享术语台账）
  - `static/core/principles.md`、`static/core/workflow.md`、`static/core/output-contract.md`
- `axes.paper_type`（唯一轴）：
  - `detect`：按论证结构而非学科标签选一个主镜头（primary lens），仅当第二贡献有独立证据、不审会漏时才加一个次镜头；"Load no more than two values"。
  - 取值 → fragment：`methods`（算法/模型/工具/框架/系统）→ `static/fragments/paper_type/methods.md`；`discovery`（经验发现/生物物理机制）→ `discovery.md`；`resource`（数据集/benchmark/atlas/数据库）→ `resource.md`；`clinical`（患者/人群/诊断/预后/干预）→ `clinical.md`；`materials`（材料设计/合成/表征/工程性能）→ `materials.md`；`review`（综述/perspective/commentary/meta-analysis）→ `review.md`。
  - `default: methods`，`multi: true`。
- `references.on_demand`：
  - 打 provenance 标签、区分来源事实/外部事实/分析/假设 → `references/evidence-and-provenance.md`；
  - 起草或审计 01–16 Markdown 结构 → `references/card-schema.md`；
  - 生成/评估/限定候选研究想法 → `references/research-idea-gates.md`。
- `quality_tools`（声明式绑定脚本）：`prepare_script: scripts/prepare_paper.py`（处理 PDF 或 nature-reader source-map JSON 时用，产出 `source_bundle.json`）；`audit_script: scripts/audit_paper_card.py`（首个完整草稿后及每次修正后用，产出 `audit-report.json`）；`script_resolution` 一律相对已加载 SKILL.md 所在目录解析，绝不相对用户工作目录；`replacement_scripts: forbidden`；`locator_modes: page-grounded / structure-grounded / source-limited`。

**静态层与动态判断的分工**：core 三件（原则/流程/输出契约）对所有论文恒定加载；paper_type fragment 只改变分析检查项，卡片 16 节结构跨学科保持稳定；references 三个文件按需进入。

**对 nature-shared 的依赖**：`../nature-shared/core/terminology-ledger.md`（always_load）——首次接触稿件就建术语台账（canonical form、首用展开、源中变体），全篇锁定一致术语，"一个东西一个名字"，术语一致性优先于词汇多样性。

## 三、核心工作流（实际功能流程）

### 步骤 1：加载 manifest 与 core 层

读 manifest.yaml，再读全部 `always_load` 文件；不得只凭 router 本体生成卡片。

### 步骤 2：界定来源边界（source boundary）

识别可用材料档位：带图表的全文 / 无可靠版式的正文 / 仅摘要或元数据 / 已有 `nature-reader` artifact（带稳定 source ID）。**有 nature-reader artifact 优先复用**，不重复全文双语翻译或图表抽取。材料不全时产出可见的 Partial 卡，不受支撑的节标 `Not assessable from supplied material`。

PDF 或 source-map JSON 输入时**内置脚本为强制**：

1. 将 `SKILL_DIR` 解析为已加载 SKILL.md 所在目录；
2. 验证 `SKILL_DIR/scripts/prepare_paper.py` 存在；
3. 按解析路径运行：`python "SKILL_DIR/scripts/prepare_paper.py" INPUT --output WORKDIR/source_bundle.json`；需要视觉页面复核时加 `--render-dir WORKDIR/rendered-pages`；
4. 起草前检查脚本退出码与 bundle 的 validation 块；source-map 输入还要查 `locator_summary` 与 `unlocated_blocks`——只有 `pages` 下的记录有经核验的正 PDF 页码；缺失/非法页码留在 `unlocated_blocks` 并带显式状态，**必须结构化引用，绝不当 page 1**。

**定位状态机（locator modes，三选一且必须记录）**：

- `page-grounded`：内置脚本成功且核验出可靠 PDF 页索引 → 用 PDF 页 + 结构定位；印刷页码只是可选元数据。
- `structure-grounded`：页抽取不可靠但节/图/表/公式/源块可靠 → **不输出任何页码引用**。
- `source-limited`：仅摘要/元数据/用户摘录可靠 → 不输出页码，不推断未见证据。

准备失败时记录失败，优先复用已有 nature-reader source map 或环境 PDF/OCR 能力，但**不得新写替代脚本**，然后进入最强的受支持降级模式。印刷页码标签永远不影响 page-grounded 是否可用。

### 步骤 3：论文类型分类

按 manifest 的 `paper_type` 轴选一个主镜头，混合论文至多一个次镜头；按论证与证据结构分类而非学科；分析前声明两个选择（例：算法论文带重大数据集 → 主 methods + 次 resource）。

### 步骤 4：先建证据基座（drafting 之前）

内部证据清单至少枚举：书目元数据与获取状态；研究问题与声称贡献；方法组件、假设、数据流；每个主图/主表/关键公式及其论证角色；实验、基线、指标、消融与报告结果；作者自述局限；指向页/节/公式/图/表/nature-reader block ID 的稳定来源指针。再建紧凑的 **claim–evidence matrix**（central claim、source pointer、evidence type、精确结果或免引用转述、被支持的 claim 强度、不被支持的更强解释、confidence、矛盾或缺失支撑）。

外部检索只用于 Section 04、Section 15、书目核验或显式新颖性检查；**绝不把论文自己的 related-work 叙事当作已独立核实的领域史**。记录 context mode 三档之一：`paper-only` / `targeted external check` / `externally verified`。

### 步骤 5：生成固定 Sections 01–16

按序应用：core principles → 选定 paper-type fragment → core workflow → output contract。起草前读 `references/evidence-and-provenance.md`；写最终 Markdown 时读 `references/card-schema.md`；写 Section 16 前读 `references/research-idea-gates.md`。

workflow.md 的 9 个子流程：

1. **Inspect and bound the source**：记录来源、完整度、图表可得性、补充材料、抽取置信度、页码体系；尽量解析 DOI/arXiv 元数据；摘要输入绝不暗示全文覆盖。
2. **Build an evidence inventory**：每个主节、主图、主表、关键公式、数据集/人群/材料、基线/指标/评测设置/不确定性、作者明示局限、补充材料有无；清单没说明中心方法与结果证据在哪之前不起草。
3. **Build a claim-evidence matrix**：page-grounded 用 PDF 页索引 + 结构标识；structure-grounded 省略全部页码；source-limited 只用 `Abstract` / `Metadata` / `User-provided excerpt`；不发明页码行号。
4. **Reconstruct the argument**：固定链条 `problem -> limitation of prior approaches -> core insight -> design choice -> evidence required -> experiment or analysis supplied -> conclusion actually supported`；指出断裂/薄弱/未检验的环节，但不写成审稿报告。
5. **Analyze method and evidence**：每个中心组件六问（做什么/为何需要/输入输出/引入什么假设/哪个实验隔离其贡献/拿掉后什么仍未知）；每个中心实验六问（测什么 claim/在什么条件下与什么比较/报告什么指标与不确定性/观察到什么/什么结论成立/什么更强结论不成立）。
6. **Add external context selectively**：仅在可得且有用时引外部来源并引用；未核实则将发展路线与新颖性立场标 `paper-framed` 或 `unverified`；用户明确要 source-only 时不强制外部核验。
7. **Separate limitations**：Section 12 只放作者明示的局限；Section 13 放 Agent 分析、替代解释、缺失对照、失败案例、提议的验证实验；**不得把 Agent 批评挪进 Section 12**；论文无明确局限声明就照实说，不把顺手提到的数据集描述/评测约束提升为作者承认的局限（可另列 "Related constraints noted by the authors" 小节并声明性质）。
8. **Generate knowledge and ideas**（Sections 14–16）：抽取可迁移概念/方法/公式/实验设计；连接到已核实文献、用户提供知识或明确标注的候选方向；研究想法必须源于具体局限或未解释观察；写明 hypothesis、delta、validation、failure modes；无专门检索不得声称新颖。
9. **Write and audit**：按精确 schema 与用户语言写 `paper-card.md`，顶部加简短 evidence-status 摘要；自查术语/数据集/样本量/基线/指标/数字的内部一致性；跑内置审计脚本，修完全部 error、评估 warning，重跑至无 error。

### 步骤 6：Groundedness QA

- page-grounded：`python "SKILL_DIR/scripts/audit_paper_card.py" --card WORKDIR/paper-card.md --bundle WORKDIR/source_bundle.json --locator-mode page-grounded --report WORKDIR/audit-report.json`。
- 两种降级模式：同一审计器去掉 `--bundle`，`--locator-mode` 填实际的 canonical mode。
- audit error 视为 blocker；warning 用科学判断逐条审视而非机械压制；审计器跑不起来就声明失败并手工执行其文档化检查，**不得写替代审计器**。
- 附加人工核对清单：数值结果与来源一致；证据清单覆盖每个主图主表；每个主要方法/结果/边界/局限有来源指针；PDF 页码与印刷页码区分；作者陈述与 Agent 分析分离；外部领域史 claim 有外部引用或标 unverified；提议的想法是 hypothesis 而非 novelty claim；不存在 17/18 节；未夹带学术英语集、测验、公众号稿。

### Paper Card 16 节精确结构（references/card-schema.md）

01 Basic Information（标题/作者单位/期刊/年份/类型/领域/关键词/DOI 或 arXiv/代码/数据集/阅读日期/在用户方向中的位置）；02 One-Sentence Summary（什么问题+什么方法+什么机制+有界结果，一句，禁宣传形容词）；03 Research Question（具体问题、为何重要、现有方法为何不够、恰当时一条精确 `Can ... ?`）；04 Research Background and Development Path（阶段/代表方法/优劣/论文声称位置，标注外部核实还是 paper-framed）；05 Core Pain Points（表格：Pain point / Manifestation / Cause or author explanation / Evidence from the paper；作者解释不得无证据升级为既定根因）；06 Core Idea（表层方法 / 核心洞见 / 可能的一般教训——一般教训标 `[Analysis]`）；07 Method Overview（输入、输出、模块、训练、工具、反馈环、假设 + 输入到输出的文字流）；08 Core Module Breakdown（表格：Module / Function / Why needed / Input and output / Supporting evidence / Known or expected effect of removal；区分实测消融与预期效果）；09 Essential Formulas and Symbols（只收理解必需公式：公式、符号含义、目的、直觉、来源指针）；10 Experimental Design and Evidence Chain（先记数据集/人群、规模、指标、基线、预算、backbone/仪器、oracle 输入、评测协议；再填表格：Experiment / Claim tested / Comparison and conditions / Result / Supported conclusion / Unsupported stronger conclusion / Source）；11 Correct Interpretation of the Conclusions（任务范围、oracle 输入、端到端状态、算力成本、历史数据依赖、模型依赖、最难情形、人群/领域边界、不确定性；结尾给主结果的有界重述）；12 Limitations Explicitly Acknowledged by the Authors（表格：Limitation / Specific manifestation / Future direction proposed by authors / Source；无则写 "No explicit author-acknowledged limitation was found in the supplied source."）；13 Critical Analysis（表格：`[Analysis]` Observation / Potential issue or alternative explanation / Why it matters / How to test it / Basis；只收具体可证伪的关切）；14 Knowledge Learned（可迁移概念/方法/公式/实验设计；非用户提供时小节名 `Agent-derived knowledge candidates`）；15 Connections to Existing Knowledge（只写有支撑的同/合/冲突/可迁移域）；16 Research Ideas（每个候选含 name、源起的局限或观察、core hypothesis、相对论文的 delta、初始方法、validation、可能失败模式、innovation status 三档 `unverified`/`partially checked`/`prior-art checked`；Agent 产出的小节名用 `Agent-derived research candidates`，禁叫 "My research ideas"）。

### 六个 paper_type fragment 的分析重心

- **methods**：重构输入/输出/模块/训练需求/外部工具/反馈环/推理成本；区分核心洞见与实现捆绑；每个消融映射到一个组件并检查比较是否只改该组件；查基线公平、backbone 对等、数据泄漏、算力预算、oracle 输入、端到端状态；组件的预期角色在有隔离实验前只是作者 claim；Section 16 提改变具体假设/机制/瓶颈并定义预算公平验证的想法；数据集为重大的第二贡献时加载 resource 镜头。
- **discovery**：观察、因果/机制 claim、干预、对照、重复、替代解释；区分关联、必要性、充分性、机制；把每张图映射到机制收窄过程；查生物/物理尺度、模型系统极限、时间顺序、测量效度、独立重复；不从相关推因果、不从表型推机制；Section 16 提判别性实验。
- **resource**：采集、纳入/排除、标注、预处理、划分、许可、访问、预期用途；代表性、缺失、标注质量、污染、泄漏、人群/领域覆盖、更新政策；区分"资源大"与"资源有用/有效/可推广"；审视 benchmark 激励与指标是否测到声称能力；下游提升以评测设计为条件；Section 16 提验证/扩展/压力测试/治理类想法。
- **clinical**：人群、场所、入组、暴露/干预、对照、结局、随访、缺失数据、分析集；区分预设与探索性结局、相对与绝对效应；查随机化、盲法、混杂、多重性、校准、外部验证、脱落、不良事件、临床相关性；统计显著 ≠ 效应量与实际获益；不外推研究人群之外、不把观察性关联转因果；Section 16 提伦理可行的验证并明示目标人群。
- **materials**：成分、合成/制备、工艺条件、表征、工作环境、对照；结构—性质—机制—性能链不跳证据步骤；查批次差异、测量不确定度、耐久、放大、真实条件、性能指标归一化；区分已证机制与事后解释；与匹配条件下的公平 SOTA 基线比较；Section 16 提机制判别或放大实验并给失败判据。
- **review**：范围、检索/选择方法、纳入排除规则、分类体系、综合方法、预期贡献；系统综述/meta 分析查 protocol、异质性、偏倚风险、发表偏倚、敏感性分析；叙述性综述区分证据综合与作者立论/倡导；用分类稳健性、覆盖度、反例、遗漏文献替代方法模块消融问题；不把引用量当证据质量；Section 16 从未解决分歧/缺失比较/覆盖不足出发而非虚构系统模块。

## 四、输出契约与质量检查

**产出文件**（output-contract.md）：

- 默认交付 `paper-card.md`；page-grounded 模式必须有工作产物 `source_bundle.json` 与 `audit-report.json`；降级模式下 bundle 可缺，但审计器能跑时 `audit-report.json` 仍必需。这些是机器可读支撑物，不是额外卡片节。
- 卡片必须含：标题与 evidence-status 头部；输入范围与论文类型分类；按序的 Sections 01–16；实质性论文派生 claim 的来源指针；外部事实/分析/假设/用户判断的 provenance 标签；需要处的 `Not applicable` / `Not assessable`。**不得创建 Sections 17/18**。

**Evidence-status 头部固定字段**：

```
> Source coverage: Full paper / Partial paper / Abstract only / Metadata only
> Extraction confidence: High / Mixed / Low
> Locator mode: page-grounded / structure-grounded / source-limited
> Primary analytical lens: ...
> Secondary analytical lens: None / ...
> Context verification: Paper-only / Targeted external check / Externally verified
> Card completeness: Complete relative to supplied source / Partial
```

（`Complete relative to supplied source` 不暗示领域史/新颖性/知识连接已独立核实。）

**Locator 书写规则**：page-grounded 用 `[Paper: PDF p. N, Figure/Table/Equation/Section]`；structure-grounded 用 `[Paper: Figure N]` 等结构标识、绝不带页码；source-limited 用 `[Paper: Abstract]` 等并把未见节标 not assessable。

**Quality gates（阻断交付）**：主要数字无法追溯来源；主图/表未入清单；部分输入声称全覆盖；作者局限与 Agent 批评混杂；未经 prior-art 核验把研究想法说成 novel；为填节编造无关模板内容。阻断无法解决时降级为 `Partial` 并指明确切缺口。审计器非零退出（audit errors）阻断交付；审计通过不证明科学正确，仍需人工审 claim、证据强度与分析结论。

**验证脚本及用法**：`scripts/prepare_paper.py`（准备标准化 source bundle，PDF/source-map 输入强制使用）、`scripts/audit_paper_card.py`（结构与可追溯性审计：节序号、Paper 指针、禁止节、出界词、研究想法三要素等；`--card/--bundle/--locator-mode/--report`）。`tests/test_prepare_paper.py` 为准备脚本单测。

## 五、关键规则与红线

**脚本红线（SKILL.md "Script red lines"）**：

1. 不得相对用户当前工作目录解析内置脚本。
2. 不得写/执行内联 Python 替代两个内置脚本。
3. 不得创建 `extract_pdf.py`、`parse_paper.py` 之类一次性替代。
4. 正常 Paper Card 生成期间不得 patch skill 代码（仅当用户明确要求开发/调试/改进 skill 本身才可改）。
5. 准备失败不得伪造页码。
6. 降级模式不得去掉全部 grounding——用结构定位或显式来源范围定位。

**Grounding/反幻觉红线**：

7. 五类 provenance 标签各司其职：`[Paper]`（论文直接报告，需指针）、`[External]`（外部来源，需直接引用或 URL）、`[Analysis]`（Agent 从已指明证据推理，需理由+指针）、`[Hypothesis]`（可检验未验证的解释/想法，需提议的检验与可能证伪者）、`[User]`（用户提供的判断）；Agent 分析不得以用户口吻写；用户未提供时用 `Agent-derived candidate`。
8. 未核实的领域史加前缀 `[Paper-framed; external verification not performed]`；不因作者声称就称方法 first/novel/SOTA/unprecedented。
9. 矛盾处理：散文/表/图/补充不一致时记录双方数值与位置、不静默二选一、受影响结论标 uncertain、说明什么能消除分歧。
10. claim 动词与证据匹配：reports/observes/is associated with；supports/is consistent with；suggests/does not distinguish；`demonstrates necessity` 仅当有有效干预或消融；`demonstrates sufficiency` 仅当有充分性证据；`causes` 仅当设计支持因果推断。
11. `null` 页码意味未知位置，**绝不渲染成 page 1**；不发明行号；HTML 无页用节与图/表 ID。
12. 研究想法六门（research-idea-gates）：Traceability（系于具体局限/失败/假设/矛盾/未解释结果并附指针）、Hypothesis（陈述可能为假的命题，拒绝"提升效率""换更大模型"式空话）、Delta（说明相对论文改了什么：表示/机制/数据/目标/反馈/评测/部署约束）、Validation（比较、受控变量、指标、数据集/人群、预期观察、证伪结果、相关时给预算）、Failure（至少两条可信失败理由：归因错误、混杂、不稳定、成本、数据稀缺、假设失效）、Novelty-language（只用 candidate idea / hypothesis / possible extension / prior-art search required；无专门检索禁 novel/first/unexplored/数字创新分）。

## 六、配套资源

- `references/card-schema.md`：Sections 01–16 的逐节精确 schema（标题、表格列、必填要素、空节写法）。
- `references/evidence-and-provenance.md`：五类 provenance 标签、来源指针格式、三种 locator mode 写法、claim 动词强度表、paper-framed 前缀、矛盾处理。
- `references/research-idea-gates.md`：Section 16 想法的六道门（可溯源/可证伪假设/delta/验证/失败/新颖性措辞）。
- `static/core/`：`principles.md`（立场、范围、语言、provenance 分类、适用性）、`workflow.md`（9 步流程）、`output-contract.md`（交付物、evidence-status 头、locator 规则、quality gates）。
- `static/fragments/paper_type/`：methods/discovery/resource/clinical/materials/review 六个分析镜头（见第三节）。
- `scripts/prepare_paper.py`：docstring "Prepare a stable source bundle for the nature-paper-card workflow"——从 PDF 或 nature-reader source map 生成 `source_bundle.json`（元数据、页数、节列表、图表公式清单、抽取置信度、validation 块、可选渲染页）。
- `scripts/audit_paper_card.py`：docstring "Audit a Paper Card against its source bundle and output contract"——检查 01–16 节序、禁止 17/18 节、Paper 指针、出界词（academic english/self-quiz/公众号等）、研究想法的 innovation status/验证/失败要素。
- `agents/openai.yaml`：display_name "Nature Paper Card"，default_prompt 引导生成 01–16 节深读卡。
- `evals/evals.json`：6 条代表性测试，主题覆盖——methods 论文完整卡片+无新颖性声称的研究想法；仅摘要元数据时的诚实 Partial 卡；实验科学论文的关联/必要/充分/机制因果强度纪律；中文卡片 + methods 主镜头与 resource 次镜头 + PDF 页码与印刷页码区分；页码抽取失败时的 structure-grounded 降级且不写替代 Python；仅摘要的 source-limited 卡片不发明方法/实验/页码。
