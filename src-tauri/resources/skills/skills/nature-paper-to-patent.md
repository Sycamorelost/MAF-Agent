# nature-paper-to-patent（本地版）

> **本地化改造说明**：本技能产出（权利要求书/说明书/交底书 DOCX、Markdown）全部在本地生成，不向专利局、代理机构或任何第三方平台提交/推送。Step 5 查新保留，但只走**对公开网站的只读查询**（通用网页检索/抓取国知局公布公告站 `epub.cnipa.gov.cn` 与 Google Patents 的公开页面）：**不运行 Playwright 版 `cnipa_epub_search.py`/`cnipa_epub_crawler.py`，不安装/驱动任何浏览器**；WAF 拦截或公开渠道查不到时如实标注检索缺口，不寻求登录态或自动化绕过。mermaid 图渲染（`mmdc`/headless Chrome）属可选本地工具：本机已有则用，没有就交付保留 mermaid 围栏源码的 Markdown（任何支持 mermaid 的阅读器可直接出图），**不自动下载浏览器二进制**。

## 一、定位与触发场景

**用途**：把科研论文、学位论文、技术报告、源代码、图表或发明人笔记等研究材料，转换为**有证据约束（evidence-grounded）的中国发明专利草稿**（权利要求书/说明书/摘要/摘要附图 DOCX 包），或从项目文档与代码中挖掘专利点、生成可交给代理人继续修改的中文**技术交底书**（Markdown + Word）。核心纪律是：每个权利要求特征必须能回溯到稳定 source ID，证据不足的特征标记为缺口而不是写进正式权利要求。

**description 原文摘录**（SKILL.md frontmatter）：

> "Turn research papers or inventor materials into evidence-grounded Chinese invention patent drafts and technical disclosures. Use for 技术交底书、专利撰写、现有技术对比 and Chinese DOCX patent packages; not general manuscript writing."

**典型触发请求**（README）：

- "把这篇论文转成中国发明专利交底书和权利要求草稿。"
- "扫一下这个项目，挖几个专利点，生成技术交底书。"
- "基于这版交底书补充新材料，另存一版 Word。"
- "只提取可专利点，先不要写完整说明书。"
- "对比论文 claim 和专利 claim，看看哪些技术特征缺证据。"

**明确不做什么（边界）**：

- 不是通用论文写作（not general manuscript writing）；不做可专利性意见、侵权意见或申请提交保证——产出是"供发明人及专利代理师复核的起草辅助"。
- 不虚构实施例、实验数据、权利要求特征或现有技术对比；不推断发明人身份、权属、未公开的实现细节、发表日期、现有技术结论或法律充分性。
- 不凭论文摘要或贡献列表直接起草申请文件（SKILL.md 第 1 节明令）。
- 缺事实时在正式权利要求之外用 `[TO CONFIRM: 具体问题]` 标记，而不是编造。

## 二、运作模式（加载/路由机制）

**SKILL.md 作为路由**，共 5 节：①加载 workflow（读 manifest → 读全部 always_load → 检测三轴并一行声明检测值 → 只加载匹配的 fragments，references 按条件加载）；②源锚定规则（P001/E001/F001/C001 四类稳定 source ID，support state 仅限 `explicit` / `inherent` / `needs-confirmation` / `unsupported`，`unsupported` 特征不得进入正式权利要求）；③分阶段门控起草；④产出中文正式文件；⑤交付前验证。

**manifest.yaml 结构**（version 1.0.0）：

- `always_load`（3 件）：
  - `static/core/principles.md`（10 条核心原则）
  - `static/core/workflow.md`（Stage 0–8 流程与各阶段 gate）
  - `static/core/output-contract.md`（产出契约与质量阈值）
- `axes.source_format`（源格式轴，`default: pdf-text`，`multi: true`）：
  - `pdf-text`（可选文字的 PDF）→ `static/fragments/source/pdf-text.md`；
  - `scanned-pdf`（纯图像/需 OCR 的 PDF）→ `static/fragments/source/scanned-pdf.md`；
  - `pasted-text`（用户粘贴的文本）→ `static/fragments/source/pasted-text.md`；
  - `mixed-project`（工作区还含代码、补充材料、发明人笔记、图或已有专利）→ `static/fragments/source/mixed-project.md`。
