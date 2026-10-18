# nature-citation（本地版）

> **本地化改造说明**：本技能只做两件事——本地文本分析（claim 拆分与匹配）与对 CrossRef/PubMed 等**公开免费 API 的只读元数据查询**（拉取，不推送）；RIS/ENW/Zotero RDF/HTML 全部是**写入本地磁盘的导出文件**（Zotero RDF 只是一种文件格式，不与 Zotero 软件建立任何连接）。用户已确认 DOI 时可完全离线运行（`--offline`）。无任何第三方账号、key、消息推送或外发动作。

## 一、定位与触发场景

**用途**：把手稿段落/单条科学判断拆成可引用的 claim，为每个 claim 在严格的 Nature/CNS 期刊族范围内寻找并验证支撑文献，产出 claim↔文献映射和文献管理器导出文件（RIS/ENW/Zotero RDF）。

**description 原文摘录**（SKILL.md frontmatter）：

> "Find and verify Nature/CNS-family literature supporting manuscript claims, with claim-to-source mapping and reference-manager export. Use for Nature系列引用、CNS支撑文献、分段补引用 when this journal scope is requested; use broader literature search for unrestricted sources."

**典型触发请求**（README.md / evals）：
- "把这段 introduction 分段补 Nature 系列引用。"
- "只用 CNS 及子刊，为这些 claim 找近五年的支撑文献。"
- "我已经确认这些 DOI，帮我导出 Zotero 可导入文件。"
- "把这段文字自动分段并给出 Nature/CNS 及其子刊引用，导出 Zotero RDF 格式和 HTML 可视化。"
- "只看 Nature 系列……给我文本和引用的对应关系，方便插入论文。"

**明确不做什么（边界）**：
- 期刊范围限定 Nature Portfolio / Science family / Cell Press（或仅旗舰刊）；需要不限范围的文献检索时改用 nature-academic-search。
- 只把论文作为候选支撑，不替作者保证其必然适合最终引用。
- 不使用博客、新闻稿或搜索摘要作为唯一依据。
- 文献只能支撑相邻但不完全相同的 claim 时，明确标注证据偏差。
- 医学/临床/安全性 claim：声明引用不能替代临床指南或系统综述。

## 二、运作模式（加载/路由机制）

### SKILL.md 作为路由

SKILL.md 标题为 "Nature Citation — Router"。routing protocol：

1. 读 `manifest.yaml`，再读 `always_load` 全部文件。
2. **无内容轴**：与其他 nature-* skill 不同，本 skill 没有 fragment 轴；变化维度是运行时参数而非不同内容体：
   - **journal scope**——从用户措辞读出（`Nature系列` / `CNS` / `CNS及子刊` / flagship-only），传给脚本 `--scope`；
   - **user language**——用户写中文或要求中文指导时，读 `static/core/chinese-mode.md`（中文注释、英文检索式）；
   - **input length**——超过 ~10 个分段时切换到 `references/script-usage.md` 的批量长文策略。
   检索前用一句话声明检测到的 scope 与日期限制。
3. 执行 `core/workflow.md` 七步流程。有网时优先用 `scripts/nature_citation.py` 做检索/导出；DOI 元数据缺 given name 时按 PMID 重取或与出版社核对，**不得导出仅姓氏的 `AU` 字段**。
4. references 按需加载（见下表）。

### manifest.yaml 结构

- `always_load`：`static/core/principles.md`（产出物、严格期刊范围、来源层级、检索质量规则）、`static/core/workflow.md`（七步流程与最终报告格式）。
- **无 axes**（manifest 设计说明：本 skill 是线性参数化流程 segment→parse→search→evaluate→export→report，变化由 `--scope`、`--batch-size` 等 flag 处理；重型、偶尔需要的材料放 references 以保证常规短任务轻量）。
- `references.on_demand`（条件 → 路径）：
  - 用户写中文/要求中文指导 → `static/core/chinese-mode.md`
  - 运行脚本（完整 flag 列表、长文批量策略、速查表）→ `references/script-usage.md`
  - 把 claim 翻译成检索式与支撑等级 → `references/search-strategy.md`
  - Nature/CNS 期刊族边界与官方来源说明 → `references/journal-scope.md`
  - RIS/EndNote/Zotero RDF 导出细节 → `references/ris-endnote.md`
  - 分段、检索 Crossref、导出 ENW/RIS/RDF、生成 HTML 浏览器 → `scripts/nature_citation.py`

### 静态层与动态判断的分工

- 静态 core 始终加载：原则（principles）+ 七步流程（workflow）；chinese-mode 是条件加载的 core 文件。
- 动态判断只有三件事：scope、语言、输入长度，全部内联处理，无 fragment 文件。

### 对 nature-shared 的依赖

无。manifest 明确："nature-citation does not use the prose-oriented nature-shared layer."

## 三、核心工作流（七步，static/core/workflow.md）

