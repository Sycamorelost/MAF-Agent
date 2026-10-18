# nature-response

## 一、定位与触发场景

nature-response 负责**审稿意见回复与返修通信包**：把编辑决定信、审稿意见、作者笔记、已有 rebuttal 草稿或修改摘要，转成可审计的返修通信产物——内部/编辑总表（master tracker）、按审稿人互相隔离的逐点回复信、返修 cover letter、标红修改稿、或 LaTeX 返修包。目录：`skills/nature-response/`。

SKILL.md frontmatter 中的 description 原文：

> Draft, audit, or revise responses to peer review, revision cover letters, and marked-manuscript or LaTeX revision packages. Use for 审稿意见回复、逐点回复、返修信、rebuttals and edits to existing response drafts. Initial-submission materials belong to nature-writing; simulated peer review belongs to nature-reviewer.

**典型触发请求**：`审稿意见回复`、`逐点回复`、`返修信`、rebuttal 起草或修改；"这是编辑邮件和审稿意见，帮我生成逐点回复框架"；"把我的中文修改说明改成英文 reviewer response"；"检查这份 rebuttal 是否漏回、语气是否太强、有没有缺证据"。

**明确不做什么（边界）**：初投稿材料归 nature-writing；投稿前模拟审稿归 nature-reviewer。不编造实验、分析、行号、图版、统计结果或编辑要求；不在某位审稿人的回复中写交叉审稿人信息；不用"文中已经写过"责备审稿人；appeal（申诉）不作为默认路径，单独路由。

## 二、运作模式（加载/路由机制）

### SKILL.md 路由协议（四步）

1. **读 manifest.yaml 和 always_load 核心层**：`static/core/stance.md`（面向编辑的目标、默认立场、红线、来源层级）+ `static/core/workflow.md`（接受的输入、返修通信工作流、输出包格式）。
2. **无内容轴——运行时内联识别模式与语言**。与 nature-writing、nature-figure 不同，nature-response 没有 fragment 轴，变化在运行时识别：
   - **task mode**：`draft` / `audit` / `revise` / `triage-only` / `cover-letter` / `revision-package` / `latex-template` / `appeal-like`；
   - **decision type**：minor revision、major revision、revise-and-resubmit、transfer after review 或 unclear；
   - **user language**：用户用中文写作时另附 `中文核对` 块。
   
   decision type 是"包级修改策略或完整回复包"的前置门槛：先从编辑信或任务上下文提取；仍不明确时用用户语言问一句（"这是 Major Revision（大修）还是 Minor Revision（小修）？……"），只暂停依赖决定类型的工作。**禁止从审稿意见数量、语气或难度推断决定类型**。局部措辞编辑、已有回复审查、意见级 triage 可以在 decision 未知下继续。appeal-like 案件单独路由，不默认起草申诉信。用 `references/intake-and-routing.md` 在起草前确定 task mode、最小输入和就绪状态。局部编辑或有界审查只返回所请求的段落或发现，不扩大为完整包。
3. **运行 workflow**（详见第三节）。凡回复中提议或执行了手稿正文修改，同时加载 `../nature-shared/core/main-text-discipline.md`：回复信里完整回答审稿人，但手稿修改只保留读者所需的最短文本，优先替换或压缩而非追加，非核心的稳健性/调和细节移去 SI（除非改变中央解释）。**绝不发明实验、引用、行号、图版、补充材料、编辑指示或手稿改动**；任何需作者提供的内容标记为 `AUTHOR_INPUT_NEEDED`。
4. **references/ 与 templates/ 按需加载**。`qa-checklist.md` 与 `package-consistency-audit.md` 互补且都作用于最终包：前者问回复是否完整、诚实、语气得当；后者问标红稿、清稿、回复信三者编辑后是否仍然一致。LaTeX 包在首个完整草稿后、每次手稿编辑后、交付前都要跑 `scripts/check_package_consistency.py`——任何手稿编辑都会使回复信中的逐字引用和页码引用失效，所以审计不是一次性终检而是要重跑。

