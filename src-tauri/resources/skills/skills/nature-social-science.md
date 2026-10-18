# nature-social-science

## 一、定位与触发场景

- **用途**：社会科学及相邻学科的**学术写作与选题论证工作台**，
  覆盖选题构思、文献梳理、论证组织、学术表达四大要素。
- **本质**：带 Gate 的 8 步选题与研究设计工作流，而非代写工具。
- **来源**：metadata.author 标注为 community contribution,
  adapted from "Finding a PhD Topic with Codex"。
- **description 原文摘录**（SKILL.md frontmatter）：

  > "社会科学及相邻学科学术写作与选题论证工作台，覆盖选题构思、文献梳理、
  > 论证组织、学术表达四大要素。Use for 找论文题目/研究问题、
  > 把兴趣或宽泛主题变成可讨论的研究方向、文献综述与领域版图、
  > 区分'研究少'与真正未决的知识问题、设计证据路线与主张边界、
  > 新颖性审计与'审稿人2'式攻击、博士/论文可行性评估、导师沟通话术、
  > 或检查论文中的过度断言与概念混用。
  > 也适用于开题报告、研究计划书、学位论文的选题阶段与论证自检。"

- **典型触发请求**：
  - "我对 X 感兴趣，帮我变成一个可研究的方向。"（Mode A）
  - "我有一个宽泛主题，帮我找到具体 research question。"（Mode B）
  - "审计我这个暂定 RQ 的新颖性和可行性。"（Mode C，audit route）
  - "检查我论文里的过度断言与概念混用。"
- **明确不做什么（边界）**：
  - 不替用户生成"正确题目"；不保证新颖/录取/资助；
  - AI 自信不是证据；需要真人判断处会停下；
  - 假起点（只有数据集/方法偏好）须回 Mode A。

## 二、运作模式（加载/路由机制）

### SKILL.md 作为路由

- frontmatter 含 `name` + 中文长 description + `metadata.author`。
- 正文包含以下机制：
  - **路由表**：诉求 → 要素 → 步骤 → 参考文件；
  - **三种入口**：Mode A（只有兴趣）/ Mode B（宽泛主题）/
    Mode C（暂定 RQ，audit route 而非 advanced route）；
  - **假起点识别**：只有数据集/方法偏好 → 回 Mode A；
  - **四条铁律**：
    1. 未知写"未知"；
    2. 主动找矛盾/零结果/反向关系/替代解释；
    3. 保留证据层级/证据状态/失败搜索/关键未知；
    4. 每步更新 Current Research State，不带整段旧对话；
  - **配套禁止 6 条**：
    1. 不生成选题；
    2. 不声称空白/新颖；
    3. 不默认方法；
    4. 不把"本轮没找到"写成"没有研究"；
    5. 不把兴趣写成已证重要性；
    6. 不编造引文；
  - **十二个判断表**：
    Interest≠Topic、Topic≠Research problem、
    Research problem≠Research question、Research gap≠"few studies"、
    Novelty≠nobody has studied it、Interesting≠Important、
    Important≠Answerable、Available source≠Valid evidence、
    Correlation≠Mechanism、Advanced method≠Strong contribution、
    Feasible study≠PhD-worthy project、AI judgement≠Researcher judgement；
  - **8 步工作流表**与 Gate 三态；
  - **回退规则**：弱代理/错误单位/不合适时间 → 回 Question；
    知识后果消失 → 回 Problem；
  - **五拍节奏**：
    1. 读章节确认目的/Gate；
    2. 从 `05-prompt-library.md` 取 prompt，只备 Required input；
    3. 输出写进 `06-worksheets.md` 表格；
    4. 质量检查 + Red flags 需真人判断停下；
    5. 更新 Current Research State，按 `08-usage-handoff.md` 交接。

### manifest.yaml 结构（version 1.0.0）

