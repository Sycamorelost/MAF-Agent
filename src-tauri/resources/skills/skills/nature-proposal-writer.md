# nature-proposal-writer

## 一、定位与触发场景

- **用途**：proposal-first 的科研写作状态机 pipeline——从证据出发撰写、修订或审查科研 proposal、开题报告（开题/研究计划）与科研项目申请书。**不是通用"帮我写论文" prompt**。设计灵感来自 autonovel（状态机+打分）、professor（动态专家）、brainstorming（入口追问）、anti-AI-writing（语言清理）。
- **description 原文摘录**（SKILL.md frontmatter）：

  > "Compose, revise, or audit research proposals, opening reports, and research plans from supporting evidence. Use for 研究计划、开题报告、科研项目申请书. Invoked as researchwrite for compatibility; use nature-writing for general manuscript sections."

  注意：frontmatter 的 `name` 是 `researchwrite`（兼容旧安装的触发名），仓库目录名才是 `nature-proposal-writer`，无迁移方案时不得改名。
- **典型触发请求**：
  - "我有一个基金题目，帮我先拆科学问题和研究内容，不要直接写正文。"
  - "这段 introduction 太散，按 proposal-first 的方式重构。"
  - "把这份草稿扩成 proposal 框架，并标出证据缺口。"
  - "用 researchwrite 审查这段 discussion，paper 挡位。"（进入 QA 模式）
- **明确不做什么（边界）**：
  - 不替作者虚构实验结果、参考文献、合作基础或可行性证据；缺失事实标 `AUTHOR_INPUT_NEEDED` 而非用流畅文字掩盖。
  - 仅句子级英文润色走 `nature-polishing`；一般手稿章节写作走 `nature-writing`；NSFC 专项走 `nsfc-proposal`。

## 二、运作模式（加载/路由机制）

### SKILL.md 作为路由

frontmatter：`name: researchwrite`、`description`（见上）、`license: MIT`、`metadata.hermes.tags: [research, writing, proposal, revision, qa, multi-agent]`、`related_skills: [brainstorming, professor, avoid-ai-writing, docx]`。

路由逻辑：

1. **模式分派**（按输入三选一，模糊时自行推断，仅在选择改变工作流时才问）：
   - 题目/方向/模糊想法 → `compose`（加载 `references/compose-mode.md`）
   - 已有段落/章节/完整 proposal → `revise`（加载 `references/revise-mode.md`）
   - 已有草稿 + 扩写/补写/重构 → `hybrid`（加载 `references/hybrid-mode.md`，非独立流程，是 revise+compose 的组合）
2. **QA 模式**：用户说"审查这段/跑 QA/检查方案/过 pipeline"时进入四层 QA pipeline。
3. 需要初始化模板、按条件加载 reference 或执行导出脚本时先读 `manifest.yaml`，**只加载与当前条件匹配的路径**，不一次性注入全部 reference。

### manifest.yaml 结构（name: researchwrite, version 1.1.1）

- **always_load**：仅 `SKILL.md` 自身（与其他 skill 不同，此 skill 无 static core/fragments 分层，功能文字全在 references/templates）。
- **templates.on_demand**（条件→模板）：
  - 初始化新项目/重建 foundation 文件集 → `templates/00_scope.md`
  - 建立/审查 research canon → `templates/01_research_canon.md`
  - 起草前映射 claim→evidence → `templates/02_evidence_table.md`
  - 章节起草前建 argument map → `templates/03_argument_map.md`
  - 定义各节 purpose/inputs/allowed/forbidden claims → `templates/04_section_contracts.md`
  - 设定术语、语气、禁用表达、anti-slop 风格 → `templates/05_style_guide.md`
  - 四层 QA 后产报告 → `templates/qa_report.md`
  - gate 未过/专家审查后写定向修订简报 → `templates/revision_brief.md`