### manifest.yaml 结构（version 1.7.0）

- 设计说明写在注释里：nature-response 是线性参数化工作流（intake → 抽取编辑/审稿条目 → 分类 → 策略 → 起草 → 映射 → QA → 打包），变化项在运行时识别，因此**没有内容轴**，拆分方式是"核心（常载）+ 既有按需 references"。
- `always_load:`：`static/core/stance.md`、`static/core/workflow.md`。
- `references.on_demand:` 12 条条件→路径映射：
  - 审稿/编辑意见导致手稿正文增删、压缩、搬迁或统计细节改动（分类结果、防返修累积、区分正文/图注/SI 功能、删除检查等）→ `../nature-shared/core/main-text-discipline.md`（**共享层依赖**）
  - 任务范围识别、Major/Minor 门槛、解析粘贴的决定邮件、最小输入、编辑 ID、就绪状态 → `references/intake-and-routing.md`
  - 来源层级、规则出处、政策与建议边界 → `references/source-basis.md`
  - 回复包格式、互盲审稿人隔离、逐点回复信结构、cover letter 结构、标红稿规则、LaTeX 交付路由 → `references/response-structure.md`
  - 用户请求 LaTeX cover letter / 逐点回复 / 标红稿模板或填充 .tex 包 → `references/latex-templates.md`
  - 按类别和严重程度分类审稿意见 → `references/comment-taxonomy.md`
  - 动作标签、逐项工作状态、tracker 字段、阻塞状态、缺失输入状态 → `references/action-mapping.md`
  - 推荐用语、禁用措辞、分歧语气、审稿人漏看已有内容 → `references/tone-and-stance.md`
  - 用户用中文写作或提供中文作者笔记 → `references/chinese-author-alignment.md`
  - 不可能实验、事实错误、审稿人冲突/重复、审稿人可见性隔离、引用、统计、合规、转投、申诉类 → `references/difficult-cases.md`
  - 定稿输出或审查已有回复草稿前 → `references/qa-checklist.md`
  - 手稿与回复信同步编辑、多轮编辑后、包即将编译交付（逐字引用同步、标红基线、删除涟漪、易变页码、手稿数值自洽、术语清扫、清稿/标红稿交付门）→ `references/package-consistency-audit.md`
- `quality_tools:` 登记 `package_consistency_script: scripts/check_package_consistency.py`，使用时机（首个完整 LaTeX 包后、每次手稿编辑后、交付前）与用途，并要求**脚本路径相对 SKILL.md 目录解析，绝不用用户工作目录**。

## 三、核心工作流（实际功能流程）

### stance.md：默认立场与红线（常载层）

**目标**：master tracker 是面向编辑/作者内部的核查文档；每份 reviewer-facing 回复是独立文档，只含该审稿人的意见与对应回复；cover letter 是面向编辑的返修摘要。总体目标是证明每条审稿关切都被理解、处理、并映射到具体手稿改动、有据的科学回应或未解决的作者行动，且不向互盲的另一审稿人泄露材料。

**默认立场**要点：回复前忠实保留每条审稿意见原文；每条关切必须有回复或显式标记未解决；交叉引用只允许在同一审稿人自己的回复内，绝不跨审稿人；默认互盲——reviewer-facing 文件不得透露另一审稿人的意见、编号、推荐意见、身份、回复文字或冲突要求；master tracker 明确标注 not reviewer-facing；多审稿人提同一问题时各自给完整独立回答；每条回复映射到手稿证据、修改位置、有据分歧或 `AUTHOR_INPUT_NEEDED`；**response action、per-item work status、package readiness 三者分开**——该做什么不等于已做完；回复信完整但手稿修改最小，每处正文新增都要审全段、优先替换/压缩/SI 搬迁而非追加；rebuttal 按"可能公开的评审档案"标准写；cover letter 保持简短；编辑手稿时在备份副本上操作并标红改动；回复信中粘贴的修改后手稿文字必须**斜体**；回复信、cover letter、内部摘要、作者笔记中避免把破折号和冒号当常规散文连接符（审稿意见原文引用、标题、公式、标识符、URL 等除外）。

