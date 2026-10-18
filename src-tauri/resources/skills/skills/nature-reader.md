# nature-reader

## 一、定位与触发场景

nature-reader 把一篇论文（PDF、DOI、arXiv 链接、出版社 HTML 或粘贴文本）转换为**可回溯的全文中英对照 Markdown 阅读材料**：段落级原文/译文对照、图表裁剪并按首次实质引用位置插入、公式渲染为 Markdown 数学块并配稳定锚点、附 source map 供后续追问引用。目录：`skills/nature-reader/`。

SKILL.md frontmatter 中的 description 原文：

> "Create source-grounded Chinese-English paper readers with aligned text, figures, tables, and equations. Use for 全文翻译、中英文对照、论文精读 or source-linked questions about a paper; respect a requested excerpt or question without generating a full reader."

**典型触发请求**：用户要求 `全文翻译`、`中英文对照`、`原文对照`、`论文精读`，或直接调用 nature-reader 而未指定更窄交付物；也包括"根据 DOI 获取论文内容，生成带 source map 的阅读材料"这类来源解析型请求。

**明确不做什么（边界）**：

- 默认产出是完整双语 reader，**不得退化为**：纯中文摘要、无原文对照的评述、只有图注没有图表裁剪、脱离出处位置的知识点列表、只做摘要/引言/节选（principles.md "Non-negotiable defaults" 列出的五种禁止替代形态）。
- 反过来，用户明确只要摘要、节选或针对原文的问答时，**不得扩大为全文处理**——按所请求范围执行即可，SKILL.md 路由协议第 1 段专门区分了"建 reader"与"答问题/译节选"两种模式。
- 默认以 Markdown 为中心，不在交付物中加入交互式问答面板；`reader.html` 仅在用户明确要求浏览器预览时作为次级产物生成。
- 引用检索类任务属于专门的 citation 工作流，不在本技能范围。
- 版权注意：对版权受限的出版方 PDF，聊天回复保持简短并指向本地产物；完整双语正文只针对用户提供的文件或明确合法的开放获取内容。

## 二、运作模式（加载/路由机制）

### SKILL.md 路由协议

SKILL.md 是路由器，分五步：

1. **读 manifest.yaml 和 always_load 核心层**——核心层持有适用于一切阅读任务的原则、工作流和输出契约，外加共享 Terminology Ledger。
2. **检测 source_format**——按 manifest 的 `detect:` 提示和用户输入判定取值，并在处理前用一行短句告知用户检测结果以便低成本纠正。一个来源可映射多个值（如 DOI 解析出 PDF）：先加载解析 fragment，再加载解析后产物的 fragment。
3. **只加载匹配到的 fragment**——明确"Do **not** read every fragment in `static/`"。
4. **按优先级应用已加载材料**：① 核心原则 → ② 来源格式 fragment（决定文本/图/表如何提取）→ ③ 阅读工作流（六步 source-map-first 流程）→ ④ 输出契约（必需文件与交付前核对清单）。翻译过程中同步构建 Terminology Ledger，它最终成为 `paper.md` 的术语表和 `source_map.json` 的 glossary。受限无法完整处理时仍生成草稿 reader，在 `translation_notes.md` 标注缺失页/图/低置信裁剪，**不得切换为摘要模式**。
5. **references/ 按需加载**——不是默认项，按 manifest 的 `references.on_demand` 条件打开。

### manifest.yaml 结构（version 2.1.0）

- `always_load`：`../nature-shared/core/terminology-ledger.md`（共享层）+ `static/core/principles.md`、`static/core/workflow.md`、`static/core/output-contract.md`（技能本地核心）。
- `axes:` 单轴 `source_format`，`default: pdf-text`，`multi: true`（可组合加载）。五个取值 → fragment 映射：
  - `pdf-text` → `static/fragments/source/pdf-text.md`（可选中文字的 PDF，默认）
  - `scanned-pdf` → `static/fragments/source/scanned-pdf.md`（纯图像或需 OCR 的 PDF）
  - `html` → `static/fragments/source/html.md`（出版方或预印本 HTML 页）
  - `doi-arxiv` → `static/fragments/source/doi-arxiv.md`（需先解析的裸 DOI/arXiv 链接）
  - `pasted-text` → `static/fragments/source/pasted-text.md`（无原始版面可检索的粘贴文本）