- `axes.task_mode`（任务模式轴，`default: full-draft`，`multi: false`）：
  - `full-draft`（完整申请包）→ `static/fragments/task/full-draft.md`；
  - `claim-set`（只要权利要求）→ `static/fragments/task/claim-set.md`；
  - `disclosure-analysis`（面向可专利性的技术提取）→ `static/fragments/task/disclosure-analysis.md`；
  - `technical-disclosure`（专利点挖掘、查新、面向代理人的中文技术交底书）→ `static/fragments/task/technical-disclosure.md`；
  - `disclosure-iteration`（在已有交底书草稿上继续迭代）→ `static/fragments/task/disclosure-iteration.md`；
  - `paper-patent-audit`（论文与已有专利对照）→ `static/fragments/task/paper-patent-audit.md`。
- `axes.invention_type`（发明类型轴，`default: algorithm-software`，`multi: false`）：
  - `algorithm-software`（数据处理/AI/信号处理/控制/计算机实现方法）→ `static/fragments/invention/algorithm-software.md`；
  - `apparatus-system`（物理结构或协作装置）→ `static/fragments/invention/apparatus-system.md`；
  - `process-material`（制造/化学/材料制备）→ `static/fragments/invention/process-material.md`；
  - `mixed`（单一类别不足以覆盖）→ `static/fragments/invention/mixed.md`。
- `references.on_demand`（条件 → 路径）：
  - 起草或审计中文权利要求与说明书 → `references/cn-patent-drafting-guide.md`；
  - 绘制附图或选取论文方法图 → `references/patent-figure-guide.md`；
  - 填充或验证结构化草稿 → `references/draft-schema.md`；
  - 从论文-专利成对样例学习转换模式 → `references/corpus-derived-patterns.md`；
  - 对照给定论文与已有专利 → `references/corpus-pair-audit.md`。

**静态层与动态判断的分工**：core 三件恒定加载，规定原则、阶段门与产出契约；source/task/invention 三组 fragments 由轴检测值决定加载哪些；正式申请包走 workflow.md 的 Stage 0–8，交底书（technical-disclosure / disclosure-iteration）走 `references/disclosure/` 下的 Step 1–8 参考链，两者是同一技能内并列的两条工作流。

**对 nature-shared 的依赖**：manifest.yaml 中无 `../nature-shared/...` 引用，该 skill 自包含。

## 三、核心工作流（实际功能流程）

### A. 正式申请包工作流（full-draft / claim-set / disclosure-analysis / paper-patent-audit）

#### 阶段 0：接收与风险标记（Stage 0）

记录源文件、请求交付物、目标法域、发表状态、已知公开日期、缺失的发明人事实。产出 `work/00-intake.json`。Gate：每个输入文件已识别；未知的发表/发明人/权属事实被显式标记。

- source 轴 fragments 的具体要求：
  - **pdf-text**：运行 `scripts/extract_pdf_text.py` 并保留页标记（`===== PAGE N =====`）；抽查首页/中间页/末页的抽取覆盖率；公式与图须对照原 PDF 检查，因为文本抽取可能丢失符号、上下标或阅读顺序。
  - **scanned-pdf**：不得把"抽取为空"当作"披露为空"；可行时做带页坐标的 OCR；标记低置信度的符号、公式、表格、图注；任何会实质限制权利要求的不确定内容必须人工确认。
  - **pasted-text**：除非用户确认已含完整技术披露，否则按不完整输入处理；建段落级 source ID；只有当缺失的图/公式/实现细节/实验上下文阻碍所请交付物时才追问，否则以草稿模式继续。
  - **mixed-project**：清点每篇论文、补充材料、代码目录、发明人笔记、源图、已有专利并分别记录出处；源代码可支持实现细节但不自动确立发明人身份或发表状态；当论文与专利文件编号相同时，须从内容核实技术对应关系。

#### 阶段 1：源映射（Stage 1）

抽取全部实质性披露（不只是摘要），为文本块、公式、图、补充/代码证据分配稳定 ID（P=论文文本、E=公式、F=源图、C=代码或补充证据），保留页/节/图注/文件/行号定位。产出 `work/01-source-map.json`。Gate：方法、实现、实验、局限、公式、方法图均已检查或标记不可用。

#### 阶段 2：技术清单（Stage 2）

建五份清单：术语台账（每个对象一个规范中文术语）；输入-操作-输出映射；公式清单（含符号与技术角色）；图清单（区分方法图与结果图）；实现缺口清单。产出 `work/02-technical-inventory.json`。Gate：每个核心操作有明确的输入输出；每个核心公式和源方法图有处置结论（disposition）。