**红线**（原文逐条意译）：不忽略任何审稿意见；不改写审稿意见至改变含义；未获用户提供不得声称已做修改；仅有草稿或作者口头"done"不得标记已验证；不发明行号/图版/引用/统计结果/补充材料；不用敌意或指责语言；不以时间、经费、便利为拒绝实验的首要理由；不隐藏局限；不把主文写成预防性 rebuttal 或让稳健性/调和文字淹没中央证据链；不默认生成申诉信；有占位符时不得声称 submission-ready；不把缺失的手稿改动藏进注释；reviewer-facing 文件中禁止 "as Reviewer 2 also noted"、"see our response to Reviewer 1"、"the other reviewer requested" 等措辞，禁止出现另一审稿人的意见 ID 或推荐意见；禁止 "we already stated this in the manuscript"、"this was already explained"、"as clearly described in the original manuscript" 式责难——原文已存在不免除表达不清的责任。

**来源层级**：① 目标期刊说明与编辑决定信；② Nature / Nature Portfolio / Springer Nature 返修与评审流程指南；③ Springer Nature 关于 rebuttal 的编辑建议；④ 作者提供的本地手稿事实。政策细节可能变化时，给最终提交建议前先核对期刊当前页面。

### workflow.md：20 步工作流

**接受的输入**：粘贴的编辑决定/返修邀请邮件、决定信、审稿意见、已有回复草稿、手稿修改笔记、tracked-change 摘要、行号/页码、图表和补充材料清单、中英作者笔记、期刊名与文章类型、手稿标题/作者列表/稿件号、原稿文本或 LaTeX 源、要求的 cover-letter 或 LaTeX 输出格式、期刊/门户可见性规则。审稿人边界或意见切分含混时标记含混，**不发明审稿人结构**。

**决定类型门槛与策略**：包裹级工作先定 decision；`major review`/`minor review` 等口头说法在含义明确时归一化为 `Major Revision`/`Minor Revision`；不明确就问并只暂停依赖决定的工作。策略表：

| Decision type | 默认策略 |
|---|---|
| `Major Revision` | 证据优先的工作计划：优先中央论点支撑、实验/分析、方法与统计、验证、图、局限、结构重写；未解决的中央证据或诚信/合规项是终稿阻塞项 |
| `Minor Revision` | 有界纠正计划：精确措辞、定义、引用、报告细节、图表呈现、局部澄清；避免不必要的重设计或扩 claim |

决定标签定包级姿态，**不降级单条意见的严重度**——Minor Revision 里的实质证据/统计/伦理/数据诚信问题仍然是 major 或 blocking。期刊说明与编辑明确指示覆盖默认值。