- `references.on_demand:` 五条条件→路径映射：
  - 图表裁剪/就近放置/紧凑裁剪规则 → `references/figure-extraction.md`
  - `paper.md`/`source_map.json` 精确字段 schema → `references/output-spec.md`
  - 公式、数学式、化学式、自定义 LaTeX 宏、纯图片公式 → `references/equation-handling.md`
  - 带来源引用的追问回答 → `references/grounding-rules.md`
  - 给每节标注论证功能（gap/contribution/result/limits）作为阅读辅助 → `references/article-anatomy.md`

### 静态层与动态判断分工

静态层（always_load + 选中的 source fragment）决定"怎么提取、怎么译、产出什么"；动态判断只在运行时做两件事——判定 source_format 取值、以及按任务需要决定打开哪些 references。来源格式判别是唯一的轴，不做期刊/语言等其它维度分片。

### 对 nature-shared 共享层的依赖

仅一处：`../nature-shared/core/terminology-ledger.md`（术语台账，翻译时同步构建，保证全文术语一致）。

## 三、核心工作流（实际功能流程）

`static/core/workflow.md` 定义六步。建 reader 走 1–5；来源追问直接走第 6 步，不重建产物。

### 1. 识别来源与论文类型

来源格式 fragment 决定提取方式；同时识别论文类型（发现/机制型、方法/算法型、资源/数据集型、会议论文、综述/观点）以把握文本-图-图注耦合的紧密程度。

### 2. 翻译前先建全文 source map

用户提供全文就处理全文档，不止步于摘要或代表性页面。为来源块建立稳定 ID：

- `S001, S002, ...` 正文文本；`C001, ...` 图注；`F001, ...` 图；`T001, ...` 表；`E001, ...` 独立公式。

每块记录：页码、块类型、原文、译文、阅读顺序索引、邻近图表引用、图/表首次实质提及位置、置信度（提取不确定时）。公式另需记录印刷公式编号（如有）、规范化 LaTeX、bbox、视觉兜底时的原图裁剪路径。source map 保持稳定以便后续追问引用同一 ID；长论文加页索引。

### 3. 保守翻译

逐块翻译所有可提取的实质内容，规则包括：技术术语保留原文除非有明显更好的标准中译；基因名/蛋白名/公式/模型名/符号保持原样；引用标记、上下标、数值不变；方法细节不得压成含糊散文；保持段落与章节顺序；OCR/版面提取弱时标注不确定而非猜测；不得把密段转为要点列表；**不得静默跳过 Methods、limitations、data availability、code availability、competing interests 或扩展图注**；论文过长时按页/节增量写 `paper.md` 并标记待办块，不得切换摘要模式。

行内式用 `$...$`、独立公式用 `$$...$$`，禁止放普通代码段或代码围栏。低置信 OCR、渲染器不支持的自定义宏、纯图片公式走 `references/equation-handling.md`：显示原图裁剪 + 显式标注的低置信转写。

翻译中同步构建 Terminology Ledger（`../nature-shared/core/terminology-ledger.md`）。

### 4. 提取图表并放到相关论述附近

每个图/表裁剪进 `assets/`，置于首次实质提及附近；图注与图/表保持相连，含英文图注原文与中文图注。完整放置与紧凑裁剪规则及图/表块格式见 `references/figure-extraction.md`。

### 5. 生成 Markdown 文件

默认输出单个全文 `paper.md`，必须包含：元数据头；简短页/节索引；长论文的页级或节级分隔；全部可提取实质文本的段落级原文/中文对照；就近放置的图表块；带 `E...` 锚点和可见来源位置的公式块；**三个及以上独立公式时附紧凑公式索引**；每个实质文本/图/图注/表块上的来源锚点；术语表（来自 Terminology Ledger）；仅在双语正文之后附短小的 `阅读提示 / critical reading notes`，不得以此替代正文；提取弱时附简短不确定性说明。不在 Markdown 交付物中加交互问答面板。