- invention 轴 fragments 的具体要求：
  - **algorithm-software**：权利要求锚定技术数据获取、变换、模型/算法操作以及特定技术领域输出或控制动作；保留必要公式与数据依赖；数据集名、benchmark 数值、模型品牌、确切层数放入实施例或更窄权利要求（除非必不可少）。
  - **apparatus-system**：识别物理部件、连接、空间关系、信号/物料路径、工作状态与协作关系；结构稳定后才使用附图标记；不得用功能性模块名替代结构性披露。
  - **process-material**：识别原料、顺序、条件、范围、中间态、产品特性与测量方法；保留已披露的范围边界与单位；不得把单个实施例泛化为无支撑的属或范围。
  - **mixed**：选一个主保护对象，其余类别作为有支撑的回退；在方法/装置/系统/产品/介质权利要求间保持同一技术关系；不机械重复权利要求类别。

#### 阶段 3：证据台账（Stage 3）

把候选特征转为证据台账：逐条赋 support state（explicit/inherent/needs-confirmation/unsupported）、source IDs、技术角色、效果、拟去向。产出 `work/03-evidence-ledger.json`。Gate：无候选必要特征为 `unsupported`；每个 `needs-confirmation` 特征转为发明人问题或被移除。

#### 阶段 4：发明构思与权利要求策略（Stage 4）

写一句话构思：`技术问题 -> 协同技术手段 -> 具体技术输出/效果`。选定主保护对象、必要特征链、回退位置、技术上适当的权利要求类别；不自动添加装置/介质/系统/用途权利要求。产出 `work/04-claim-strategy.json`。Gate：每个必要特征有证据；主权利要求形成闭合的输入-操作-输出链。

- disclosure-analysis 模式到此为止（Stage 0–4）：产出可保护技术链、有源支撑的回退位置、实现缺口、公式、图与发明人问题，不主动生成润色后的正式权利要求。

#### 阶段 5：权利要求（Stage 5）

先起草主独立权利要求，再按同一技术顺序起草从属权利要求，然后追加其他有支撑的类别；为每条实质限定建 claim-feature map（映射到证据 ID）。产出 `work/05-claims.txt` 与 `work/05-claim-map.json`。Gate：`scripts/audit_claims.py` 无 ERROR；每项正式权利要求有已映射的源支撑；正式权利要求中不残留 `[TO CONFIRM]` 标记。

- claim-set 模式跑 Stage 0–5 加 Stage 8 的权利要求相关部分，返回发明构思、证据台账、权利要求、claim-feature map、审计报告与发明人问题；不得暗示被省略的说明书支撑已检查。
- paper-patent-audit 模式：为论文和专利分别建独立 source map，用 `same` / `narrower-in-patent` / `broader-in-patent` / `renamed` / `patent-only` / `paper-only` 六种关系标签逐特征对照，双侧引位置；不推断 patent-only 内容有论文支撑。

#### 阶段 6：说明书与附图（Stage 6）

围绕权利要求起草说明书，从源材料补足实现细节：核心公式、符号定义、备选方案、附图说明、实施例；生成与权利要求对齐的主流程图和有支撑的方法图。产出 `work/06-draft.json`。Gate：每个权利要求术语出现在说明书中；每个权利要求步骤有解释；每幅图被引用；每个核心公式有可编辑数学源与符号定义。

- 附图规则（patent-figure-guide.md）：主方法流程图由独立方法权利要求的有序操作构建；黑线白底、矩形过程节点、实心方向箭头、`图1` 编号、`S1/S2/S3` 步骤标识、简洁中文操作标签；**末节点必须写方法的具体结果**（如"输出缺陷检测结果""获得电池健康状态估计结果"），结果名与独立权利要求逐字一致，禁用"技术结果/处理结果/最终结果"等笼统词。摘要附图默认复用主流程图（同一绘图文件、图号、节点标签、箭头），同时嵌入摘要附图 DOCX、摘要 DOCX 与说明书 DOCX。一致性 10 条：每个 `claim_step` 出现在方法权利要求中、标记 `complete_claim_flow` 的总图覆盖该权利要求每个编号步骤、节点顺序符合权利要求数据流、每条边连接已存在节点、每个节点可达、图注与图号标题一致、实施例解释每个节点操作、可选细节留在主流之外、`abstract_figure_number` 指向已存在的完整主图、摘要与说明书复用同一图像文件。
- 公式规则：核心公式在说明书中以专利记法重现并解释；逐条满足——连续编号、在起草记录中标源页与论文公式号、定义每个变量/下标/算子/范数/权重、说明输入与输出、关联到方法步骤与技术效果；禁止只写"按公式计算"、禁止堆无关评价指标凑数；DOCX 中渲染为原生可编辑 Office Math，不用裸 LaTeX 字符串充数。

#### 阶段 7：摘要与打包（Stage 7）

最后写摘要，术语与主权利要求对齐；摘要附图与说明书主图为同一张。产出：`outputs/` 下的中文分件 DOCX、SVG/PNG 附图、结构化 JSON 与审计报告。Gate：`scripts/validate_patent_draft.py` 与 `scripts/build_patent_package.py` 均无错完成。