- **references.on_demand**：20 个条件-路径对，覆盖三模式、评分、anti-slop、中文综述风格、停止规则、专家分派、foundation 文件、项目结构、导出归档、分阶段范围、引用编号级联、综述框架、综述批判方法、自动校验、跨 agent 交接、已批本子内写作、完整示例、降承诺模式。
- **scripts.on_demand**：导出 DOCX → `scripts/build_proposal_docx.py`。
- 无 axes；**不依赖 nature-shared**。

### 核心原则（SKILL.md 九条，决定全流程行为）

1. 证据先于文字——起草前必须建立或读取 `research_canon` 和 `evidence_table`；
2. 论证先于章节——写正文前必须完成 `argument_map`；
3. 契约先于段落——每节需要 purpose / allowed claims / forbidden claims / inputs / validation；
4. 范围先于完备——分阶段写作先锁阶段边界；
5. 动态专家，不设固定池——用 `professor` 按失败模式召唤专家；
6. 内容先于语言——先诊断科学逻辑再做 anti-slop/语言打磨；
7. 不自动升级事实——永远不把 "may indicate" 改成 "proves"，除非有证据；
8. 删除胜于解释——主张不可行就直接删，正文干净，解释留给答辩；
9. 该停就停——平台期、专家冲突、证据缺失是停止理由，不是润色理由。

## 三、核心工作流（实际功能流程）

### 项目结构（SKILL.md + references/project-structure.md）

工作目录 `<outputs>/researchwrite/<project-slug>/`；归档需批准后才写 `<archive-root>/researchwrite/<project-slug>/`，**不主动写外部 vault/同步目录**。

标准文件：`00_scope.md`（任务边界）、`01_research_canon.md`（硬事实与约束）、`02_evidence_table.md`（claim→evidence 映射）、`03_argument_map.md`（论证架构）、`04_section_contracts.md`（节契约）、`05_style_guide.md`（风格/术语/禁用表达）、`state.json`（最少字段：project / mode=compose|revise|hybrid / text_type / language=zh|en|mixed / target_reader / current_round / scores[] / technical_debts[] / status=intake|foundation|drafting|revision|export）；目录 `sources/`（user_materials、literature、data）、`drafts/`（含 sections/）、`revision_briefs/`、`qa_logs/`、`exports/`。

### Compose mode（references/compose-mode.md，9 步）

总览链：`Intake Q&A → scope lock → research canon → evidence table → argument map → section contracts → foundation review → section drafting → section QA → full proposal QA → export`。foundation review 未过阈值（foundation_score > 7.5）不得开写。