**Step 1 分段（Segment）**
- 先按段落边界、再按句子边界切分；每段聚焦一个可引用观点；保留原序和稳定分段 ID（`S001`、`S002`…）；跳过明显不可引用的连接句（除非用户要求句句引用）。
- 默认规则：空行为段落界；段落 >约 700 字符或含多个 claim 则拆成句；过短碎片并入邻段（除非含独立 claim）；章节标题只作标签不作可引段。>约 10 段进批量模式。

**Step 2 解析（Parse）**
- 每段抽取一句话核心 claim；识别 claim 类型（`mechanism`/`association`/`method`/`clinical`/`epidemiology`/`background`/`definition`/`review-context`）；识别实体、干预/暴露、结局、人群/模型、方向性、边界。
- 转成 2–4 条英文检索式：一条精确（全关键词）、一条同义词、一条较宽背景、一条方法/模型（如相关）。claim 过宽时拆成子 claim 再检索。

**Step 3 检索候选**
- 有网时首选 `scripts/nature_citation.py`；生物医学主题同时用 PubMed（带期刊过滤器）并与 Crossref 结果比对；重复检索遵守 NCBI E-utilities 限速并带 `tool`/`email` 参数。

**Step 4 保守评估支撑等级**
- 五级：`strong support`（直接检验同一关系/机制/方法且结果支持）、`partial support`、`background support`、`contradictory/limiting`、`metadata-only candidate`。
- **绝不**把 metadata-only candidate 未查摘要/出版社页就引为支撑；综述标为 review/context，有原始研究时不用综述作实验性 claim 的首要证据。

**Step 5 导出文献管理器文件**
- 默认写一个 `references.ris`；可选 `references.enw`（EndNote tagged）或 `references.rdf`（Zotero RDF）；支持 `--from-year`/`--to-year`；除非用户明确要多份，否则只导出一个文件；缺字段（DOI、页码、卷、期）保持缺失，**不虚构**。
- 导出前跑 author-integrity gate：
  - 只用 Crossref/PubMed/出版社的结构化有序作者表；**绝不**从显示引文、`et al.` 串或仅姓氏列表推导 `AU`；
  - 每人一行 `AU`，`Family, Given` 形式，保留首字母、后缀（Jr./III）、名字颗粒与源顺序；
  - 团体/机构作者作为 corporate author，**尾部加逗号**（如 `AU  - NMSS Validation Group,`），内嵌逗号先双写，防 EndNote 倒置；
  - 任一作者缺姓或名/首字母 → **停止导出**，按 DOI/PMID 重取并与出版社记录比对；不完整期间不得声称文件 EndNote-ready；
  - `--allow-incomplete-authors` 仅是显式紧急豁免，警告保留在导出记录中。

**Step 6 可选审查 artifacts**
- 长文或歧义场景用 `--with-artifacts` 生成 HTML/TSV/JSON/report，HTML 浏览器是用户筛选/选择/下载候选的主要方式；启用时显著报告 HTML 路径。

**Step 7 报告**
- 固定中文模板：交互式引用浏览器路径（仅在确实生成时放最前，否则整块省略，不得为填模板编造路径）→ 检索范围（期刊族 + 日期限制）→ 分段引用对应关系（S001: 源句 / 候选 Author-year-title-journal-DOI / 支撑等级 / 插入建议）→ 导出文件绝对路径 → 风险和缺口（未做全文核对、矛盾证据、无直接 CNS 文献等）。无合适 CNS/Nature 文献时直说，仅在用户要求时建议非 CNS 邻近选项。长文提及所用批量策略（`--batch-size`/`--max-segments`）。

### 期刊范围（principles.md + journal-scope.md）

- `Nature系列`：Nature Portfolio 优先——`Nature`、`Nature [field]`、`Nature Communications`、`Communications [field]`、`npj`、`Scientific Reports`。
- `CNS`：`Cell`、`Nature`、`Science` 及主要姊妹刊。
- `CNS及其子刊`：仅接受的旗舰+子刊白名单（Nature Portfolio / AAAS Science family / Cell Press；Science family 默认含 Science、Science Advances、Science Translational Medicine、Science Signaling、Science Immunology、Science Robotics，**不含** AAAS Partner Journal；Cell Press 含旗舰 Cell、主要研究刊、Cell Reports 系列、`Trends in *`）。
- `只要正刊/flagship only`：仅 `Nature`、`Science`、`Cell`。
- 名字含 "nature" 的无关期刊不算；投稿关键边界须用时核对官方期刊页。

### 来源层级（principles.md）

1. 结构化元数据（Crossref、PubMed/NCBI E-utilities、DOI 元数据）→ 2. 出版社页面（nature.com、science.org、cell.com）→ 3. 全文/摘要页 → 4. Google Scholar/Semantic Scholar/WoS/Scopus 仅作发现线索，不作唯一支撑依据。元数据与出版社页面冲突时保留 DOI 与期刊页事实并标记差异。