#### 阶段 8：终审（Stage 8）

对证据支撑、权利要求架构、一致性、可实施性、技术效果推理、公式覆盖、附图对齐七个维度打分（1–5，各附一句证据）；列未决发明人问题与公开风险。Gate：达到 output-contract.md 阈值，否则整包标记 `incomplete draft` 并列出改进所需的具体发明人输入。

### B. 技术交底书工作流（technical-disclosure / disclosure-iteration）

technical-disclosure 模式按 `static/fragments/task/technical-disclosure.md` 指定的顺序依次加载并执行 `references/disclosure/` 下的参考文件：

#### Step 1：边界与输入录入（intake.md）

用少量问题收敛案件边界：Q1 技术主题或产品模块（一句话）、Q2 专利类型倾向（方法/系统/装置/暂不确定）、Q3 文头联系人是否占位（不需要则全部写"待填写"）。信息不全可跳过，由 Agent 推断并注明假设；已确认项用 3–6 行 bullet 复述后进入扫描。

#### Step 2：项目文档扫描（project_scan.md）

按优先级扫描可专利化内容：①专利相关文档（专利点分析、已有交底书、申报建议、创新点汇总）→ ②详细设计/方案文档 → ③核心实现代码（算法策略、业务逻辑、数据处理、规则引擎、接口集成、状态机调度、性能缓存、安全权限等）→ ④系统设计文档。大仓库先搜索定位再精读；记录引用路径便于脱敏表述。**Office 文档硬性规则**：凡出现 `.docx`/`.pptx` 必须先用 `scripts/disclosure/docx_to_md.py` / `pptx_to_md.py` 转 Markdown 再读（旧版 `.doc`/`.ppt` 须先另存为 OOXML），不得只扫纯文本而漏掉 Word/PPT；转换会生成 `{md 主名}_media/` 嵌入图目录。`sample_assets/` 等目录下的独立裸图不作为必须逐个 OCR 的对象（与 Office 内嵌图通常重复），例外是用户点名或关键且未出现在 `_media` 中的图。

#### Step 3–4：专利点挖掘与融合（patent_points_analyzer.md）

列出 3–5 个候选专利点，每个须说明技术背景、创新点、与现有技术区别（可先基于材料推断）、可实施性；可基于已有事实适度推演但须有技术合理性。随后检查可否合并创新要素，优先融合为"方法+系统"类专利，突出组合创新。**默认只产出最有价值的一篇**交底书；用户明确要多篇时先列大纲（标题+核心区别）再约定撰写顺序。选定依据：创新性、可授权性、与查新结果的差异化、方案完整可实施、保护范围合理。

#### Step 5：联网检索查新（prior_art_search.md）

生成交底书全文之前或过程中必须执行；结论写入第一章 1.1 现有技术及区别论述。

- **优先渠道 A（本地版）：国知局公布公告站公开检索**（仅 `epub.cnipa.gov.cn`），用通用网页检索/只读抓取公开结果页，不运行任何 Playwright 脚本。**检索词拆分责任在 Agent**：从技术方案归纳 2–8 个有检索意义的语义块（专业术语、名词短语、名动组合），逐块分次查询；禁止把无空格整句长中文当唯一查询（极易 0 命中）；整句 AND 检索在本地版改为多个词块结果的交集比对，不启用 `cnipa_epub_crawler.py`。结果按公开号主键去重合并，提取标题、公开号、链接、摘要。
- **`abstract` 必用规则**：某条摘要非空时，对该专利的方案概括、应用场景与局限分析必须先完整理解摘要后消化重写，禁止仅凭标题臆造或与摘要矛盾；正文不得大段逐字粘贴官方摘要；无摘要条目须注明并用详情页或 Google Patents 公开页补全理解。
- **降级渠道 B**：公开查询失败、超时或结果为空/明显无关时，降级通用网页检索（中文关键词、可加 `中国`/`专利`/`CN` 语境词）与 Google Patents（用稳定著录页 URL，如 `https://patents.google.com/patent/CNXXXXXXXXXA/en`）。**所有渠道都查不到 → 在 1.1 如实标注"公开渠道未检索到/检索受限"，不编造、不转自动化浏览器方案。**
- 每条现有技术必须附至少一个可公开访问、与著录项一致的 URL，写入前打开验证；禁止编造 URL。
- **1.1 检索说明写法**：只写公开数据库名称与检索词；禁止在 1.1 正文出现脚本名、Playwright、WebSearch、Agent、技能仓库名等内部流程用语。

#### Step 6：生成前摘要预览（disclosure_preview.md）