- **Adaptive intake**：默认一次一个问题；用户要求加速才 3-5 个打包；材料或会话中已有答案的不问。优先级：P0（文本类型/目标读者/主题方向/输出语言/交付范围）、P1（核心科学问题/已有证据/必含内容/禁用声称/目标风格）、P2（字数/格式/模板/时间线）。只问会改变写作路径的问题；用户说"先不管这些，直接开始写"就用合理默认锁 scope 进 Step 2。**本子-based intake 特例**：用户基于已有本子工作时，先收齐再动手、让用户控制顺序、逐件简短确认、用户示意信息完整后才问缺什么，然后才走 scope→canon→evidence→argument map。
- **Step 1 → `00_scope.md`**：proposal 深度匹配项目时间线（1 年后才启动的项目写方向级而非执行级，操作细节属于实验前规划）；用户说"只是大致 proposal 不是执行书"就锁方向级，避免过度指定设备/SOP/安全规程。最少填写：project slug、mode、text type、target reader、language、deliverable scope。
- **Step 2 → `01_research_canon.md`**：最少内容——文献事实 ≥3 条（各带来源 author-year 或 DOI）、实验事实 ≥2 条、热力学/模型事实（如有，标"model output, needs experimental verification"）、导师约束（原话或转述）、术语定义、**forbidden claims**（文本不得作出的声称）、unresolved claims（有争议未证实的，可提及不得解决）。红线：不得从"常识"推导 canon，每条须溯源；文献结论冲突时标 unresolved 并列双方；模型数据不得当实验事实；canon 是边界文件，写作不得暗改，起草中出现新证据先更新 canon。
- **Step 3 → `02_evidence_table.md`**：行格式 `claim | evidence/source | strength | usable section | risk | status`；strength 取值 `evidence-backed` / `plausible-inference` / `hypothesis` / `unsupported`；最少 ≥5 行覆盖核心科学声称。红线：不得把 hypothesis 充成 evidence-backed；无证据声称标 `unsupported` 不靠省略隐藏；"文献中显然"的证据也要引用。
- **Step 4 → `03_argument_map.md`**：科学张力（已知/未知/gap 为何重要）、中心研究问题（一句话，须可被所提工作回答）、中心论点（一句话）、2-4 个支撑论证（各带 claim/evidence/limitation）、反方论证（替代解释及应对）、final move（读者读完应得出什么）。红线：论证必须能由 evidence table 支撑，发现缺口回 Step 3；弱问题（"研究…的行为"）产弱 map，可证伪问题产强 map；写明"某未控变量可推翻首选路线"的 proposal 比无视它的更强。
- **Step 5 → `04_section_contracts.md`**：每节一个契约：Purpose / Inputs / Allowed claims / Forbidden claims / Required evidence / Validation checklist（2-4 条检查）。默认章节（可按导师/学校要求调整）：研究背景与意义、国内外研究现状、科学问题与研究目标、研究内容、技术路线与实验方法、创新点、可行性分析、研究计划与预期成果。红线：背景节契约不得放行属于研究现状的声称；创新点契约必须禁用"本研究具有重要的理论和工程意义"，只允许具体可检验的贡献声明；分阶段写作时加载 `partial-proposal-scope.md` 并给每个契约加 scope-boundary 约束。
- **Step 6 Foundation review**：professor 派 1-2 位专家（博士 proposal 评审 + 领域专家），按 evaluation rubric 给 foundation 打分；`foundation_score < 7.5` 回最弱 foundation 文件重修；**门禁：>7.5 才能开写**。审查记录写 `qa_logs/foundation_review.md`。
- **Step 7 逐节起草**：读契约 → 提取 allowed claims 与 required evidence → 边写边引证据 → 节 QA（rubric 打分，目标 `section_score > 6.5`，低于则修好再进下一节，不积累坏节）。红线：初稿求推进不求完美，6.5 可保留，别磨到 9.0 才写下一节；防串节（"研究内容"开始讲背景就停并回查契约）；草稿中出现 evidence table 没有的证据须补录进表——草稿文本不得成为不受治理的声称来源。
- **Step 8 Full proposal QA**：对完整 proposal 跑 8 维 rubric，记录 `qa_logs/proposal_qa.md`；门禁：`proposal_score > 7.0` 可给导师看，`> 8.0` 进入最终打磨；低于 7.0 定位最弱 1-2 维，回对应 foundation/节修复后重打。
- **Step 9 Export**：产出 `exports/proposal_final.md`（归档）与 `exports/proposal_final.docx`（提交）。交付前检查：无未决 TODO、无伪装成事实的 unsupported claim、引用占位符已解析、图表编号一致、版本+日期清楚。红线：不得跳过 docx（md 给版本控制，docx 给导师看）；分阶段产物文件名须体现（如 `part1_xxx.md`）。

### Revise mode（references/revise-mode.md，9 步）

总览链：`classify text → extract claims → compare with canon/evidence → diagnose structure → professor review → anti-slop scan → revision brief → revise → rescore`。核心规则：**先诊断后改写，内容先于语言，绝不给 unsupported claim 做美容**。无 foundation 文件时首轮从文本反向建最小 canon/evidence_table。