交付前运行 `scripts/validate_reader_math.py paper.md --source-map source_map.json`，解决全部失败项；准备可复用/发布产物时加 `--strict`。

### 6. 带来源 grounding 的追问回答

从相关论文证据回答；已有 reader 时复用其精确块 ID 与页码，没有 reader 时引用已验证的来源位置而不编造 ID。完整规则见 `references/grounding-rules.md`。

### 五个来源 fragment 的差异化提取规则

- **pdf-text**：先加载 `pdf` 技能；直接抽取文本层、不对可选中文字做 OCR；处理全文档；多栏版面恢复自然阅读顺序；保留连字、断词、上下标和数学；独立公式须对照渲染页验证后再写 LaTeX（纯文本抽取常破坏分式/矩阵/对齐）；图表是页内嵌图，按 figure-extraction 裁剪，不得把表格所在页文字当作表格；部分页有文本层、部分为扫描页时，扫描页按 scanned-pdf 规则处理并标注置信度。
- **scanned-pdf**：逐页 OCR，不假设存在可用文本层；每块记录置信度，低置信块显式写进 `translation_notes.md` 而非猜测；OCR 确信处保留原文措辞，乱码处标记而非静默"纠正"；数字/单位/符号/基因蛋白名/化学式是 OCR 高危处，须上下文交叉核对；每个独立公式按 image-first 处理——先裁剪原图再尝试数学 OCR，OCR 所得 LaTeX 只作显式低置信便利层；**绝不根据上下文散文静默修复或猜测扫描公式**；页面歪斜/旋转/裁切时注明受影响页、只译可辨识部分。
- **html**：只取正文，剥离导航、cookie 横幅、相关文章栏、文献管理插件、广告；保持文章标记中的章节结构与段落顺序；图通常是独立元素，逐图抓图与图注；表格在标记干净时忠实重建 HTML 表而非截图；保留行内数学（MathML/LaTeX/图片）、上标引用标记和文献列表链接，优先用可访问的 MathML 或出版方 LaTeX 并规范化为 `$...$`/`$$...$$`；付费墙/保留所有权利页面聊天输出从简；JS 渲染缺失内容时注明未能获取而非编造。
- **doi-arxiv**：先解析标识符——arXiv → 摘要页 → PDF（有 HTML/LaTeX 源时一并取）；DOI → 出版方落地页 → 合法可得的 OA PDF 或 HTML。解析后即转为 pdf-text/scanned-pdf/html 任务，加载对应 fragment，本 fragment 只管获取。抓取书目元数据（标题、作者、期刊、年份、DOI/arXiv id）供 `paper.md` 元数据头。版本记录付费时优先 OA 版本（arXiv、作者稿、PMC），并在 `translation_notes.md` 注明所读版本（arXiv 与正式版可能不同）。有合法 arXiv HTML/TeX 源时公式优先取源码，但仍须对照渲染论文核对编号与符号。标识符无法解析或只拿到摘要时，用已有内容建草稿 reader 并明确标注未获取部分，**不得编造正文**。
- **pasted-text**：粘贴文本即事实来源；页码不可知，用顺序块 ID（S001...），粘贴中有章节标题时用节级锚点替代页锚点，并在 `translation_notes.md` 注明页锚点不可用；通常无图表图像，**不得发明裁剪**——文中提到图/表时保留引用与图注为文本块并注明视觉资产未提供；粘贴明显不完整时只对所给部分建 reader 并标草稿模式，**不得凭记忆补全缺失章节**；保留粘贴中的引用标记、公式与符号原样，保留已有合法定界符，将明白无误的裸 LaTeX 规范化为 `$...$`/`$$...$$`，含混或不完整表达式标注而非猜测；不得因缺版面元数据就把粘贴散文压成摘要。

## 四、输出契约与质量检查