- **always_load**：`references/01-foundations.md`
  （六概念逐字定义、AI 能力边界表、evidence levels 四级）。
- **axes**：无。
- **references.on_demand**（8 条，按 Step 路由）：
  - Steps 1–3 → `references/02-*.md`
  - Steps 4–5 → `references/03-*.md`
  - Steps 6–8 → `references/04-*.md`
  - prompt 唯一权威版本 → `references/05-prompt-library.md`
  - 全部 worksheet 表格 → `references/06-worksheets.md`
  - 状态术语系统 → `references/07-status-terminology.md`
  - 交接与恢复 → `references/08-usage-handoff.md`
  - 学术表达与 claim ceiling → `references/09-academic-expression.md`
- **明确声明**：不使用 nature-shared 层。

## 三、核心工作流

### 八步主流程（每步 Gate = PASS / REVISE / STOP；Step 5 起加 PARK）

1. **Step 1 Research Search Space（一页）**：
   五项工作 = 拆概念 / 扩展相邻词汇 / 边界选项 / 暴露预设 / 形成搜索范围；
   **禁止在此步生成候选问题**。
2. **Step 2 Research Landscape**：
   Quick Run + Deep Run 两档；主动搜反证；标证据层级；
   记录检索日期、检索式、失败搜索。
3. **Step 3 Unresolved Problems（3–7 张问题卡）**：
   - 四问检验：已知什么 / 什么未决 / 解决后改变什么理解 / 什么证据可区分；
   - 九种较强问题结构：矛盾发现、竞争解释、机制不清、测量失败、
     时间过程、异质性、尺度错位、新证据、有科学理由的整合。
4. **Step 4 Evidence Map（1–3 条证据路线）**：
   - Check Ask 八项：Construct / Unit / Timing / Coverage / Access /
     Ethics / Source-generation bias / Claim ceiling；
   - 证据状态四级 **NAMED → DOCUMENTED → ACCESS-CONFIRMED → TESTED，
     不可跳跃**；
   - claim ceiling 示例：访谈不能估计总体比例、一次问卷不能观察个体变化、
     机构文件不能代表执行经验、平台日志不一定说明动机。
5. **Step 5 Candidate RQs（6–12 张候选卡，启用 PARK）**：
   - 三条生成路线：problem-driven / evidence-driven / mechanism-driven；
   - Candidate RQ Card 字段：候选编号 / 生成路线 / 知识问题 / 准确问句 /
     核心概念 / 人群情境材料 / 分析单位 / 时间边界 / 证据路线与状态 /
     拟议机制 / 竞争解释 / 可观察区分痕迹 / 最大可辩护主张 / 贡献假设 /
     最可能致命缺陷 / 进入 Step6 前必须核实 / 假设与未知；
   - 合并伪变体；**禁止排序**。
6. **Step 6 Academic Survival**（两层审查）：
   - **Novelty Audit**：找最近似工作，比较是否已答同一知识问题；
     搜索失败保留边界；
   - **Reviewer 2 Sheet**：七部位攻击（新颖性/构念/理论/证据/方法逻辑/
     替代解释/贡献）；最强拒绝理由 + 致命性 + 反证/负案例 + 隐藏假设 +
     主张上限 + 修订路径 + 决定性核实动作；
   - 防五种错误：精确组合谬误/构念漂移/理论装饰/证据过度承诺/贡献循环；
   - 处置 **SURVIVE / REDESIGN / PARK / REJECT**；
   - 不靠总分/多数票；保留审稿分歧。
7. **Step 7 PhD Feasibility**：
   - 硬门槛 8 项：核心证据 / 访问与许可 / 伦理 / 研究者安全 / 时间 /
     必要技能 / 监督与资源 / 科学信息量；
   - 每项 **CONFIRMED / RESOLVABLE DEPENDENCY / UNKNOWN / FAIL**，
     UNKNOWN≠通过、FAIL 不被平均；
   - 三层检查：minimum viable study / 博士发展路径 / 过度扩张警报；
   - 12 个月依赖图：责任人 / 最早验证动作 / 所需证据 / 失败备选 / 决定日期；
   - minimal reality check（合规、不找显著结果）；
   - PhD Feasibility Dossier；
   - 处置 **PROCEED TO COMPARISON / REDESIGN / PARK / REJECT**；
   - 不虚构导师匹配。