**20 步流程**（workflow.md 原文步骤）：① 识别 task mode 与输入就绪度；② 输入是粘贴的期刊邮件时自动抽取手稿标题、稿件号、期刊、决定类型、编辑指示、审稿报告边界、所需返修文件、截止日、审稿人可见性规则、门户约束；③ 包级工作过决定类型门槛，未知则问且不写把 Major/Minor 等同对待的通用回复；④ 决定已知时选包级策略且保留条目级严重度；⑤ 先抽编辑指示赋 `E.1` 等 ID，再拆审稿意见赋 `R1.1`、`R1.2`、`R2.1` 等 ID；⑥ 每条按类别、严重度、动作标签、工作状态、所需输入、预期产出、是否阻塞终稿、包就绪度、风险分类；⑦ 起草散文前先建内部/编辑 master strategy 与 tracker（可记录跨审稿人重复与冲突，标注 not reviewer-facing）；⑧ 默认互盲，每位审稿人一份独立隐私过滤回复，对外的文件用 `Comment 1` 等中性本地编号；⑨ 重复关切在每份相关审稿人文件里完整作答；冲突诉求在 master strategy 里协调后，只用该审稿人自身关切和手稿证据向各方独立解释；⑩ 审稿人问到原稿已有内容时用 `CLARIFY_EXISTING`，视为呈现问题——直接回答、承认原文措辞/位置不够清晰、做或提议小澄清、引用修改后位置，不说审稿人本该看到；⑪ 期刊/门户明确要求单一合并回复文件时遵循该要求并标注可见性影响，否则默认"编辑/内部 master + 分审稿人文件"，绝不静默把 master tracker 当 reviewer-facing；⑫ `cover-letter`/`revision-package` 时起草简短的编辑向 cover letter，概括返修范围并指向逐点回复而不复制；⑬ 每条声称的改动映射到手稿位置/图/表/补充/引用或显式占位符；每处正文修改加载 `../nature-shared/core/main-text-discipline.md`，归类结果并决定归属（正文/图注/Methods/source data/SI/回复信），每处新增触发受影响段落的删除检查；⑭ 编辑手稿文本时在备份副本上标红，LaTeX 用 `templates/revised-manuscript-redline.tex` 的 `\revised{...}`；⑮ 回复后粘贴的修改手稿文字用斜体，LaTeX 回复文件用 `templates/response-to-reviewers.tex` 的 `\RevisedExcerpt{...}`；⑯ 用户要 LaTeX 时每位审稿人一份 `response-to-reviewers.tex` 填充副本，cover letter/标红稿按需用对应模板，缺失事实保留可见占位符；⑰ 声称的改动只有匹配到可检查产物（修改稿文字、分析输出、图表内容等）才标 `VERIFIED_DONE`，无支撑的作者口头报告标 `REPORTED_DONE_UNVERIFIED`；⑱ 缺作者输入就标记，不编造；⑲ 跑 QA（完整性、决定策略一致性、审稿人隔离、状态校准、阻塞一致性、可追溯、事实性、语气、未解决风险、标红、斜体摘录、LaTeX 占位符可见性）；⑳ 从条目状态推导包就绪度：`ready_to_submit` / `draft_with_placeholders` / `needs_author_input` / `blocked`。

**输出格式**（固定骨架）：Response strategy summary（Decision type / Task mode / Overall posture / Major risks / Parsed email metadata / Suggested ordering / Package readiness）→ Internal/editor master tracker 表（列：ID、Reviewer concern、Type、Severity、Proposed action、Work status、Required input、Expected output、Blocks finalization?）→ 各审稿人独立回复文件 → Draft revision cover letter（被请求或完整包时）→ Marked manuscript changes → LaTeX files → Manuscript change checklist（含 Main-text discipline audit 行：结果/解释类别、去向、追加/替换/压缩/搬迁/删除、改动前后段落词数）→ Missing information / risk flags → 中文核对（用户写中文时）。

### 关键 references 的规则细节