先展示结构化摘要请用户确认方向：选定专利点名称（工作标题）、解决的技术问题（1–3 条）、核心创新模块或步骤（3–6 条）、与最相近现有技术的区别（1 段）。用户明确要求跳过时可直接进入 Step 7。

#### Step 7：交底书全文生成（disclosure_builder.md + template_reference.md）

固定章节结构：注意事项 → 一、现有技术（1.1 按技术方向分类含检索结果，每条带经核验公开源 URL；1.2 缺点）→ 二、所要解决的技术问题 → 三、技术方案详细阐述（3.1 背景；3.2 系统框图 fenced mermaid；3.3 模块功能说明，聚焦作用与关联关系而非输入输出；3.4 系统流程说明 mermaid 流程图；3.4.1 符号与公式，出现公式时须设且先符号定义再公式；3.5 关键技术参数，"符号"列与 3.4.1 同形）→ 四、与现有技术相比的优点 → 五、技术关键点和欲保护点 → 六、其它（实施例、技术效果、参数示例）。

关键体例（§7.3/§7.5/§7.7）：

- **文件命名（必遵）**：主文件名 = 规范化的案件名称（去占位、去 `\ / : * ? " < > |` 非法字符、≤80 字超长截断）+ `_` + 14 位时间戳（`YYYYMMDDHHmmss`，取当次落盘本地时间）；首次定稿与迭代同规则；不覆盖已有交付文件（用户明确指定除外）；`mermaid_render.py` 的 `-o` 与 `--docx` 用同一主名。
- **图**：3.2/3.4 均只用 fenced mermaid（本地 `mmdc` 渲染），本机有渲染器时交付前转 PNG，不要 ASCII 文字框图；节点标签用简短中文/数字，避免 ①②③。**本地版无 `mmdc` 时 PNG 与含图 DOCX 降级，Markdown 定稿照常交付（mermaid 围栏保留），并在回复中说明。**
- **脱敏**：业务/行业抽象为通用描述、具体分类用"分类A/B/C"、具体数值用范围或"一定规模"、公司/产品名删除或"某系统"。
- **公式体例（必遵）**：先符号表（含符号、含义、下标含义、量纲/取值范围，可按任务侧/节点侧分组）再公式；资源维度标签一律下标 + `\mathrm{}`（如 `b_{i,\mathrm{cpu}}`），禁止 `^{cpu}` 类上标维度；上标仅用于幂次/转置/序号；同一字母不得多义；行内 `\(...\)` 或 `$...$` 二选一、块级 `\[...\]` 或 `$$...$$` 二选一，全文不混用；比较符优先 `\leq`/`\geq`；块级公式尽量单行，编号用 `\tag{1}` 或"式 (1)"全文择一；逻辑连接词写在公式外中文里，避免 `\text{且}`；3.4.1 符号表、正文首次定义式、3.5"符号"列、第六章实施例四处逐字同形。
- **定稿交付（必做）**：同时交付 Markdown（保留 mermaid 围栏源码 + `<!-- ![图示](…png) -->` 注释）与 Word，用 `python3 scripts/disclosure/mermaid_render.py -i 草稿.md -o "案件名_时间戳.md"` 一条命令（默认同目录生成同名 `.docx`，`--no-docx` 可跳过）。**本地版例外：本机缺 mermaid 渲染器时，改用 md_to_docx 出不含渲染图的 Word 或仅交付 Markdown，并明确告知图未光栅化；不因此阻塞交付。**
- **§7.6 交付回复交互（必做）**：每次交付定稿时在同一条对话回复中追加"权利要求偏向点"引导——用户可用一句话说明侧重点，Agent 读 `iteration_context.md` 后按 `merger.md` 以当前稿为基准另存新时间戳版本；对举的两类侧重必须能从当前定稿与已用材料推出，禁止为凑交互编造未涉及的场景/模块/行业词；该引导不得写入交底书正文。

#### Step 8：生成后自检（disclosure_self_check.md，内部执行，结果不写入正文）

- 8.1 逻辑闭环：方案是否形成"发现→处理→扩展/输出"闭环、模块关联与上下游衔接、分支样本处理路径、依赖项来源说明、第五章与第三章呼应。
- 8.2 公式与参数一致性（含公式时必做）：符号表完整性、维度下标体例、字母多义、公式表述同形同义、LaTeX 体例统一、3.5 与符号表逐字同形、阈值范围、参数命名、实施例数值对应；还须**主动复核公式正确性与逻辑**（式面笔误、不等式方向与文字一致、公式与 3.4 流程/3.3 模块可互推、边界情形如空队列/分母为零、量纲取值一致），发现问题直接改稿并联动修订相关章节，不得只报告不改。
- 8.3 格式与引用：迭代时是否含"## 合并摘要（留档）"/"## 纠正摘要（留档）"、"交底书修订对话记录.md"是否追加、交付文件名是否带时间戳、图均为 mermaid 且已转 PNG、章节引用真实存在、1.1 URL 可访问且检索说明无内部用语、国知局 `abstract` 条目概括体现摘要理解、文末无技能仓库/教学示例等元脚注、交付回复是否已附 §7.6 交互。

