# nature-paper2ppt

## 一、定位与触发场景

- **用途**：把科研论文、预印本、PDF、图注或阅读笔记转换为**中文、图文并茂的 PPTX 汇报包**，带原始论文图和 speaker notes，适用于组会、文献汇报、论文分享、会议报告与答辩准备。
- **description 原文摘录**（SKILL.md frontmatter）：

  > "Create or improve a Chinese academic PPTX from a scientific paper or research reading notes, with source figures and speaker notes. Use for 论文做PPT、文献汇报、组会PPT and paper-based conference or defense presentations."

- **典型触发请求**：
  - "把这篇 Nature Communications 论文做成 12 页中文组会 PPT。"
  - "根据摘要和图注先做一版文献汇报，不要太密。"
  - "把这篇机器学习论文做成方法、结果、局限都清楚的 PPT。"
- **明确不做什么（边界）**：
  - 只要求 outline 或讲解时**不**生成 deck（"A requested outline or explanation alone does not require creating a deck"）。
  - 不把论文内容改写成无法回溯来源的泛泛介绍；不虚构结果、数字或图细节。
  - 全文翻译或逐段阅读应走 `nature-reader`；机制图/方法图重画走 `nature-figure`；版式深加工走 `presentations`。
  - 编辑已有 deck 时不重跑论文 intake、不重建叙事，只改受影响页面及跨页引用。

## 二、运作模式（加载/路由机制）

### SKILL.md 作为路由

frontmatter 仅含 `name: nature-paper2ppt` 与上述 `description`。正文是 "Paper-to-PPTX — Router"，给出 5 步路由协议：

1. 读 `manifest.yaml`（声明 `paper_type` 轴及取值→文件映射）+ 读 `always_load` 全部文件。
2. 用 manifest 的 `detect:` 提示对论文分类，并**先用一句话告知用户检测到的 paper_type**，让用户低成本纠正。
3. 只读与检测到类型匹配的**一个** fragment，"Do **not** read every fragment in `static/`"。
4. 按优先级应用材料构建 deck：`core/principles.md` → `core/toolchain.md` → paper_type fragment → `core/workflow.md`（9 步）→ `core/output-and-quality.md`。
5. `references/` 仅在需要时按 manifest 的 `references.on_demand` 表打开；生成真实 PPTX 后须跑 `scripts/audit_pptx_quality.py`。

### manifest.yaml 结构

- **always_load**（5 项）：
  - `../nature-shared/core/terminology-ledger.md`（共享层：术语台账）
  - `static/core/principles.md`、`static/core/toolchain.md`、`static/core/workflow.md`、`static/core/output-and-quality.md`
- **axes**：仅一个轴 `paper_type`，`multi: false`，`default: discovery`。六个取值与映射文件：
  - `discovery` → `static/fragments/paper_type/discovery.md`（发现/机制类，question-to-evidence 弧线）
  - `methods` → `static/fragments/paper_type/methods.md`（方法/AI/工具/算法，problem-to-solution）
  - `resource` → `static/fragments/paper_type/resource.md`（资源/数据集/图谱/基准，workflow-to-validation）
  - `clinical` → `static/fragments/paper_type/clinical.md`（临床/人群/干预，design-to-inference）
  - `materials` → `static/fragments/paper_type/materials.md`（材料/化学/物理/工程，property-to-mechanism 或 design-to-performance）
  - `review` → `static/fragments/paper_type/review.md`（综述/观点/评论/Meta，evidence-map）
  - 交叉学科论文按"主导论证"而非领域标签选弧。
- **references.on_demand**（条件→路径）：
  - 设计/审计版式、视觉节奏、字体、anti-template、archetype、on-slide text budget → `references/design-and-layout.md`
  - 图表资产的选择/提取/裁剪/质检 → `references/figure-assets.md`
  - 自审-修订循环、严重度分级、程序化 PPTX/XML 检查、渲染预览策略、最终验证 → `references/self-review.md`