- **intake-and-routing.md**：8 种 task mode 各自的最小输入与默认输出表（如 `triage-only` 只产 tracker/动作映射/缺失输入清单，不写终稿；`appeal-like` 路由出默认流程）；4 种就绪状态（任一意见停留在非 ready 状态，整包不得叫 `ready_to_submit`）；编辑指示先于审稿人处理且优先级更高；粘贴邮件自动抽取字段清单，不让用户手工拆分；审稿人边界不安全时不出对外文件；澄清问题规则（只有五种情形才提问，其余带占位符继续）；路由捷径（"we fixed it"→`needs_author_input`，敌意草稿→`audit`/`revise`，拒稿挑战→`appeal-like` 等）。
- **comment-taxonomy.md**：严重度四级（minor/major/blocking/unclear）；八大类别及默认策略——editorial/presentation（`ACCEPT_TEXT`/`ACCEPT_FIGURE`）、evidence/interpretation（`ACCEPT_EXPERIMENT`/`ACCEPT_ANALYSIS`/`SOFTEN_CLAIM`/`CLARIFY_EXISTING`/`PARTIAL`/`DISAGREE`，证据缺失时软化 claim 加局限）、methodological、statistical（高危直到细节补齐，索取检验名/重复单位/样本量/校正方法/效应量/置信区间，**不发明 p 值**）、data/code/materials（可对接受拒稿协调 nature-data）、citation/positioning（只加真实相关且已验证的引用）、scope/feasibility（承认科学价值+研究设计理由+局限，避开时间经费理由）、ethics/compliance（通常 `BLOCKING`，不写绕开缺失合规的回复）。
- **action-mapping.md**：12 个动作标签（`ACCEPT_TEXT`、`ACCEPT_ANALYSIS`、`ACCEPT_EXPERIMENT`、`ACCEPT_FIGURE`、`CLARIFY_EXISTING`、`ADD_CITATION`、`SOFTEN_CLAIM`、`PARTIAL`、`DISAGREE`、`OUT_OF_SCOPE`、`AUTHOR_INPUT_NEEDED`、`BLOCKING`）及各自使用条件；tracker YAML 字段（comment_id/severity/category/action/work_status/required_input/expected_output/verification_evidence/blocks_finalization/package_readiness/risk_level/manuscript_location）；8 种 work status（`VERIFIED_DONE` 需可检查产物、`REPORTED_DONE_UNVERIFIED` 仅作者口头、`TODO_TEXT`/`TODO_ANALYSIS`/`TODO_EXPERIMENT`/`TODO_AUTHOR_CONFIRM`/`NOT_FEASIBLE`/`PROPOSED_DISAGREEMENT`）及状态规则（草稿回复不是手稿已改的证据；`NOT_FEASIBLE` 不自动 submission-ready）；包就绪度保守推导规则；风险四级；映射规则（"we revised it"→`AUTHOR_INPUT_NEEDED` 直到位置与性质明确等）。
- **tone-and-stance.md**：核心姿态（合作但不屈从、证据优先、编辑可快速审计）；推荐句型与禁用句型清单（禁用 "The reviewer misunderstood..."、"Due to lack of funding..."、"We already stated this..." 等）；分歧五段式（承认关切→窄化分歧点→给证据/范围逻辑→小澄清→不人身化）及模板；审稿人误解模式（视为呈现信号，未获作者批准的小澄清保留为 `AUTHOR_INPUT_NEEDED`）；out-of-scope 模板；claim 强度动词分级（demonstrate/show/establish > indicate/suggest/support > are consistent with/may reflect/raise the possibility），证据是相关性时先软化因果动词。
- **difficult-cases.md**：九类难案——不可能/超范围实验（五步法+模板）；审稿人事实性错误（六步法，不指责、把漏看当呈现不清的证据）；冲突审稿人要求（内部浮现冲突→编辑指示优先→找满足双方的最小修改→不向任何一方透露另一方）；重复关切（只在内部 master 关联，对外各自完整作答）；审稿人要求引用（只加真实相关且已验证者，元数据缺失用 `AUTHOR_INPUT_NEEDED`）；重大统计批评（高危/阻塞，索取九项细节）；伦理/合规/数据诚信批评（通常 `BLOCKING`，索取批准号等）；transfer after review（保留转投包审稿人 ID）；appeal-like 案件（识别四种路由信号，默认动作是说明"这属于申诉类而非返修回复，申诉信应按期刊专门规则单独处理"）。
- **chinese-author-alignment.md**：接受中文审稿摘要/作者笔记/中英混合输入；最终逐点回复默认英文（用户明确要求才中文）；保留短小的`中文核对`块用于未解决作者行动；九条中文笔记转换表（"我们已经改了"→追问改了什么/在哪/有无修改文本；"按审稿人意见修改"→`AUTHOR_INPUT_NEEDED`；"由于时间原因没做"→只在属实时换成研究设计/范围理由，否则标风险；"审稿人误解了"→重构为手稿清晰度问题等）；双语起草四步；语气纠偏示例。
- **source-basis.md**：来源层级表（Nature editorial criteria、transparent peer review、Nature Electronics 编辑流程、Springer Nature rebuttal 指南、Scientific Reviews 政策，各附 URL 与本技能落地的规则摘要）；实现含义（逐点回复是默认结构；cover letter 不复制逐点回复全文；不夸大来源权威——Springer Nature 建议是写作指导而非约束性政策）。