#### 迭代模式（disclosure-iteration：merger.md / correction_handler.md / iteration_context.md）

先读 `iteration_context.md` 选模板：补充材料/扩展方案/第五章权利要求书式强化 → `merger.md`；指出错误、事实参数不符、风格或保护点调整 → `correction_handler.md`。共同门禁：读迭代上下文 + 读当前定稿 → 在稿内完成合并/纠正（merger 非破坏性追加或局部重写、correction 按事实技术/符号公式/查新区别/保护点四类落地）→ 增量改变技术实质时联动补充查新 → 跑自检 8.2/8.3 → 写入**新时间戳** `{案件名}_{YYYYMMDDHHmmss}.md` 并生成同名 `.docx`（禁止覆盖旧稿）→ 用 `scripts/disclosure/iteration_dialog_log.py --kind merge|correct`（或手工）在案件目录追加 `交底书修订对话记录.md`（含本地时间+UTC、类型、用户说明摘要、本轮交付文件名、摘要摘录）→ 回复中必须输出固定标题小节"## 合并摘要（留档）"（3–6 句：改了哪些章节、原因、是否影响保护点或检索结论、是否已做 8.2/8.3 核对）或"## 纠正摘要（留档）"（2–5 句：修改位置、依据、是否影响保护点或检索）；作为定稿交付时还须附 §7.6 权利要求偏向点交互（可缩写）。禁止在已判定为迭代意图时跑去重做 Step 3–4 全文专利点挖掘而不落盘新稿（用户明确要求重做除外）。

## 四、输出契约与质量检查

### 正式申请包（output-contract.md）

full-draft 必须产出 9 类文件：`权利要求书.docx`、`说明书.docx`、`说明书摘要.docx`、`摘要附图.docx`、`完整审阅稿.docx`、`结构化草稿.json`、`权利要求检查.txt`、`草稿验证报告.txt`，以及每幅生成附图的 SVG + PNG。结构化草稿（draft.json，schema 见 `references/draft-schema.md`，`schema_version: "2.0"`）是唯一事实源，DOCX 为渲染产物。

**必备可追溯性**：每个实质权利要求特征映射到至少一个 source ID；每个有源支撑的核心公式有处置记录；每个正式术语用术语台账的规范中文形；每个编号权利要求步骤映射到一个主流程图节点和一段实施例解释；每幅方法图有源支撑或明确标注为对已披露操作的重绘。

**质量阈值（1–5 分制，各附一句证据）**：evidence support ≥4；claim architecture ≥4；terminology and dependency consistency ≥4；enablement detail ≥3；technical-effect reasoning ≥3；有核心公式时 formula coverage ≥4；需要附图时 figure alignment ≥4。任一验证 ERROR、任一未映射的实质权利要求特征、或缺失核心公式 → 强制标记 `incomplete draft`。

**验证脚本用法**：

```bash
python scripts/validate_patent_draft.py draft.json
python scripts/build_patent_package.py draft.json --output-dir outputs --prefix patent
```

需解决全部 ERROR、逐条对照源材料复核 WARNING。交付说明须写明"需发明人确认并经合格中国专利专业人员复核"，不得因自动检查通过就宣称可直接提交。

### 技术交底书

产出：`outputs/{案件标识}/` 下的 `{案件名}_{YYYYMMDDHHmmss}.md` + 同名 `.docx`（含 PNG 化的 mermaid 系统框图/流程图、渲染后的公式），迭代案件另加 `交底书修订对话记录.md`。质量检查即 Step 8 自检（内部执行，不写入正文、不单独输出报告除非用户索要）；mermaid/公式/URL/章节一致性问题须在交付前解决。

## 五、关键规则与红线