### 中文模式（chinese-mode.md）

中文输入照收，但用英文概念检索式检索（除非主题是中国特有/中文文献）；分段注释与证据注释默认中文；支撑等级中文标注：`强支撑`/`部分支撑`/`背景支撑`/`不建议引用为该句支撑`。

## 四、输出契约与质量检查

- 必产出：一份文献管理器文件（默认 `references.ris`）；分段↔候选映射；支撑等级证据注释。
- 命名规范：`references.ris` / `references.enw` / `references.rdf`；HTML artifact 为 `citation_visualization.html`。
- RIS 规范：期刊文章 `TY  - JOUR`；每人一行 `AU` 保序；`ER  -` 结束每条；DOI 写 `DO`；PMID 存 `AN  - PMID:…` 以便溯源；摘要不长拷进 RIS。
- 自检（author-integrity preflight，ris-endnote.md 五条）：结构化有序作者表 / 每人有姓+名或首字母 / 后缀颗粒团体作者保留 / 不缩写为 et al. / Crossref 作者不全的生物医学记录按 PMID 重取并与 PubMed 或出版社比对。反例 `AU  - Chaudhuri`（仅姓氏）禁止交付；正例 `AU  - Chaudhuri, K Ray`。
- 长文批量策略速查：1–10 段一次跑、全内联分析；11–25 段 `--batch-size 10` + 紧凑汇总表 + 指向 HTML；26+ 段按章节拆分、每节跑一次、仅紧凑汇总 + HTML。批量产物合并时按 DOI 去重。
- EndNote 导入指引：File > Import > File，选 `.ris`，Import Option 设 `Reference Manager (RIS)`（菜单名随版本/OS 略异，不做过度具体的 UI 断言）。
- 验证脚本即主脚本本身：`scripts/nature_citation.py`（最小运行示例：`python scripts/nature_citation.py --text "..." --scope cns --outdir /tmp/nature-citation --format ris --with-artifacts`）；`tests/test_author_exports.py` 测作者导出完整性。

## 五、关键规则与红线

- **绝不**仅因标题相关就把论文当作支撑；**绝不**未查摘要/出版社页就引用 metadata-only candidate。
- **不虚构**缺失书目字段；缺 DOI/页码/卷期就留空。
- 作者完整性是硬门：仅姓氏 `AU` 禁止导出；缺名时停止导出并按 PMID 重取/核对出版社；`--allow-incomplete-authors` 不得用于把导出描述为"已验证"。
- 被引数只作 tie-breaker，不作支撑证据。
- 可见的撤稿、更正、expression of concern 必须捕获并呈现。
- 精确匹配优先于数量：有用的答案通常是 3–8 个候选，而非 50 篇松散相关。
- 日期敏感主题必须即时检索并明示检索日期。
- 综述不作实验性 claim 的首要证据（有原始研究时）。
- 常见失败模式需警惕：同疾病不同机制、把关联当因果、物种/细胞/人群不符、claim 过宽单引用撑不住、刊名含 "Nature" 但非 Nature Portfolio。
- 中文模式红线：不因标题相关就宣称支撑（重申）。

## 六、配套资源

**references/**：
- `script-usage.md` — 脚本完整 flag 表（`--text/--text-file/--claim/--doi/--pmid/--scope nature|flagship/--from-year/--rows/--per-segment/--max-retries/--format/--mailto/--ncbi-email/--batch-size/--max-segments/--sleep` 等）、长文批量策略、作者元数据修复路径（补 PMID 重跑）。
- `search-strategy.md` — claim 拆解（phenomenon/entity/relationship/context/boundary）、三级检索式（precise/synonym/broad）、支撑等级表、证据注释模板、常见失败模式与改进检索动作。
- `journal-scope.md` — 三大期刊族默认边界、flagship-only 范围、官方来源说明。
- `ris-endnote.md` — RIS 标签映射、author-integrity preflight、EndNote 导入指引、Zotero RDF 结构（`bib:Article`、`foaf:Person` in `rdf:Seq`、期刊容器节点按 ISSN/卷期去重）。

**scripts/**：
- `nature_citation.py` — 一体化主脚本：分段、Crossref 检索（期刊白名单内置）、导出 RIS/ENW/Zotero RDF、可选生成 HTML 引用浏览器；默认 author-integrity 拦截。

**tests/**：`test_author_exports.py` — 作者导出规则测试。

**evals/evals.json**：4 条代表性 prompt——①分段+Nature/CNS 子刊引用+导出 Zotero RDF+HTML 可视化；②仅 Nature 系列的分段-引用对应表；③仅旗舰刊（Nature/Science/Cell）+ RIS 导出且不虚构元数据；④带年份筛选、可勾选、可下载 ENW/RIS/RDF 的交互式引用浏览器。

**agents/openai.yaml**：agent 装配声明。无 templates/、无 static/fragments/（设计上无内容轴）。