## 四、输出契约与质量检查

**输出包**（response-structure.md 固定九段顺序）：① 明确标注 not reviewer-facing 的策略摘要；② 同标注的 master tracker；③ 每位审稿人一份独立隐私过滤逐点回复；④ cover letter（被请求或完整包时）；⑤ 标红手稿改动；⑥ LaTeX 交付物（被请求时）；⑦ 手稿修改清单；⑧ 缺失信息/风险标记；⑨ 中文核对（用户写中文时）。

逐点回复信结构：`Dear Reviewer` 致谢开场 → 每条意见 `## Comment N` 块，内含 **Reviewer comment**（完整保留原文）→ **Response**（直接回答 + 具体动作 + 位置）→ **Revised manuscript text**（斜体粘贴修改稿文字）。Cover letter 短于逐点回复：指认手稿、致谢、概括主要返修动作、指向逐点回复；不用 cover letter 绕开未解决关切。

**QA 闸门**（qa-checklist.md 十组）：Completeness（每条意见有稳定 ID、有回复或显式未解决标记、重复关切逐份完整作答、编辑指示先于审稿人处理、每条目五要素齐全）；Status calibration（`VERIFIED_DONE` 必须有可检查产物支撑、口头"done"只能是 `REPORTED_DONE_UNVERIFIED`、`Blocks finalization?` 与严重度/缺证据/就绪度一致）；Traceability（不发明行号，行号不可得时用节名）；Revision formatting（备份副本标红、LaTeX 用 `\revised{...}`、回复中摘录斜体或用 `\RevisedExcerpt{...}`、每互盲审稿人一份文件）；Reviewer isolation（禁止跨审稿人措辞、推荐意见/身份/私密评语不得跨文件、各文件对手稿改动描述一致）；Factuality（不发明数据/p 值/置信区间/效应量/样本量/DOI/登录号/审稿人身份/编辑指示）；Tone（无指责审稿人无能/偏见/误解、无过度道歉、无空洞致谢、局限干净承认、不以时间经费能力为首要理由、漏看当清晰度问题处理、无 "we already stated this" 类责难）；Actionability（缺失输入具体、高危/阻塞项在终稿前可见、修改清单指明节/图/表/补充/claim、部分回复说明已解决与未解决边界）；Final output gate（编辑能否逐条核验、放进透明评审档案是否仍专业、占位符是否足够可见以防作者误交编造合规、就绪度标签是否诚实、任一非 ready 项存在时不得标 `ready_to_submit`、每个 `VERIFIED_DONE` 是否引用了验证证据、每份 reviewer-facing 交付物是否独立通过隔离检查）；Readiness gate 四标签定义（`ready_to_submit`/`draft_with_placeholders`/`needs_author_input`/`blocked`）。