1. 每个实质权利要求特征必须映射到一个或多个 source ID；support state 只有 `explicit`/`inherent`/`needs-confirmation`/`unsupported` 四种；`unsupported` 特征一律排除出正式权利要求。
2. 绝不推断发明人身份、权属、未公开实现细节、发表日期、现有技术结论或法律充分性；缺事实用 `[TO CONFIRM: 具体问题]`（仅限正式权利要求之外）。
3. 不把论文当权利要求语言；论文宽泛的目的陈述本身不构成对每种实现方式的支撑；区分"实验证明的结果""可溯因的技术效果""愿景/未来方向"三类，第三类不入权利要求。
4. 权利要求范围不得宽于披露所能支撑；必要特征与示例、参数、数据集、优选实现分离；不用"智能模块""新型网络"等标签，要写清模块接收什么、执行什么、产出什么。
5. 正式申请文本保持干净：source ID、support 标签、起草笔记、质量分数只进结构化草稿与审阅附录，不进权利要求/说明书等正式章节。
6. 正式公式渲染为原生可编辑 Office Math，不以裸 LaTeX 字符串作为可见公式；每个符号有定义，每条公式关联方法步骤与技术效果。
7. 主流程图末节点必须是具体领域输出（检测结果/目标位姿/状态估计/控制指令等），禁用"技术结果/处理结果/最终结果"（`render_flowchart_svg.py` 与 `validate_patent_draft.py` 均内置该正则硬检查）；同一主图复用为摘要附图与说明书附图，不生成措辞不一的两个版本。
8. 不机械复制语料模式与权利要求形状；论文-专利对照语料只用于学习转换模式，不得把专利独有细节当作论文支撑、不得凭 PDF 断定新颖性/优先权/权属。
9. 交底书红线：Office 文档必须先转 Markdown 再读；国知局检索词必须拆成 2–8 个语义块分次查询（本地版只用公开网页只读检索，不跑 Playwright）；`abstract` 非空条目必须基于摘要理解后重写；每条现有技术附经核验 URL；交付文件名必须带 14 位时间戳且不覆盖旧稿；自检清单、source ID、技能元信息、仓库脚注一律不得写入交付正文；迭代交付必须维护修订对话记录并输出"合并/纠正摘要（留档）"小节。
10. 产出是起草辅助：声明需发明人与专利代理师复核；不做新颖性、创造性、FTO 或可申请保证。
11. **本地版零外发红线**：不向国知局/代理机构/任何平台提交或上传文件；不运行 Playwright 脚本、不安装/驱动浏览器（含不为 mermaid 渲染自动下载 chrome-headless-shell）；查新只用公开网页只读查询，查不到如实标注。

## 六、配套资源

### references（根级 5 件）

- `references/cn-patent-drafting-guide.md`：中文发明专利用证纪律、论文结构→专利去向映射表、独权/从权/说明书各节起草规则、最终审计清单与 5 维质量评分量表（含交付阈值）。
- `references/patent-figure-guide.md`：方法流程图与摘要附图的画法规范（黑白、S 编号、末节点具体结果）、10 条一致性规则与算法发明建议图集。
- `references/draft-schema.md`：结构化草稿 JSON 的完整 schema（`schema_version: "2.0"`，source_map/terminology_ledger/formula_inventory/figure_inventory/evidence_ledger/claims/claim_feature_map/figures/specification/quality_assessment 等字段）与填充规则。
- `references/corpus-derived-patterns.md`：从五对论文-专利样例（机器人抓取、电池 SOH、工业故障检测、表面缺陷检测、遥感生物量）归纳的学术贡献→专利处理转换表与权利要求分层模式、问题-方案模式与起草经验。
- `references/corpus-pair-audit.md`：五对语料的匹配强度评估表、日期注意事项与论文-专利双向特征对照程序及安全用法/禁用清单。

### references/disclosure（交底书链 12 件）

- `intake.md`：Step 1 三问收敛案件边界。
- `project_scan.md`：Step 2 四级优先级扫描与 Office 文档先转换再读的硬性规则。
- `patent_points_analyzer.md`：Step 3–4 候选专利点挖掘、融合与默认只产出最优一篇的输出策略。
- `prior_art_search.md`：Step 5 国知局公布站优先的查新流程、检索词拆分纪律、`abstract` 必用规则、URL 规范与 1.1 检索说明写法。
- `disclosure_preview.md`：Step 6 全文生成前的结构化摘要预览与确认。
- `disclosure_builder.md`：Step 7 交底书章节结构、命名/时间戳规则、脱敏表、公式体例 §7.7、md+docx 双交付与 §7.6 权利要求偏向点交互。
- `template_reference.md`：脱敏版交底书章节范例、mermaid 系统框图模版、符号表示例与公式正/反例对照表。
- `disclosure_self_check.md`：Step 8 内部自检（8.1 逻辑闭环 / 8.2 公式与参数一致性 / 8.3 格式与引用）。
- `iteration_context.md`：迭代入口路由、新时间戳落盘规则与"交底书修订对话记录.md"维护要求。
- `merger.md`：增量合并迭代流程与"## 合并摘要（留档）"强制输出。
- `correction_handler.md`：对话纠正迭代流程（四类纠正分类）与"## 纠正摘要（留档）"强制输出。
- `tooling.md`：disclosure 脚本群的依赖安装、用法、降级行为与 Windows/PowerShell 编码注意事项。

