# nature-reviewer

## 一、定位与触发场景

- **用途**：对科学论文全文或节选做**有据可依的模拟同行评审**（mock peer review），
  覆盖 significance、validity、major/minor concerns 三个维度。
- **默认产出**：3 份互盲 reviewer reports + 1 份 post-review synthesis。
- **description 原文摘录**（SKILL.md frontmatter）：

  > "Provide evidence-grounded mock peer review of scientific manuscripts or excerpts,
  > covering significance, validity, and major/minor concerns. Use for
  > 模拟审稿、投稿前自审、审稿人视角评估; not author rebuttal drafting."

- **典型触发请求**：
  - "以 Nature 审稿人视角评估我这篇 manuscript 的 major concerns。"
  - "投稿前自审：帮我跑一轮三审稿人互盲评审。"
  - "这份稿件的技术健全性和跨学科可读性如何？"
- **明确不做什么（边界）**：
  - **不写作者反驳信**：rebuttal / response-to-reviewers 路由到 `nature-response`。
  - 不虚构审稿人身份、编辑决定或 Nature 录用结论；产出是模拟评审，不是期刊决定。
  - 不为凑数量而造 concern；材料不足处写 "None identified from the supplied material"。

## 二、运作模式（加载/路由机制）

### SKILL.md 作为路由

frontmatter 仅 `name` + `description`；正文规定评审的基本规则
（互盲隔离、默认三报告、五轴评估、输出结构），
细节分发到 `manifest.yaml` 声明的 always_load 与 on_demand 参考文件。

### manifest.yaml 结构（version 1.5.0）

- **always_load**（2 项，评审的权威基础）：
  - `references/editorial criteria and processes.md`：
    Nature 官方编辑标准原文摘录（唯一权威本地源），要点包括：
    - original scientific research、outstanding scientific importance、
      interdisciplinary readership 等定义；
    - 送审标准：novel / arresting / immediate and far-reaching implications；
    - 理想 referee report 应指明 who will be interested 与 technical failings；
    - 编辑判断 broad interest，referee 可提示 significance over/undersold。
  - `references/source-basis.md`：来源声明，确认 editorial criteria 文件是唯一权威本地源。
- **axes**：无 axes/fragments 机制（与 nature-writing / nature-polishing 不同，本 skill 不做轴路由）。
- **references.on_demand**（9 条，条件→路径）：
  - 完整隔离评审流程 → `references/reviewer-workflow.md`
  - 评审轴与权重 → `references/review-axes.md`
  - concern ledger 与 Major/Minor/Blocking 校准 → `references/technical-concern-taxonomy.md`
  - 化学/工程/材料/大气/气候生态/水文/遥感 7 个领域的专门审查门
    → `references/domain-specific-review-gates.md`
  - 报告结构 → `references/report-structure.md`
  - 角色边界 → `references/role-boundaries.md`
  - QA 清单 → `references/qa-checklist.md`
  - 冻结后 forensic 一致性审计 → `references/forensic-consistency-audit.md`
  - 术语/数值一致性扫描 → `../nature-shared/core/consistency-sweep.md`

### 对 nature-shared 的依赖

仅一条：一致性扫描复用 `../nature-shared/core/consistency-sweep.md`（相对路径解析）。

## 三、核心工作流

把分散在 reviewer-workflow / report-structure / forensic-consistency-audit
等文件中的流程按执行顺序整合为 **10 步**：

1. **识别输入范围**：确定评审对象是全文还是节选、学科领域、可见材料范围。
2. **构建不可变 review packet**：打包供评审的稿件与证据，**不含任何预判结论**；
   packet 一旦构建不可变。
3. **预定义 emphasis briefs**：为三位 reviewer 设定侧重——默认权重为
   R1=技术健全性（technical soundness）、R2=原创性+科学重要性、
   R3=跨学科读者+非专业可读性。
4. **隔离上下文各跑 reviewer**：三位 reviewer 在互不可见的上下文中独立工作，
   各自构建**私有 concern ledger**。ledger 字段：
   `issue_key / axis / applicability / severity / blocking / severity_rationale /
   claim_pointer / evidence_pointer / evidence_status / concern / resolution_test /
   reviewer_id`。
   - 五轴评估：originality、scientific importance、interdisciplinary readership、
     technical soundness、readability for nonspecialists。
   - 内部 12 轴 technical concern taxonomy 仅作**覆盖检查**用：
     novelty-significance、mechanism-evidence、experimental-design、
     statistical-rigor、reproducibility、clinical-validity、ethical-governance、
     data-resource-quality、figures-and-tables、writing-clarity、
     claim-moderation、causal-vs-correlative。
   - 适用时套用 `domain-specific-review-gates.md` 的领域门
     （化学/工程/材料/大气/气候生态/水文/遥感）。