8. **Step 8 Final Shortlist（≤3 张 Topic Cards）**：
   - 入围资格：Step6 SURVIVE 或完成 REDESIGN、Step7 PROCEED、
     无未处理 FAIL；
   - Final Topic Comparison Matrix **文字判断不打分**，13 个比较项：
     知识吸引力 / 重要性 / 可回答性与主张上限 / 贡献逻辑 /
     核心证据及状态 / 最大硬风险与未知 / 伦理与研究者安全 /
     学习和执行负担 / 监督与资源需要 / Minimum viable study /
     博士发展空间 / Fatal failure condition / 一个决定性下一步；
   - **PhD Topic Card 18 字段**：Working direction / Research puzzle /
     What we already know / What remains unresolved /
     Exact research question / Why it matters /
     Proposed explanation or mechanism / Population setting or materials /
     Evidence and access status / Likely method and maximum defensible claim /
     Main contribution / Biggest risk and fatal failure condition /
     Why this is PhD-sized / Minimum viable first study /
     Possible thesis extensions / Required skills and ethics /
     Potential supervision fit / Next verification action；
   - 区分"可核实减少的未知"与"学生价值取舍"；**不选 winner**。

### 状态术语系统（`07-status-terminology.md`）

- 五系统状态表：
  Workflow Gate（PASS/REVISE/STOP）；
  Evidence State（NAMED/DOCUMENTED/ACCESS-CONFIRMED/TESTED）；
  Academic Survival（SURVIVE/REDESIGN/PARK/REJECT）；
  Feasibility Dependency（CONFIRMED/RESOLVABLE DEPENDENCY/UNKNOWN/FAIL）；
  Feasibility Disposition（PROCEED TO COMPARISON/REDESIGN/PARK/REJECT）。
- 六条不等式：
  UNKNOWN≠PASS、DOCUMENTED≠ACCESS-CONFIRMED、SURVIVE≠novel、
  PARK≠bad idea、PASS≠permanent approval、REJECT≠wasted work。
- 不建综合状态；
- Terminology Mini Reference：mechanism / claim ceiling /
  source-generation bias / minimum viable study / minimal reality check /
  PhD-sized / fatal failure condition 等；
- Appendix D Current Research State 一页模板。

### 交接与恢复（`08-usage-handoff.md`）

- Entry→1→…→8→Supervisor 每段四栏：
  Carry forward / Leave behind / Human decision / Blocking unknown。
- Resume rule 5 步：
  1. 打开 CURRENT_RESEARCH_STATE.md；
  2. 看 CURRENT GATE 和 MOST IMPORTANT UNKNOWN；
  3. 只开对应 worksheet 和 prompt；
  4. 检查可变事实；
  5. 执行 NEXT VERIFICATION ACTION，不重放对话。

## 四、输出契约与质量检查

- **产出**：每步输出写入 `06-worksheets.md` 的 11 张表：
  1 Entry Diagnostic、2 Research Search Space、3 Research Landscape Map、
  4 Unresolved Problem Checklist、5 Evidence Map、6 Candidate RQ Card、
  7 Novelty Audit、8 Reviewer 2 Sheet、9 PhD Feasibility Check、
  10 Final Topic Comparison Matrix、11 PhD Topic Card；
  每张标填写时机 + Purpose + What decision，逐字填空结构。