- **quality_tools**：`pptx_audit_script: scripts/audit_pptx_quality.py`，在首个 PPTX 草稿后及修订后使用，用于检测越界形状、文字过载、AI 模板用语、图片裁剪问题、过小证据图、近似错位对齐。

### 静态层与动态判断分工

- 静态 core 提供固定规则：原则、工具链、9 步工作流、输出与质量规则；
- 动态判断只有一件事——判断 `paper_type`，然后加载对应 fragment 的叙事弧线与默认页结构；
- references 为深参考，默认不加载，按需打开。

### 对 nature-shared 的依赖

- `../nature-shared/core/terminology-ledger.md`：构建 Terminology Ledger（术语台账），保证模型名、基因/蛋白名、数据集名、指标、缩写在所有 slide 与 speaker note 中完全一致。

## 三、核心工作流（实际功能流程）

来自 `static/core/workflow.md` 的 9 步，配合 principles/toolchain 规则执行：

**Step 1. 读取并提取源材料。** 提取：标题、作者、期刊/预印本服务器、年份、DOI；领域与子领域；论文类型；中心问题与知识缺口；主 claim；研究设计/流程/模型/数据集/实验体系；关键方法与对照；主要结果与定量发现；关键图表及图注；验证/稳健性/消融/敏感性分析；局限与未解问题；更广意义。采用**两遍阅读法**：先抓 metadata、abstract、headings、图注、表题，再只读支撑 slide 所需的结果与方法页。**不得虚构缺失数字、机制、数据集、图细节**；在此步开始建 Terminology Ledger。

**Step 2. 确认分类并选定叙事逻辑。** 路由已检测 `paper_type`，此处对照源文确认，按 fragment 的弧线（`claim-first`、`question-to-evidence`、`problem-to-solution`、`workflow-to-validation`、`evidence-map`）排页序。

**Step 3. 制定中文汇报计划。** 默认长度：15-20 分钟报告 12-16 页；快速/未指明请求优先 10-14 页；超过 16 页仅限详细研讨或论文确需。使用 fragment 的默认页结构但**不强行套模板**。写作前先规划视觉节奏：为每页指定 visual role 与 composition type，避免同一构图过度重复（细则打开 `references/design-and-layout.md`）。

**Step 4. 把图当证据而非装饰来选择。** 优先级：设计/流程图 → 主证据 → 验证/稳健性 → 机制/模型/综合 → 应用/概念意义。宁要少量可读的关键 panel，不要一堆 unreadable 整图。

**Step 5. 提取与准备图表资产。** 仅提取选中的图，裁剪密集 panel，保持原始数据图不被改动，存至 `output/assets/figures/`，在 `output/asset_manifest.md` 记录溯源信息。10-14 页 deck 通常选 4-8 个资产；数值明确时优先用可编辑的 PPT 原生表格/图表。插入前跑 figure-crop 自检：裁掉 panel 字母、坐标轴、图例、比例尺、方法标签、表头等属于 **high-severity 缺陷**，必须重新提取/扩大裁剪/拆成多页。

**Step 6. 逐页撰写内容。** 每页包括：中文标题（尽量结论式）、slide purpose、建议布局、2-4 条精炼中文要点、选用的图表资产、中文图注与解读、一句核心 takeaway、需要口头解释时的中文 speaker note。**每页只讲一个点**。禁止模板句式如"一句话总结""最有价值的后续方向""不是……而是……""提供了新的视角"等。遵守 on-slide text budget：普通页 2-3 条 bullet，每条 ≤ 约 18 个汉字（或 8-10 个英文词）；result 页 = 1 句短解读 + 至多 2 个 callout；解释放进 speaker notes。结果页顺序：hero evidence 先行，再窄解读栏，再最少标签。