- **Step 1 分类文本**：类别表 background / literature_review / research_question / objectives / methodology / innovation / feasibility / schedule / expected_outcomes / full_proposal。不得仅凭节标题分类（名为"研究背景"实谈文献的算 literature_review）；分阶段的 full_proposal 加载 `partial-proposal-scope.md`，缺后期内容不算缺陷。
- **Step 2 提取 claims**：格式 `Claim / Location / Type（evidence-backed|plausible-inference|hypothesis|unsupported）/ Evidence cited`；单节 ≥5 条、完整 proposal ≥15 条；机械提取不评判；隐性声称（埋在方法段的"The melting point is around 200°C"）也算；数值声称最重要（最易被夸大）。
- **Step 3 与 canon/evidence 比对**：输出比对表（claim / canon match / evidence table match / issue）。有引用来源的声称不因不在表中而判 unsupported——须核对来源是否真说了这话；越界声称是"scope violation"而非"factual error"。
- **Step 4 结构诊断**：检查 section order（背景→gap→问题→目标→方法→结果是否合逻辑）、section integrity（是否串节）、argument presence（有无研究问题且被方法回答）、claim distribution（证据型集中文献综述、假设型在方法、unsupported 在预期结果是正常分布）、missing sections。**只诊断不修复**——修复处方归 revision brief。
- **Step 5 Professor review**：结构诊断发现非平凡内容风险时派 1-2 位专家，记录 `qa_logs/professor_round_XX.md`。**可跳过**：纯语言问题、无内容问题、用户明说"只看语言"。
- **Step 6 Anti-slop scan**：按 `research-anti-slop.md` 扫描并标注（类型/位置/建议修复）。红线：anti-slop 修复不得改变科学含义（原文有统计显著的"显著降低"不能降级为"可能降低"）；有真实逻辑功能的程式化连接词（方法节描述工序的"首先…其次…"）不删。
- **Step 7 生成 revision brief** → `revision_briefs/brief_round_XX.md`：Overall assessment（2-3 句+当前分数估计+文本处于 draft/internal/supervisor/polish 哪级）、Priority problems（P0 阻断提交 / P1 强烈建议 / P2 优化，各带具体位置与修法）、**What NOT to change**（写得好的部分，防过度修订）、Suggested revision order。红线：不写"全部重写"的 brief——底子烂就明说并建议转 compose mode；每条 P0 必须引用具体段落或声称（"改进实验设计"不可执行，"§3.3.4 纯化后取样环节未说明气氛控制"才可执行）。
- **Step 8 修订**：按优先级修；每个大修后抽查未引入新问题。规则：不改 brief 未标记的节；unsupported claim 标出/降级/索要证据，**绝不默默升级**；需新证据则更新 `02_evidence_table.md`；改动范围边界则更新 `00_scope.md`。
- **Step 9 重打分**：跑完整 rubric 对比修订前后，写 `qa_logs/score_round_XX.md`；门禁：`proposal_score < 7.0` 且停止规则允许时回 Step 7；停止规则触发则报告状态并升级。

输出文件：`qa_logs/diagnosis_round_XX.md`、`qa_logs/professor_round_XX.md`、`qa_logs/anti_slop_round_XX.md`、`revision_briefs/brief_round_XX.md`、`drafts/revised_vX.md`、`qa_logs/score_round_XX.md`。

### QA Mode——四层质量保障 Pipeline（SKILL.md）

情境挡位：`paper`=7.0 / `proposal`=7.0 / `internal`=5.0 / `quick`=无阈值（只标 P0）。

顺序（**先内容后语言**，Gate 2 故意在 Gate 1 前，避免改完句子被专家打回重写）：

1. **Gate 2 professor Convener（内容层）**：论文 → 方法论专家+领域专家；proposal → 可行性专家+创新性专家；文献综述 → 覆盖面专家+批判深度专家。
2. **Gate 1 avoid-ai-writing detect-only**：仅对英文有效，中文跳过或降级为手工扫读。
3. **Gate 3 auto-validation（格式/完整性层）**：每个 claim 有 citation？方法可复现？编号连续？（细则见 `references/validation-checklist.md`）
4. **Gate 4 评分阈值**：总分 ≥ 挡位阈值通过；< 阈值按低分维度定向回退，**不超过 3 轮**。

### Evaluation rubric（references/evaluation-rubric.md）