**必产出文件**（`static/core/output-contract.md`）：`paper.md`（全文 Markdown 产物）、`source_map.json`（稳定来源锚点）、`translation_notes.md`（术语、不确定性、版面说明）、`assets/`（图、表、公式裁剪）；`reader.html` 仅在用户明确要浏览器预览时产出。来源不完整时把产物标注为草稿模式，**不隐藏缺失信息**。

**交付前核对清单**（逐条）：`paper.md` 含 `**Original:**` / `**中文:**` 块对；文中引用的每个图/表链接在 `assets/` 下真实存在；`assets/` 中每个图/表在文中有对应卡片块和来源指针；独立公式在 `$$...$$`（或 `math` 围栏）内、行内公式在 `$...$` 内；数学内容跨语言不变——只译散文、每个独立公式只出现一次、中文绝不出现 `(I_0)` 式伪数学；普通散文中不得出现 `\frac`、`\sum`、`\begin{...}` 等裸 LaTeX 命令；每个独立公式有 `E...` 锚点且在 `source_map.json` 有匹配条目；每个低置信/纯图片公式指向 `assets/equations/` 下存在文件；`source_map.json` 可解析为 JSON 且含来源块 ID；`translation_notes.md` 记录了跳过/不确定/草稿内容。

**验证脚本**：`python scripts/validate_reader_math.py paper.md --source-map source_map.json`（发布级加 `--strict`），检查定界符、裸 LaTeX、公式 ID、source-map 关联、公式兜底路径。

**工具配合**：PDF 输入先加载 `pdf` 技能获取提取/OCR 指导；浏览器富预览只作为 Markdown 工作流之上的展示层。

## 五、关键规则与红线

1. **Translate for meaning, not for style**：保留结构、证据、对冲措辞、术语、公式、单位、引用标记；输出保持散文段落形态，除非源本身是表格/列表；不得压成关键词要点。
2. 公式是一等来源块：每个独立公式配 `E001, E002, ...` 锚点；无法确信转写时裁剪原图入 `assets/equations/` 并在 `paper.md` 显示图片，best-effort LaTeX 必须标低置信；**绝不猜缺失符号**，绝不让 `\frac`、`\sum` 裸露在数学定界符之外。
3. 每个实质来源块都要有稳定锚点和可见双语对（`<a id="S001"></a>` + `**Source:** p.1 S001` + `**Original:**` + `**中文:**` 四行结构）。
4. **Grounding 红线**（grounding-rules.md）：追问引用精确页码与块 ID，没有 reader 时引用已验证位置；论文不支持该说法时直说 `原文未明确说明`；坏模式包括——无来源 ID 的含糊转述、仅靠标题摘要回答需正文支撑的问题、声称图支持却不引图/图注、OCR 不确定处编造细节。追问时若某整段因版权或提取失败无法收录，保留块锚点并在 `translation_notes.md` 说明限制。
5. **公式不翻译**（equation-handling.md）：公式、符号、下标、算子、希腊字母、单位原样保留，只译引入/解释性散文；独立公式只发一份共享 `E...` 块，中文侧不重复不重写；禁用 `(I_0)`、`(E_0=...)`、`(tau)`、`(Delta T)`、翻译变量名、改变源记号的 Unicode 近似；印刷公式编号放定界符外，源无编号就省略 `Eq. (...)`，**绝不发明公式编号**。
6. **置信阶梯**：高置信（出版方 MathML/LaTeX 或对照渲染页验证过的 TeX）→ 直接渲染；中置信（可选中 PDF 重建 + 目视核对）→ 渲染并记 `confidence: "medium"`；低置信/纯图片 → 裁剪原图先入 `assets/equations/E001.png`，best-effort LaTeX 仅在有用时附并标 `低置信度转写 / Low-confidence transcription`；无法转写时保留裁剪并写 `LaTeX transcription unavailable`，不输出编造数学。
7. **图表裁剪红线**（figure-extraction.md）：只裁内容区，用完全包含视觉对象的最小矩形；排除页眉页脚周边散文；不确定的裁剪框标 approximate 而非放大；不用整页截图替代（除非无法更紧凑，且须标注）；后续章节再提同一图时链接回已插入块而非重复。
8. **article-anatomy 的使用边界**：论证功能标签（gap/contribution/result/limits）只是导航辅助，**不是摘要许可证**——逐段对照翻译契约仍然成立，不得退化为 summary-only。
9. 版权：版权受限来源的聊天回复从简并指向本地产物；合法 OA 内容才做完整双语收录。