- **prompt 来源**：`05-prompt-library.md` 是唯一权威版本，
  13 条 prompt 总览：
  Interest Mapper；Research Landscape Mapper QUICK/DEEP；
  Unresolved Problem Finder；Evidence Mapper QUICK/DEEP；
  Candidate RQ Generator；Academic Survival Triage QUICK；
  Novelty Skeptic + Reviewer 2 DEEP；PhD Feasibility Triage QUICK；
  Feasibility Auditor + PhD Scope Auditor DEEP；
  Topic Comparator + Topic Card Builder。
  每条含 When to use / Required input / Stop condition / What to save +
  逐字 prompt 正文（字母节输出格式 + 禁止项 + 证据核实要求 + 停止条件）。
- **质量检查**：
  - 每步 Gate 三态 + Red flags（需真人判断处停下）；
  - `01-foundations.md` 的 evidence levels 四级
    （METADATA ONLY / ABSTRACT LEVEL / FULL-TEXT LEVEL / PRIMARY/OFFICIAL，
    与 Step4 的 evidence state **不可互换**）；
  - AI 能力边界表：
    AI can lead — search / terminology expansion / source comparison /
    contradiction hunting / structured checking；
    Researcher must own — construct refinement / intellectual commitment /
    disciplinary positioning / ethics and positionality / acceptable risk /
    supervision judgement。

## 五、关键规则与红线

1. **四条铁律**：未知写"未知"；主动找矛盾/零结果/反向关系/替代解释；
   保留证据层级/证据状态/失败搜索/关键未知；
   每步更新 Current Research State 不带整段旧对话。
2. **禁止 6 条**：不生成选题、不声称空白/新颖、不默认方法、
   不把"本轮没找到"写成"没有研究"、不把兴趣写成已证重要性、不编造引文。
3. **十二条不等式**（见 SKILL.md 判断表，
   如 Correlation≠Mechanism、AI judgement≠Researcher judgement）。
4. **证据状态不可跳跃**；UNKNOWN≠通过；FAIL 不被平均。
5. **落地句式纪律**（`09-academic-expression.md`）：
   - claim ceiling 先行（四条范式句式）；
   - 访问依赖诚实措辞表
     （NAMED/DOCUMENTED/ACCESS-CONFIRMED/TESTED 对应句式）；
   - "不要写→改写为"表，例如：
     "无人研究过，首次提出" →
     "在本次数据库、时间、语言和检索式内未找到；不能据此断言不存在"；
   - 三条判断纪律：AI 不能替真人授权 / 不虚构导师匹配 /
     不把兴趣写成已证重要性。
6. **文献梳理按问题结构排列，不按论文排列**；
   论证组织七项攻击写法 + 最强反对写法 + 可修复 vs 硬失败区分。

## 六、配套资源

- **references/**：
  - `01-foundations.md`（always_load）：
    六概念逐字定义、AI 能力边界表、evidence levels 四级、找真人时机。
  - `02-*.md`：Steps 1–3 详解
    （每步含目标/为什么重要/输入模板/Codex 应做/输出形态含微型示例与
    空白可复用结构/通过标准 checklist/Red flags/Gate/Handoff 四栏）。
  - `03-*.md`：Steps 4–5 详解。
  - `04-*.md`：Steps 6–8 详解。
  - `05-prompt-library.md`：13 条 prompt 的唯一权威版本。
  - `06-worksheets.md`：11 张工作表。
  - `07-status-terminology.md`：五系统状态表 + 六条不等式 +
    Current Research State 模板。
  - `08-usage-handoff.md`：交接四栏与 Resume rule。
  - `09-academic-expression.md`：claim ceiling、表达错误、落地句式表。
- **README.md**：
  四大要素、Mode A/B/C 用途、产出清单、边界声明、
  相关技能（nature-writing / nature-reviewer / nature-polishing /
  academic-investigation）。
- **agents/openai.yaml**：
  display_name "Nature Social Science"，
  default_prompt 用 `$nature-social-science` 做研究空间/候选审计/过度断言检查。