**包一致性审计**（package-consistency-audit.md，与 QA 互补）：核心规则是**返修包是三个耦合产物**——标红稿、清稿、回复信，改任何一个都可能静默使另外两个失效。九节内容：① 耦合规则（回复信引文必须在展开 `\input`/`\include` 后的手稿源中逐字出现；引文被改必须同轮更新信；`4.5 cm` vs `45 mm` 也算不匹配；刻意渲染替换用重复 `--substitution SOURCE=RENDERED` 登记）；② 标红必须有基线（对照归档原始投稿版，未改动文字标红=虚增工作量，新文字不标=隐藏工作；为审稿人方便复制的**既有**文字用非红色的普通斜体并在信首声明）；③ 删除涟漪（删内容比加内容更危险——删表格行后信里不能说"该表报告了全部四项"，删句要检查引入它的上句是否成孤儿）；④ 计数陈述会过期（不写"加了两句话""三条新引用"，必须写则每轮末尾对手稿核验）；⑤ 位置引用易变（每次重编译后重新核验**每一个**页码/节引用，用 `pdftotext -f 8 -l 8 main.pdf - | grep -c "..."` 验证引文确实落在所称页；节号比页码稳定；绝不发明行号）；⑥ 手稿自洽与术语漂移（跑 `nature-shared/core/consistency-sweep.md` 全套清扫；术语清扫这种"小改动"正是让包失步的高发区；**信里承诺手稿没有的文字是最具破坏性的失败**——极易被查、读起来像编造合规）；⑦ 经得起编辑的礼仪失败（"already specified in Section 2" 式指责要重构为作者侧工作；跨审稿人引用错误，正确做法是**在两个审稿人各自的段落下完整重复**所引手稿段落——这种重复是正确的而非冗余）；⑧ 编译与交付门（标红稿与清稿只允许颜色差异且用两份 PDF 全文 diff 验证、清稿用切换标记宏生成绝不手工删标记、彩色图表环境单独去色、渲染红色最多的页面确认清稿无彩字、用项目自己的引擎编译、零编译错误/零未定义引用/零未定义参考、文献条目数符合预期、**断言每位审稿人的意见数与回复数相等**）；⑨ 审计顺序（手稿自洽清扫→同步受影响引文→重编译→重验页节引用→重跑逐字引文检查→礼仪与隔离→生成清稿跑交付门→打包；第 3-5 步在之后**每次**编辑后重复）。

**验证脚本**：`scripts/check_package_consistency.py`（docstring："Check mechanical consistency across a LaTeX revision package."），默认检查 `\RevisedExcerpt`/`\revtext`/`\oldtext` 引文宏参数与手稿一致、`\ReviewerComment` 与 `\AuthorResponse` 块数配对、归一化修订标记后清稿与标红稿文本一致；支持 `--quote-macro NAME`、`--substitution SOURCE=RENDERED`、`--json`；非零退出码表示仍有机械性不匹配。用法：

```bash
python scripts/check_package_consistency.py --manuscript main.tex --response response.tex --clean main-clean.tex --marked main-marked.tex
```

## 五、关键规则与红线

1. 每条审稿意见忠实保留、编号稳定、必有回应或显式未解决标记；不改写意见至变义。
2. 互盲隔离是默认且不可协商：reviewer-facing 文件只含该审稿人材料，禁止任何跨审稿人措辞、ID、推荐意见；重复关切逐份完整作答（重复是正确而非冗余）。
3. 三态分离：action（该做什么）、work_status（做到哪步、是否已验证）、package_readiness（整包能否提交）互不替代；`VERIFIED_DONE` 必须有可检查产物。
4. 反幻觉：不发明实验、分析、引用、行号、图版、补充材料、编辑指示、审稿人身份、统计数值；缺事实标 `AUTHOR_INPUT_NEEDED` 并保留可见占位符，不把未解决项藏进 LaTeX 注释。
5. 审稿人漏看已有内容 = 呈现不清的信号：直接回答 + 小澄清 + 引用修改后位置；禁止 "we already stated this" 式责难，连 "already specified" 措辞都要重构。
6. 决定类型门槛：包级策略前必须定 Major/Minor；禁止从意见数量/语气/难度推断；`revise-and-resubmit`、`transfer after review` 是独立类型不强行归入。
7. 主文纪律：回复信完整答审稿人，手稿改动保持最短读者向文本；每处新增触发删除检查，优先替换/压缩/SI 搬迁；不把主文写成预防性 rebuttal。
8. 语气红线：不以时间/经费/便利为拒绝实验的首要理由；分歧窄化且循证；避免破折号/冒号作常规连接符；不过度道歉、不空洞致谢（致谢不能充当回复）。
9. 申诉类单独路由，不默认起草申诉信。
10. 包一致性：引文逐字同步、标红对照原始基线、删后查涟漪、页码每轮重验、清稿/标红稿仅颜色差异、每次手稿编辑后重跑 `check_package_consistency.py`。
11. 来源层级：期刊说明与编辑信 > Nature/Springer Nature 流程页 > 编辑建议 > 作者本地事实；期刊当前页面与本技能冲突时以期刊页为准。