**Step 7. 构建真实 PPTX。** 用 python-pptx（或用户提供的模板），默认 16:9；中文标题/要点/图注/备注；图页带 source label（如 `Source: Fig. 2b, Nature, 2024`）；文本框保守留边、不允许预期内溢出；排版统一；slide 几何跟随图片而非把图塞进固定 1:1 模板；标题左缘、图缘、图注带、底部条对齐到少量稳定 guides，避免仅差几个 point 的"近似错位"。**自动缩字（fit_text）仅作最后手段**，优先缩短文字、放大文本框或拆页。

**Step 8. 自审与纠偏循环。** 首稿后至少跑一轮显式自审：检查 PPTX 与资产，写出带严重度（`high`/`medium`/`low`）和页码的缺陷清单；修掉全部 high 和可修的 medium；重新生成；更新 `output/qa_report.md`。有 `.pptx` 时执行：

```bash
python skills/nature-paper2ppt/scripts/audit_pptx_quality.py \
  output/final_presentation_cn.pptx \
  --report output/pptx_audit.md \
  --fail-on high
```

把 high/medium 发现复制进 QA 报告。

**Step 9. 最终验证。** 重新打开 PPTX：核对页数、嵌入媒体数、speaker notes 是否存在、形状边界；检查裁剪资产的 contact sheet；确认审计脚本无未解决 high 发现。不允许停在"PPTX 能打开"——有 high 缺陷必须先修再验，剩余局限写入 `output/qa_report.md`。

### 各 paper_type fragment 的弧线与默认页结构（要点）

- **discovery**（13 页模板）：标题页→背景重要性→知识缺口→核心问题与假设→研究设计→关键证据 1-3（证据链）→验证/对照/稳健性→机制模型→创新与意义→局限→总结。若有强中心 claim 可用 claim-first 开场。
- **methods**（13 页）：瓶颈→核心问题→方法总览/架构图（通常 full-width 流程图）→关键模块→评测设置→主性能结果×2→消融/稳健性/失败案例→优势与边界→复用与开源→局限→总结。benchmark 数值明确时用原生表格/图表。
- **resource**（12 页）：资源需求→概览（规模/覆盖/特点）→样本/队列设计→生成与质控流程→主图谱/landscape（hero figure 独占页面）→关键发现×2→验证与可复现→访问/复用/许可→局限→总结。accession 号、URL、licence 必须准确。
- **clinical**（10 页）：临床问题→研究问题→设计（含 CONSORT 流程图）→终点与变量→主结果（生存曲线、森林图、校准曲线为 hero）→亚组/敏感性/次要分析→偏倚与局限→临床意义→总结。**效应量、CI、p 值必须与原文完全一致**，不得随意取整；不得超出研究设计做因果夸大。
- **materials**（11 页）：目标性能→设计原理→合成/制备/装置→表征→性能证据×2→机制/构效关系→稳定性/可扩展性/应用边界→局限→总结。表征 panel（XRD、SEM/TEM、光谱、性能曲线）要裁到关键 panel 而非缩小整图；单位、化学式、材料名精确一致。
- **review**（10 页）：主题为何现在重要→概念框架→主题 1-3（每主题一图）→争议/未决问题→作者综合→未来方向→总结。以概念框架图为脊柱；关键观点要归因于原始出处而非综述本身。

### 设计深参考（design-and-layout.md）核心规则