## 六、配套资源

**references/（5 个）**：

- `output-spec.md`：`paper.md`/`source_map.json`/`reader.html` 的精确字段 schema——source_map 的 paper/blocks/pages/figures/equations/glossary 六段 JSON 结构（块含 id/page/type/order/original_text/translation/bbox/confidence/refs/insert_after）、布局规则、图/表卡片格式（`Placed near` + `Source` + 图 + 双图注 + `Reading note`）、文内引用短格式（`p.7 S021`、`p.4 E001` 等）。
- `grounding-rules.md`：追问回答五步流程、好/坏回答范式、翻译规则、图表引用规则。
- `figure-extraction.md`：图表裁剪与放置——语义就近优先于像素复刻、首次实质提及处放置、紧凑裁剪规则、图/表块 Markdown 模板。
- `equation-handling.md`：公式处理全规则——可移植 Markdown 数学语法、双语公式保持、独立公式块模板、置信阶梯与视觉兜底、公式索引格式、source-map 公式条目契约、KaTeX/MathJax 兼容兜底（读者报"看到裸 LaTeX"时加产 `reader.html`，不静默替换 `paper.md`）。
- `article-anatomy.md`：基于 2025 年 20 篇 Nature Communications CS/AI 开放获取论文提炼的阅读辅助——摘要五段漏斗（field value → gap，几乎总在 However 后 → `Here we show` 铰链 → 量化结果 → 意义）、引言 hook→gap→contribution 及空白信号词、结果段结论先行、图注自足性（图注末句给出推断时要标"解读"而非"描述"）、讨论五段式、五种体裁识别（research/review/perspective/comment/benchmark）及对应中文阅读提示。

**scripts/**：`validate_reader_math.py`——校验 reader 产物的公式渲染与可追溯性（docstring："Validate equation rendering and traceability in a nature-reader bundle."），内部用正则检查 `E\d{3,}` 公式 ID、锚点、通用代码围栏、`\frac`/`\sum`/`\begin` 等裸 LaTeX 命令。

**evals/**：`evals.json` 含 6 条评测用例，代表性 prompt 主题：① 全文中英对照而非摘要/前几页、图表裁剪就近插入（带真实 PDF 附件 s41586-026-10452-4.pdf）；② 只有 DOI 时建全文或草稿模式 reader 并明确区分已确认/缺失内容，正文块必须保留 Original/中文 格式、不能只给摘要页；③ 扫描件 OCR + 低置信标注；④ 大量公式 PDF 的公式渲染（E 编号、页码、原公式编号、source_map 记录、三式以上加索引）；⑤ 扫描公式不猜测、原图裁剪 + 低置信转写标注；⑥ 双语对照中数学本体不翻译、中文不出现 `(I_0)` 式伪数学、须通过严格校验。expected_output 均强调"段落级对照 + 稳定锚点 + 图表资产 + 来源可追溯"四项同时成立。

**agents/**：`openai.yaml`——OpenAI 侧展示配置（display_name "Nature Reader"，default_prompt 用 `$nature-reader` 构建中英对照、图感知的 reader）。

**README.md / README_EN.md**：中英使用说明，列出典型请求（"把这篇 PDF 做成中英对照全文 Markdown"等）、用户需提供物（PDF/DOI/arXiv/HTML/粘贴文本、输出目录、是否裁剪图片）、产出清单与相关技能（nature-downloader 负责合法获取全文、nature-paper2ppt 可把阅读材料转汇报 PPT、nature-citation 抽取需引用支撑的 claim）。