## 六、配套资源

**references/（11 个）**：`intake-and-routing.md`（task mode/就绪状态/邮件解析/最小输入/提问规则/路由捷径）；`source-basis.md`（来源层级与政策依据）；`response-structure.md`（输出包九段格式与各件结构模板）；`latex-templates.md`（三个 .tex 模板用法、九条填充规则、交付前 QA）；`comment-taxonomy.md`（严重度四级+八大类别及默认策略）；`action-mapping.md`（12 动作标签、tracker 字段、8 种 work status、就绪度推导、风险四级）；`tone-and-stance.md`（姿态、推荐/禁用句型、分歧与误解与超范围模板、claim 动词分级）；`chinese-author-alignment.md`（中文笔记转换表、`中文核对`块、双语起草）；`difficult-cases.md`（九类难案处理）；`qa-checklist.md`（十组定稿检查）；`package-consistency-audit.md`（三产物耦合的机械一致性审计九节）。

**templates/（3 个 .tex）**：`response-to-reviewers.tex`（单审稿人逐点回复模板，定义 `\ReviewerComment`/`\AuthorResponse`/`\RevisedExcerpt`/`\ManuscriptChange` 宏，文件头注明"一位互盲审稿人一份副本"）；`cover-letter.tex`（编辑向返修信，`letter` 文档类，含稿件号/期刊占位符与三条主要返修动作 itemize）；`revised-manuscript-redline.tex`（标红修改稿骨架，定义 `\revised{...}` 红色宏与 `\deletedtext{...}` 红色删除线宏，明确这是备份标红副本而非清稿）。

**scripts/**：`check_package_consistency.py`——LaTeX 返修包机械一致性检查（引文宏逐字核对、意见/回复配对计数、清稿与标红稿归一化后文本一致）。

**tests/**：两个 pytest 文件（`test_package_consistency.py`、`test_response_instruction_contracts.py`）+ 一个手工评估 rubric（`rubric.md`，按 Decision routing 等维度列 Pass/Fail 条件）+ 10 个 Markdown 场景 fixture：`minor-revision`、`major-revision-missing-evidence`、`conflicting-reviewers`、`impossible-experiment`、`reviewer-visibility`、`defensive-draft-audit`、`task-status-tracking`、`local-edit-without-decision`、`unclear-decision-type`、`evaluation-summary`。

**examples/（3 个 md）**：`minor-revision.md`（小修包的期望输出形态）、`major-revision-with-missing-evidence.md`（作者笔记不完整时如何避免编造合规）、`conflicting-reviewers.md`（编辑指示与证据限度如何约束冲突审稿人诉求下的回复）。均为合成示例，非真实审稿意见。

**README.md / README_EN.md**：中英使用说明——能力清单（解析决定信、Major/Minor 分流、`E.1`/`R1.1` 编号、互盲隔离、`VERIFIED_DONE` 核验、LaTeX 包一致性核对）、典型请求、所需输入、产出清单、边界与相关技能（nature-reviewer、nature-polishing、nature-statistics、nature-ref-verifier）。