- 五种 composition type：`figure-dominant`（图占主导，文字退居边缘/底条）、`process-wide`（full-width 流程+小阶段标签）、`claim-led`（一句强主张+2-3 支撑碎片，不用假卡片）、`comparison`（表/图+单句结论）、`discussion`（开放式+少量尖锐 prompt）。禁止整副 deck 出自单一布局家族；slide-sorter 尺度扫查，若 ≥5 页同构则重设计。
- 证据层级：hero 图/主表 → 窄解读栏 → 最少标签 → 深度解释进 speaker notes。
- 布局自适应：按图的长宽比与角色选 full-width / 高图+窄栏 / 上下堆叠 / 70-30 或 75-25 非对称 / 紧凑 visual+callout；等权 1:1 是例外。
- 文本适配：宁可拆页也不靠缩字；已知长短语显式换行；绝不交付预期溢出/被裁的页。
- 对齐：标题左缘、图左右缘、图注/来源条、解读栏、底部条各守一条稳定 guide；"近似错位比有意不对称更糟"。
- 去模板化中文：禁用清单——`一句话总结`、`最有价值的后续方向`、`不是……而是……`、`不只是……更是……`、`值得注意的是`、`总的来说`、`从某种意义上`、`提供了新的视角`、`具有重要意义`、`未来可以进一步探索`。改写原则：句子必须点名本文的方法/数据集/机制/图证据/局限。
- 字体系统：三级层级（标题 24-32 pt / 正文 12-16 pt / 图注来源 7-9 pt）；一个中文无衬线家族 + 一个英文/数字伴随字体为上限。
- 图文协调：主导图约占页面 55-75%；图注贴图缘；1-3 个 metric callout，不用一圈等权盒子围住图。
- 页面饱满度：顶部锚点+主导中段+底部锚点（takeaway/来源条）；用证据支持元素填充，而非装饰。

### 图表资产深参考（figure-assets.md）核心规则

- 文件命名如 `fig1_workflow.png`、`fig2b_main_result.png`、`fig4ef_validation.png`。
- **Crop QA hard gate**（裁剪合格硬门槛）：panel 字母在引用处必须保留；坐标轴/轴题/刻度/colorbar/图例/比例尺/表头按阅读需要保留；方法/条件/分组/样本标签可见；裁剪四周留安全边距，不切着墨迹边界；置入后无需放大即可读；无 caption/callout 遮挡原始标签。
- 不合格修复五法：扩大裁剪框、跨两页拆图、用 full-width hero 页、数值明确时原生重建表/图、换更高分辨率源图。
- 结果记录进 `asset_manifest.md`，字段示例：`asset` / `source: Fig. 2b, p. 5` / `slide` / `method: pdf page render at 300 dpi, manual crop` / `crop_qa: pass` / `preserved` / `notes`。刻意部分裁剪须写明省略了什么、为何不影响本页论点。

### 工具链（toolchain.md）

- Python 优先跨平台栈：PyMuPDF（元数据/文本/页面渲染/页级裁剪）、Pillow（图裁剪/contact sheet/预览）、python-pptx（制作与安全编辑）、zipfile+python-pptx 重开做包体验证。
- 禁止硬编码 OS 字体路径；LibreOffice/soffice 仅在已安装且值得时可选；避免 Keynote/PowerPoint 桌面自动化、AppleScript、`open` 等平台依赖。
- 默认 fast path：抽元数据与图注 → 先定论文类型与候选图 → 图位置不清时才渲低分辨率 contact sheet → 仅对选中页渲高分辨率并裁剪 → 直接 python-pptx 建 deck → 自审修订 → 重开验包。OCR、全补充材料提取、全页高分辨率渲染、全页渲染 QA 都是 opt-in 例外。

## 四、输出契约与质量检查

**必须产出（最小完整包）**：

1. `output/final_presentation_cn.pptx` —— 主交付物：完整中文 deck，含图、图注、takeaway、来源标签、speaker notes。
2. `output/qa_report.md` —— 短 QA 报告：PPTX 创建状态、页数、插图数、缺失/占位图、裁剪 QA、对齐检查、去模板语言扫描、自审缺陷（按严重度分组）、修订内容、文字溢出检查、设计节奏审查、修订后验证方法、已知局限、后续人工项。
3. `output/assets/figures/` —— 实际用到的图资产。
4. `output/asset_manifest.md` —— 仅在提取了外部图表资产时生成；否则省略或在 qa_report 里写一行说明。