5. **冻结报告**：三份 reviewer reports 完成后冻结，之后不得因审计或 synthesis 而改写。
6. **冻结后跑 forensic consistency audit**（`forensic-consistency-audit.md`，7 层）：
   - 算术恒等（数字加总/换算是否一致）；
   - metric bounds（指标是否越出定义域）；
   - prose-table/figure 一致（正文叙述与表/图数值一致）；
   - 聚合层级（pooled 与分组数值关系）；
   - provenance / leakage（数据血缘与泄漏）；
   - dispersion 异常（离散度不合理）；
   - reproducibility gate。
   - 发现状态分类：`confirmed_internal_error` / `aggregation_ambiguity` /
     `provenance_gap` / `suspected_duplication` / `unresolved_input_needed` /
     `not_assessable` / `passed`；每项记录 `audit_id`；
     **审计结果不回灌 reviewer**，发现的问题保留为 editor-facing finding。
7. **单独上下文做 cross-review synthesis**：在报告冻结后由独立上下文综合；
   **共识成立条件：同一 issue key 须由 ≥2 份报告独立提出**。
8. **QA**：按 `qa-checklist.md` 做最终质量检查。
9. **（贯穿）severity 校准**：按 `technical-concern-taxonomy.md` 校准
   Major/Minor/Blocking——Blocking=Yes 仅当"该问题不解决则核心论证不成立"；
   Minor 永远 non-blocking。
10. **（贯穿）证据状态标注**：ledger 中的 `evidence_pointer` 必须指向具体稿件位置或证据，
    `evidence_status` 区分已见证据与缺失材料。

## 四、输出契约与质量检查

### 产出结构（`report-structure.md`）

1. **Review setup**：Input scope / Assessment boundary / Shared manuscript claim
   summary / Visible evidence base / Missing materials affecting confidence。
2. **Reviewer 1–3 三份报告**，各含：
   - Overall assessment；
   - Who would be interested and why；
   - Major strengths；
   - Major Concerns；
   - Minor Comments；
   - Technical failings（对照编辑标准的"技术缺陷"要求）；
   - Assessment against Nature-style criteria；
   - Recommendation posture。
3. **Cross-review synthesis**（post-review；**不展示给 reviewers**）。
4. **Risk / unsupported claims** 小节。

### 条目格式

- **Major Concern**：`R1-M1 [axis]`，字段：
  Severity / Blocking Yes-No / Claim pointer / Evidence pointer / Concern /
  Why it matters / Resolution test。
- **Minor Comment**：`R1-m1`，字段：
  Affected element / Evidence pointer / Issue / Required correction。
- 无内容处写 `None identified from the supplied material`。

### 质量检查

- `qa-checklist.md`：终审 QA 清单。
- `tests/test_reviewer_instruction_contracts.py`：自动化断言 SKILL.md 合同条款。
- `tests/` 4 个行为 fixture：
  punctuation-style、reviewer-independence、severity-tiering、traceable-review。
- 文风红线：避免把破折号/冒号当常规句读；保留连字符与稳定 ID（如 R1-M1）。

## 五、关键规则与红线

1. **不虚构**：不编造审稿人身份、编辑决定、Nature 录用结论；
   不编造稿件中不存在的证据。
2. **互盲隔离**：reviewer 之间互不可见；concern ledger 在冻结前不得跨 reviewer 共享。
3. **packet 不可变**：review packet 不含预判结论，构建后不可修改。
4. **审计纪律**：无算术证明不得称 confirmed_internal_error；
   审计异常一律记录 audit_id 并分类；
   审计发现不回灌 reviewer 报告，保留为 editor-facing finding。
5. **不为凑数造 concern**：Blocking 判定从严；Minor 永不 blocking。
6. **角色边界**（`role-boundaries.md`）：本 skill 只做审稿视角，
   不越界写 rebuttal（归 nature-response）。
7. **grounding**：所有 concern 必须挂 claim_pointer 与 evidence_pointer；
   缺失材料须显式列入 Missing materials affecting confidence。

## 六、配套资源

- **references/**：
  - `editorial criteria and processes.md`：
    Nature 官方编辑标准原文摘录（always_load，唯一权威本地源）。
  - `source-basis.md`：来源权威声明（always_load）。
  - `reviewer-workflow.md`：完整隔离评审流程。
  - `review-axes.md`：五轴定义与 R1/R2/R3 权重。
  - `technical-concern-taxonomy.md`：
    12 轴 concern 分类、ledger 字段、Major/Minor/Blocking 校准。
  - `domain-specific-review-gates.md`：7 学科领域审查门。
  - `report-structure.md`：输出报告结构与条目格式。
  - `role-boundaries.md`：与 nature-response 等 skill 的边界。
  - `qa-checklist.md`：终审 QA。
  - `forensic-consistency-audit.md`：7 层冻结后 forensic 审计协议。
- **tests/**：
  - `test_reviewer_instruction_contracts.py`（合同条款断言）；
  - 4 个行为 fixture（标点风格、评审独立性、severity 分层、可追溯性）。
- **agents/openai.yaml**：
  display_name "Nature Reviewer"，default_prompt 用 `$nature-reviewer` 跑三份互盲评审。