### scripts（一句话功能，据 docstring 与 tooling.md）

- `scripts/extract_pdf_text.py`：从单个 PDF 或目录批量抽取可检索文本，保留 `===== PAGE N =====` 页标记。
- `scripts/init_patent_project.py`：创建 paper/supplementary/source-figures/existing-patent/work/outputs 等目录的 paper-to-patent 项目工作区（可选复制技能包与论文）。
- `scripts/audit_claims.py`：对中文权利要求做确定性结构检查（编号、引用基础、`所述` 术语、占位符）。
- `scripts/validate_patent_draft.py`：验证结构化草稿的可追溯性、完整性与质量门（内置五维阈值与"技术结果"类模糊末节点正则）。
- `scripts/build_patent_package.py`：由 draft.json 构建标准分件中文专利申请包（强制校验权利要求/说明书/摘要/附图与公式 latex 字段）。
- `scripts/render_patent_docx.py`：把结构化草稿 JSON 渲染为 DOCX（宋体版式、调用 OMML 公式转换）。
- `scripts/render_flowchart_svg.py`：从草稿 JSON 渲染专利风格黑白流程图 SVG，并校验 claim_step 覆盖与末节点具体性。
- `scripts/math_to_omml.py`：把 LaTeX 公式转换为 Word 原生可编辑 Office Math（OMML）。
- ~~`scripts/disclosure/cnipa_epub_search.py`~~ — **本地版停用**：原经 Playwright 过 WAF 的国知局公布站检索+解析一步脚本；本地版改用通用网页只读检索。
- ~~`scripts/disclosure/cnipa_epub_crawler.py`~~ — **本地版停用**：Playwright 抓取公布站结果页；本地版不驱动浏览器。
- `scripts/disclosure/cnipa_epub_parse.py` — 解析结果页 HTML 的纯本地解析器；仅在通过公开只读查询自行取得页面 HTML 时可参考使用，不主动抓取。
- `scripts/disclosure/docx_to_md.py`：Word(.docx)→Markdown 并抽取内嵌图片到 `_media/`（mammoth）。
- `scripts/disclosure/pptx_to_md.py`：PowerPoint(.pptx/.ppsx) 按页导出 Markdown 并抽取嵌入位图、附演讲者备注（python-pptx）。
- `scripts/disclosure/mermaid_render.py`：把 mermaid 围栏（及默认先跑 LaTeX 公式）渲染为 PNG，写出带注释图引用的定稿 .md 并默认生成同名 Word。**本地版仅当本机已有 Node.js + `mmdc` 时运行；不执行 `npx puppeteer browsers install chrome-headless-shell` 等浏览器下载，缺失时交付保留 mermaid 围栏源码的 Markdown。**
- `scripts/disclosure/math_render.py`：LaTeX 公式经 matplotlib mathtext 渲染为 PNG、保留原文、图片引用写入 HTML 注释。
- `scripts/disclosure/md_to_docx.py`：Markdown→Word（标题 1–9 映射、宋体 10.5pt 正文、图片等比缩放嵌入、公式图约 0.17 英寸高）。
- `scripts/disclosure/iteration_dialog_log.py`：在案件目录追加"交底书修订对话记录.md"一条（本地+UTC 时间、用户摘要、交付文件名、合并/纠正摘要）。

### agents / evals / 其它

- `agents/openai.yaml`：display_name "Nature Paper to Patent"，default_prompt 引导把研究材料转为 source-grounded 专利草稿。
- `evals/evals.json`：3 条评测——①含模型结构/损失函数/检测流程的 PDF 生成完整中国专利包（公式须 Word 原生可编辑、特征可回溯页码）；②扫描版 PDF 的 OCR 感知权利要求草案（不猜不确定符号、列发明人问题）；③论文与已有专利双向特征对照（用声明的关系标签、双侧定位、不假定 patent-only 有论文支撑）。
- `README.md` / `README_EN.md`：中英文使用说明（适用场景、典型请求、所需输入、产出、依赖与边界）；`requirements.txt` 承载通用依赖（python-docx、mammoth、python-pptx、matplotlib 等，均为本地 Python 包）；原 `scripts/disclosure/requirements-cnipa.txt`（Playwright 依赖）**本地版不安装**；mermaid 渲染仅在本机已有 Node.js + `mmdc` 时使用，本地版**不执行** `npx puppeteer browsers install chrome-headless-shell`，缺渲染器时按上文降级交付。