8 维 0-10 分，锚点面向材料/化学/工程领域中文博士 proposal：1) 研究问题清晰度、2) 科学张力（known→unknown→gap 重要性）、3) 证据匹配（区分文献发现/模型估计/假设；红旗：仿真结果报 3 位有效数字、图数据未标需实验验证）、4) 逻辑链（每个 objective 有 method、每个 method 有 expected outcome）、5) 方法可行性（他人可否复现）、6) 创新性（具体贡献非口号）、7) 风险边界（条件化结果逻辑"if X then Y"、决策树与兜底）、8) 语言质量（anti-pattern 密度）。打分程序：先读全文 → 扫 anti-slop → 各维独立打分 → 低于 7 的维度各写一句基于原文的问题说明（不得照抄锚点）→ 报告分数+建议（重构/内部草稿/可给导师/最终打磨）。

阈值总表：`<6.0` 不可交付必须重构；`6.0-7.0` 内部草稿；`7.0-8.0` 可给导师看；`>8.0` 正式版本打磨。阶段阈值：foundation_score > 7.5 开写、section_score > 6.5 保留、proposal_score > 7.0 给导师、> 8.0 最终打磨。

### Professor dispatch（references/professor-dispatch.md）

`professor` 是动态专家生成器，非固定池；按当前失败模式选专家。Dispatch contract 提供 task_context / current_mode / text_type / research_domain / current_failure_mode / needed_review_depth / canon-evidence-draft summary；要求返回 selected_experts / expert_reviews / conflict_summary / recommendation / permanence_candidate。每位专家意见必须区分：证据支撑的发现、合理但未证实的解读、需作者补充的缺失信息、会改变范围或科学承诺的建议。专家数量：默认 1-2，复杂 proposal 2-4，终审或重大方向决策至多 5，超 5 须明确理由。窄问题不派宽泛领域专家。同一专家类别被调用 ≥3 次或用户明确要求时，可提议创建长期复用评审（须批准）。专家冲突须显式调和，指出分歧来自证据解读/可行性假设/范围；**作者或有资质的领域负责人保留最终科学判断**。

### 停止规则（references/stopping-rules.md）

满足任一即停：修订达 3 轮上限；连续两轮分数提升 < 0.5；关键声称所需证据缺失；专家冲突无法诚实调和；用户目标已达。停止时报告：当前分数/状态、剩余问题、停止原因、一条建议的下一步。**不得用更流畅的 prose 掩盖未解决的证据缺口**。

## 四、输出契约与质量检查

- **每次运行结束必输出 4 项**：当前文件路径或修订后文本；当前分数/状态；剩余风险；一条建议的下一步。
- **正式导出双文件**：`exports/proposal_final.md`（归档/版本/复用）+ `exports/proposal_final.docx`（提交/导师传阅）。
- **质量门禁汇总**：foundation > 7.5 → 开写；section > 6.5 → 保留；proposal > 7.0 → 可给导师；> 8.0 → 最终打磨；QA 挡位阈值 paper/proposal=7.0、internal=5.0；回退 ≤3 轮。
- **验证脚本**：`scripts/build_proposal_docx.py`（用法 `python3 scripts/build_proposal_docx.py <input.md> [output.docx]`）——把 proposal markdown 转成符合学术格式的 docx：标题居中 Times New Roman+宋体 18pt 加粗；标题 16/14/12pt 加粗黑色；正文 12pt、1.5 倍行距、**无首行缩进**；页边距 1 英寸；表格 10pt Table Grid。
- **自动校验清单**（references/validation-checklist.md，Gate 3）：引用完整性（每个事实 claim 有 citation、列表与正文互相闭合、关键数值有出处）等必检项，不通过则标注缺失。

## 五、关键规则与红线

1. 证据先于文字：未建 canon + evidence_table 不起草；foundation 未过 7.5 不开写。
2. 不虚构、不升级：unsupported 标出不隐藏；"may indicate" 不得改成 "proves"；anti-slop 修复不得改变科学含义。
3. canon 是边界文件：写作不得暗改，新证据先更新 canon。
4. 删除胜于解释；该停就停（停止规则五条）；不用流畅 prose 掩盖证据缺口。
5. P0 问题必须可执行定位；revision brief 必须含"不要改的部分"。
6. 分阶段写作先锁边界，不把 partial draft 强套完整结构；越界声称是 scope violation 而非 factual error。
7. 专家意见分类标注；冲突显式调和；作者保留最终科学判断；长期复用专家须经批准。
8. 不主动写外部归档/vault/同步目录，除非用户明确要求。