**可选文件**（默认不建，除非对评审/调试/溯源有用）：`output/ppt_outline_cn.md`、`output/figure_plan.md`、`output/ppt_script_cn_with_figures.md`（逐页脚本：Purpose/Layout/On-slide bullets/Figure-Table/中文图注/Core takeaway/Speaker note）、`output/rendered/`（仅有可靠 headless 渲染器时）。

**验证脚本**：`scripts/audit_pptx_quality.py`（纯标准库，解析 PPTX XML）检查——越界形状、可疑图片裁剪、主证据图过小、近似错位对齐、文字过载、AI 模板短语（一句话总结/最有价值的后续方向/不是…而是… 等正则）、页数/媒体数/备注数。`--fail-on high` 时 high 发现即判未就绪，须修复重跑，最终摘要复制进 qa_report。

**引用与归属规则**：标题页放论文标题/作者/期刊/年份/DOI；图页小字标 `Source: Fig. 2b, Nature, 2024`；改绘内容标"整理自"或"改绘自"；不得去除原图标签或改动科学数据。

## 五、关键规则与红线

1. **不得虚构**结果、方法、数字、数据集、图细节或未经支持的含义（出现于 principles、workflow、output 质量规则多处，反复强调）。
2. 图是证据不是装饰：裁剪导致科学上下文丢失是 **delivery blocker**（坐标轴/图例/panel 标签/比例尺/方法标签/表头被裁 = high severity）。
3. 文字溢出是交付阻断缺陷：不交付预期溢出、被框裁切或字体替换后可能溢出的页。
4. 结果页有图可用时不得做成纯文字页。
5. 默认简体中文输出；重要技术术语、缩写、基因/蛋白名、模型名、数据集名、公式、统计术语必要时保留英文，并经 Terminology Ledger 全程一致。
6. 临床类论文：效应量/CI/p 值严格照原文，不夸大因果。
7. 至少跑一轮自审+修订；有 PPTX 就跑审计脚本，high 发现必须清零。
8. 部分内容不可得时的 fallback：仍尽量产出可用结构、明确标注不确定页、占位符仅限必需图缺失、qa_report 写明无法验证的内容；PPTX 工具不可用时产出结构化 outline+figure plan，保证下游 builder 无需重读论文即可接手。
9. Lean operating mode：默认不做全图提取、全 OCR、存全文、装新依赖、启动 GUI 渲染预览、生成冗长 markdown 脚本——除非必要或用户要求。

## 六、配套资源

- **references/design-and-layout.md**：版式与排版大全——视觉节奏规划、on-slide 文字预算、证据层级、布局自适应、anti-template 规则、slide archetype 默认、标题写法、视觉密度、文本适配实现、对齐间距、中文学术表达与去模板、风格规则（Nature 风页面构成/字体系统/图文协调/饱满度/archetype 配方）。
- **references/figure-assets.md**：图表资产选择优先级、提取与命名规范、figure-crop 自检、Crop QA 硬门槛与五种修复手段、asset_manifest 记录格式。
- **references/self-review.md**：自审-修订循环、内容/视觉双维 checklist、high/medium/low 严重度定义、python-pptx 程序化检查项、渲染预览策略、Step 9 最终验证细则。
- **scripts/audit_pptx_quality.py**：仅用标准库的轻量 PPTX QA，从 XML 检测越界形状、文字过载、AI 模板用语、图片裁剪/过小图、对齐近似错位；用法 `python audit_pptx_quality.py <pptx> --report <md> --fail-on high`。
- **agents/openai.yaml**：display_name "Nature Paper to PPT"，default_prompt 引导把论文转成 evidence-grounded deck。
- **README.md / README_EN.md**：面向用户的入口说明（适用场景、典型请求、需提供材料、产出、边界、相关技能 nature-reader/nature-figure/presentations）。
- 本 skill 无 templates/、examples/、evals/ 目录。