## 六、配套资源

**references/（20 个，按 SKILL.md 索引的条件加载）**：

- `compose-mode.md` / `revise-mode.md` / `hybrid-mode.md`：三模式 9 步流程（hybrid 是 revise+compose 组合，用于"保留草稿同时扩写补写"）。
- `evaluation-rubric.md`：8 维 × 4 锚点评分体系与阈值。
- `research-anti-slop.md`：中文 proposal 清理——空洞意义（具有重要意义/填补空白/提供理论基础/广阔应用前景）、无支撑加强词（显著/全面/系统/深入/揭示机理/有效提升）、机械结构（首先其次再次最后/在此基础上/进一步地/本文拟）、六类结构反模式；结构问题回 argument_map/section_contracts 修而非句子打磨。
- `chinese-review-writing-style.md`：中文综述风格约束——标题是结构路标（研究对象+分析角度），禁拟人/比喻/口号；比 proposal 要求更高事实密度与证据分级。
- `stopping-rules.md`：五条停止条件与停止时报告格式。
- `professor-dispatch.md`：动态专家分派契约、数量上限、选择与永驻规则、4 个示例场景。
- `foundation-files.md`：五个 foundation 文件的字段定义与"canon 不可暗改"规则。
- `project-structure.md`：目录布局与 state.json 最少字段。
- `export-archive.md`：正式导出双文件（md+docx）与归档规则。
- `partial-proposal-scope.md`：分阶段写作的范围锁定问题与防范围蔓延。
- `ref-renumbering-cascade.md`：增删参考文献时的编号级联正确顺序——先改正文引用、按 LOW→HIGH 遍历 remap、跳过参考文献段防双重处理。
- `review-paper-framework.md`：综述框架六阶段 gap-driven 设计（文献全景→gap 识别→定向补缺→覆盖评估→因果链叙事→节契约）。
- `review-critique-methodology.md`：评估（而非撰写）已有综述的 6 维框架与评分标尺。
- `validation-checklist.md`：Gate 3 自动校验清单（引用完整性等）。
- `gpt-handoff-revision-brief.md`：把修订点整理成交给外部 LLM（GPT/Codex）的三件套指令文件（任务书+原稿+修改指令）。
- `within-approved-proposal.md`：已立项本子框架内深化写作——不能改立意、不标新立异，用户常只能复制本子片段。
- `降承诺提案模式.md`：证据不足时把承诺等级校准到证据强度（三层承诺等级），降承诺不是删内容。
- `worked-example-quaternary-proposal.md`：虚构化的材料科学 revise-mode 完整评估示例，展示 foundation 文件应有深度。

**templates/**：8 个空模板——`00_scope.md`（project/mode/text type/reader/language/scope/version/constraints）、`01_research_canon.md`（literature/experimental/thermo-model facts、supervisor constraints 等分节）、`02_evidence_table.md`（claim|evidence|strength|usable section|risk|status 表头）、`03_argument_map.md`（tension/central question/thesis 等）、`04_section_contracts.md`（六字段契约）、`05_style_guide.md`（语言/读者/语气四选一/术语/禁用表达/导师偏好/引用风格）、`qa_report.md`（round/mode/text type + 分数表）、`revision_brief.md`（round/目标文本/当前分数/主问题/必改项/待降级或删除声称/所需证据/语言清理）。

**scripts/**：`build_proposal_docx.py`——markdown → 学术格式 docx 转换器（格式标准见其 docstring）。

**agents/openai.yaml** 未单独查看细节（SKILL.md frontmatter 已含完整路由信息）；无 evals/ 目录。README 还声明相关技能：`nature-writing`、`nature-polishing`、`nsfc-proposal`。
