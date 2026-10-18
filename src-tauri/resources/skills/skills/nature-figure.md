# nature-figure（本地版）

> **本地化改造说明**：本版停用 OpenRouter / GPT Image 2 及一切第三方图像生成 API 路由（不设置 `OPENROUTER_API_KEY` / `ORCAROUTER_API_KEY`、不调用 `generate_openrouter_schematic.py`、不访问任何第三方图像端点）。"AI 示意图/graphical abstract"类需求只有两条本地出路：① 首选**手工可编辑 SVG 矢量示意图路由**（本地脚本 + 审计，完全离线）；② 若当前 agent 宿主自带内置图像能力，可用其生成**概念草稿**，并沿用原 skill 全部草稿纪律（不当证据、终版文字/箭头用矢量工具重画、provenance 留存），但不得配置或引入任何第三方服务。Python/R 数据绘图主流程完全本地，原样保留。

## 一、定位与触发场景

**用途（本地版）**：用 Python 或 R 创建、修改、审计、导出投稿级科研图件；含多面板证据架构规划、渲染时对齐门与 PDF 碰撞审计、旗舰 Nature 与 NMI 的分阶段文件契约；示意图类需求只走本地手工 SVG 路由，或宿主内置图像能力出草稿（不接第三方 API）。

**description 原文摘录**（SKILL.md frontmatter）：

> "Create, revise, audit, and export manuscript scientific figures in Python or R. Use for 论文配图、科研绘图、多面板图 and submission-ready plots, or explicitly requested AI-generated graphical abstracts and mechanism schematics. Not for interactive dashboards, data cleaning, or statistics-only analysis."

**典型触发请求**（README.md / evals）：
- "把这组数据做成 Nature 风格多面板图，优先 Python。"
- "帮我画一个机制示意图，导出可编辑 SVG/PDF，并给我 source data 表。"（本地版走手工 SVG 路由）
- "先用内置图像能力出一张 graphical abstract 概念草稿，文字和箭头我要能在矢量工具里改。"（草稿按 conceptual 对待，不调第三方 API）
- "参考 figures4papers 里 Nature Machine Intelligence 的布局，帮我补一个方法对比图。"
- "用我的实验数据做一张论文用的分组小提琴图，要求可编辑 PDF 和 600 dpi TIFF。"

**明确不做什么（边界，stance.md / README.md）**：
- 不做 Plotly/Altair/Bokeh 等交互式或 web-first 绘图；不做无投稿目标的纯 EDA 图；主流程为 3D/GIS/非科学插画工具时不加载；不做 Illustrator/Figma 优先排版。
- 纯统计分析（如 "跑 two-way ANOVA 并写结果，不要图"）不触发；纯照片裁剪/调对比度（不组装、不审计图件）不触发。
- 不把 AI 生成图片当作真实实验结果或定量数据面板；不把内部可用的 AI 草稿自动称为可投稿终稿。
- 不凭空补统计检验、样本量、误差线含义或实验条件；不为渲染方便静默抽样、忽略变量或删除不完整观测。
- 不把自动校验通过当作视觉验收；对齐门只保证已声明的几何关系在容差内，最终交付仍需逐面板检查。
- 私有模板不在用户可见输出中暴露路径、文件名或来源；第三方 `figures4papers` 材料的版权以其 `THIRD_PARTY_NOTICES.md` 为准。

## 二、运作模式（加载/路由机制）

### SKILL.md 作为路由

SKILL.md 标题为 "Nature Figure Making — Router"。路由协议五步：0. 检查 graphical-abstract / AI-schematic 路由；1. 读 manifest 与 core 层；2. 解析 plotting backend；3. 加载匹配的 backend fragment；4. 按固定顺序建图；5. references 按需。

manifest v2.8.0 声明：
- `always_load`：`static/core/contract.md`、`static/core/stance.md`（skill-local core；设计说明：figure 不使用面向文字的 nature-shared 层）。
- `axes.backend`（**阻塞门 + 持久化偏好**）：值 `python` → `static/fragments/backend/python.md`，`r` → `static/fragments/backend/r.md`；`multi: false`。选定后 backend 对一切绘图、预览、导出、视觉 QA **排他**。
- `preference`：`backend_script: scripts/nature_figure_backend.py`；配置 env `NATURE_FIGURE_CONFIG`，默认 `~/.config/nature-skills/nature-figure.json`。
- `routes`（本地版保留两条跳过 backend 门的非绘图路由；原 OpenRouter 路由停用）：
  - `graphical_abstract_workflow` → `references/ai-graphical-abstract-workflow.md`（AI 辅助 GA 的规划/修改/审计；本地版的"生成"仅允许宿主内置图像能力，规划与纯审计不需要 Python/R 门）。
  - ~~`openrouter_image_generation`~~ → **本地版停用**：不读 `references/openrouter-image-generation.md` 的 API 调用章节、不运行 `scripts/generate_openrouter_schematic.py`、不配置任何 key/端点；该类显式请求改由"宿主内置图像草稿（按草稿纪律）或手工 SVG 路由"承接。
  - `vector_schematic_workflow` → `references/vector-schematic-workflow.md` + `scripts/audit_svg_schematic.py`（手工可编辑 SVG 架构/流程/机制图；本地版示意图需求的**首选路由**；画结构不画数据；先确认简报再绘制，交付前跑 SVG 审计）。
- `references.on_demand`：20 条条件→路径映射（见第六节）。

### 静态层与动态判断的分工

- `static/core/contract.md`：五点图件契约、backend 排他、缺 runtime 规则、数据完整性门。
- `static/core/stance.md`：默认姿态（原型分类、hero panel、白底、克制调色板、统计与图像完整性作为图件一部分）、颜色政策、用户可见隐私规则、加载/不加载清单。
- `static/fragments/backend/python.md`：Python-only 执行规则 + rcParams 快速启动（`svg.fonttype='none'`、`pdf.fonttype=42`、font.size 7）+ `save_pub_py()`（导出前强制 `require_matplotlib_panel_alignment()`）。
- `static/fragments/backend/r.md`：R-only 执行规则 + `theme_classic(base_size=6.5)` 快速启动 + `save_pub_r()`（patchwork 时强制 `require_patchwork_panel_alignment()`，svglite/cairo_pdf/agg_tiff 三联导出）。
- 只加载选中的那个 backend fragment，**不加载另一个**。

### 对 nature-shared 的依赖

仅两处显式引用：
- 图序须跟随手稿论证升级时加载 `../nature-shared/core/nature-results-discussion.md`。
- 目标为 Nature Machine Intelligence 时加载 `../nature-shared/journal-formats/nature-machine-intelligence.md`（6 个主 display 预算、≤10 个 Extended Data、initial-vs-production 边界、300 dpi/180 mm、source-data 契约；图注无现行官方独立数字，2018 官方 brief 历史建议 <300 英文词，按整注计，目标 150–250 词；不得引入旗舰 Nature 的上限）。
- 显微/凝胶/印迹图像完整性加载 `../nature-shared/core/research-compliance.md` 相应章节（nature-article-requirements.md 内指引）。

## 三、核心工作流（实际功能流程）

### 0. 非绘图路由的优先判定（本地版：两条路由）

- **AI 辅助 graphical abstract**：一切用 AI 规划/生成/修改/审计 GA 的任务，先读 `ai-graphical-abstract-workflow.md`。它管辖信息/读者简报、构图与配色、政策门、人工科学审查、披露边界与 provenance。**Nature Careers 文章只是从业者建议，不等于投稿许可**；须核验目标期刊当前官方政策。本地版的生成动作只能来自宿主内置图像能力。
- **示意图/机制图/graphical abstract 显式请求（本地版替代原 OpenRouter 路由）**：不问 "Python or R?"。首选引导至**手工 SVG 矢量路由**（可编辑、可审计、完全离线）；用户坚持要 AI 出图时，仅在宿主内置图像能力可用时生成概念草稿，并依次套用 `ai-graphical-abstract-workflow.md` 的简报/划界/QA/provenance 纪律。**禁止**调用 OpenRouter、GPT Image API、OpenAI 兼容端点或任何第三方图像服务，禁止设置相关 key/URL；产出按**草稿示意图**对待，不发明实验数值、作者 logo、机构标记或未支持的机制；内部有用性与投稿资格分开判定。
- **手工 SVG 矢量示意图**：不问 Python/R；读 `vector-schematic-workflow.md` + `vector-schematic-style-rules.md`，绘制前与用户确认简报，交付前跑 `audit_svg_schematic.py`。此路由永不绘数据。
- 只有不属于上述显式请求的绘图/制图/图件组装任务才进入 Python/R backend 门。

### 1. Backend 解析（阻塞门，contract.md + SKILL.md 第 2 节）

判定顺序：① 当前请求显式选 Python/R → 用之并 `nature_figure_backend.py set python|r` 存偏好；② 输入文件/工作流明显语言专属 → 用之并存；③ 复用本任务已建立的选择，否则 `nature_figure_backend.py get` 读持久偏好；④ 全无 → **只问一次** "Python or R? I will remember this as your default."，只暂停依赖绘图的步骤，得到答复后先存再做。只读图件审查与后端无关数据检查不需此选择。不因美观猜后端；用户显式要求推荐时才用 `references/backend-selection.md`（推荐 R：R 脚本/RDS/Seurat/DESeq2/limma/ggplot 模板、ComplexHeatmap 富注释；推荐 Python：NumPy/Pandas/PyTorch 流水线、自定义版式、image plate），陈述理由并存档。缺 runtime/包时：暂停渲染、继续独立脚本/数据检查、报告确切 blocker、可给脚本与安装命令，**绝不跨语言出替代图**（evals: `backend-exclusivity-r-missing-runtime`、`backend-exclusivity-python-missing-package`）。非选中语言仅允许非视觉工作（列文件、查 CSV 行数、解压、转换数据），不得开图形设备或产出图像/矢量文件。

### 2. 图件契约（五点，core/contract.md + references/figure-contract.md）

写码前先立契约：
1. **Core conclusion**：一句话带动词的 claim 及其回答的 Results 级科学问题。
2. **Evidence chain**：每个 panel 映射到一个独特推理角色；重画/重复者删、并、降级。按 claim 而非源表格规划；配对数据（同 dataset/subject/seed/task）要看配对差值而非仅看重叠的边缘分布。
3. **Archetype**：`quantitative grid` / `schematic-led composite` / `image plate + quant` / `asymmetric mixed-modality figure` 四选一。
4. **Backend**：显式或已存的 Python/R 轨道全程排他。
5. **Journal/export contract**：最终尺寸（单栏 ~89 mm、双栏 ~183 mm）、**每个渲染字形 ≥5 pt** 下限、可编辑文字、source data、统计、图像完整性注记、导出格式、阻塞式对齐门与渲染碰撞 QA。

figure-contract.md 补充：工作笔记中的契约模板（Core conclusion / Results-level question / Panel map / Evidence hierarchy / Statistics needed / Source data needed / Image-integrity notes / Reviewer risk）；不从喜爱的模板出发；面板顺序默认 建立体系→主效应→机制/定位→量化代表性观察→稳健性/对照；reviewer-risk 提示清单（n 是否可见、误差定义是否一致、坐标轴可比性、配对视角、标签大小写保真等）。

### 3. 多面板证据架构（references/multipanel-evidence-architecture.md）

- 规划默认 **one figure = one major claim**（强默认，非计数铁律）；panel 字母是阅读顺序不是固定功能。
- 十类证据角色表：setup/schematic、representative example、primary quantitative evidence、baseline/control、decomposition、stratification、orthogonal validation、perturbation/stress test、boundary/failure case、mechanistic evidence。**角色多样性优先于指标多样性**；反例 `a:R2 → b:R2 pairwise → c:MAPE → d:MAPE pairwise`（4 panel 仅 ~2 角色）。
- 四种证据链原型：validation envelope（可信度：定义→确立→失效模式→攻击→设界）、scale-to-instance chain（群体模式→量→空间→内部结构→代表实例）、discovery sequence（假设→决定性实验→分析→精炼假设→机制验证）、capability ladder（是什么→学到什么→胜过替代→更难条件→更广能力）。跨图可复用链 `establish → compare/control → stress-test → broaden → bound`。
- Corpus 锚点（推理示例非抄版式）：MIRA（validation envelope）、Centromere architecture（scale-to-instance）、Robin（discovery sequence）、TabPFN（capability ladder）。
- 图序与 Results claim escalation 对齐：`Fig.1 是什么 → Fig.2 评估/资源可信 → Fig.3 现象存在 → Fig.4 胜过替代 → Fig.5 更难条件存活 → Fig.6 机制/普适/边界`（原型非模板）；两图同可概括为 "X performs well" 则合并/降级/改写。
- panel 去留三问必要性测试 → 路由到 主图 / Extended Data/SI / 另一张图 / 删并；不因负面结果隐藏改变结论的证据。
- 多面板审计表（绘制前与终渲后各一遍）：claim 能否一句话带动词、每 panel 是否独特角色、阅读链是否成立、最强证据是否视觉主导、对照/攻击/失效边界是否在场等。

### 4. 默认姿态与配色（core/stance.md + design-theory.md + nature-2026-observations.md + api.md）

- 原型优先构图；一 hero panel + 从属证据 panel；白底，黑色仅用于显微/体渲染 image plate；类别空间固定时直标优于图例；一图一克制调色板（通常一个 neutral + 一个 signal + 一个 accent 族）；感知分离必要非充分——渲染后验证 hero 系列比基线更醒目，不把顺序浅→深色标当无关分类色用。
- 统计、n、误差定义、source-data 可追溯、图像完整性是图件一部分；可比 seed/fold/split 聚合 panel 用同一不确定性定义或声明豁免；保 canonical 大小写（`XGBoost`、`DeepSeek`、`GPT-5.2`、`RF`），禁盲目 `.title()`。
- 用户提及宽泛 "Nature 风格" 时先读 `nature-2026-observations.md`：五种 2026 采样页面原型（schematic-led composite 示意占 45–60% 高；dark image plate 黑底+cyan/magenta 荧光通道；clinical triptych 三行纵向/森林/汇总；dense categorical 直标+纹理；asymmetric mixed-modality hero 主导），及跨切面规则（小号加粗小写 panel 标签、叙事而非仪表盘、克制用饱和色等）。
- 颜色族（api.md）：`PALETTE`（blue_main `#0F4D92` hero、green/red 系列、neutral 系列、gold/teal/violet/magenta 点缀）；`PALETTE_NMI_PASTEL`（baseline 冷族 + ours 粉紫族，绿/红只留 delta 方向）；`PALETTE_NATURE_IMAGING`（黑底+cyan `#22D7E6`+magenta `#FF2AD4`）；材料/临床/基因组学专属族。ablation 用单色变 alpha。
- Python 三行强制 rcParams（任何图创建前）：`font.family='sans-serif'`、`font.sans-serif=['Arial','DejaVu Sans','Liberation Sans']`、`svg.fonttype='none'`（文字保 `<text>` 节点可编辑）；SVG 为主格式。helper：`apply_publication_style()`、`is_dark()`、`add_panel_label()`（固定点偏移，跨行 panel 不用共享 axes-fraction `y=1.02`）、`style_dark_image_ax()`、`make_grouped_bar()`、`make_trend()`（2D 数组默认 ±SD 带）、`make_forest_plot()`、`make_heatmap()`（按亮度选字色）、`finalize_figure()`（tight_layout→对齐门→多格式导出→close）。
- design-theory.md 补充：字号层级（dense 期刊多面板 7–9 pt）、spines 只留左下、动态 y 轴收紧、x-tick 抑制、legend-only 面板、三级多面板信息架构（overview/deviation/relationship）与四种冗余陷阱表、复现 checklist。

### 5. 模板与资产适配（references/asset-adaptation.md + template-catalog.md + demos.md）

- 复用四级：Exact reuse（语义/数据形状/变换/backend 全同）/ Structural adaptation（语义与维度同、字段名异，须显式字段映射+变换守卫）/ Style-only inheritance（图族有用但数据结构/统计不同，只继承配色字体间距）/ Build anew。不得因脚本能渲染自带示例就称其 production-ready。
- 映射契约模板：`template field -> user field -> role -> units -> allowed values` 等；模糊映射向用户确认；标识符与测量分开；保类别顺序。
- 变换守卫：log 轴需严格正值（除非声明 signed-log/pseudocount 且科学上合理）、比值需有限分母、sqrt 需非负、min-max 需非常数有限值域、分箱/密度需足够观测、相关/PCA/聚类需显式缺失值处理。守卫失败时仅在科学含义仍成立时改变换并记录，否则 style-only 或重建。
- 数据完整性：默认用全部观测与请求变量；不为美观/速度降采样（用 rasterize/hexbin/透明点/带声明规则的聚合）；过滤须记录精确谓词与前后计数；用户要求抽样时记录方法/样本量/种子/是否改变推断；**生产中绝不残留模拟值**（demo 须显式隔离）。evals 对应：`large-data-no-silent-sampling`、`asset-semantic-mismatch`（2D 联合密度 ≠ 八个 1D 分布，不得静默丢弃六个 biomarker）、`exclusion-count-accounting`、`template-transform-guard`（负值零值撞上 log 模板不得静默删值或加伪计数）、`demo-data-production-boundary`。
- `template-catalog.md`：五个已验证 CSV 驱动 Python 模板（`plot_templates.py` 子命令 `volcano`/`roc`/`dotplot`/`marginal`/`paired`，各带安全守卫；`--demo` 仅限冒烟测试并在 QA JSON 标注；数值缺失/非有限默认停止，`--drop-incomplete` 是显式例外并记录排除计数；模板不算假设检验）。
- `demos.md`：第三方 figures4papers 九个保留项目的查图索引与版权边界（先读 `THIRD_PARTY_NOTICES.md`；学布局配色但用原创代码与用户数据重实现；绝不复用其稿件专属标签/数值/资产；优先走仓库自有实现路径）。

### 6. 期刊契约（nature-article-requirements.md + figure-legend-conventions.md）

旗舰 **Nature**：
- **Stage gate**：`initial_submission`（图可嵌 Word/PDF，图注与图同页，不要求生产级文件，但仍跑统计/source-data/图像完整性检查）/ `revision` / `accepted_in_principle`（主图与 Extended Data 走不同文件契约）。
- **图注契约**：完整图注 <250 词；先一句全图标题句，再逐 panel 简描；不叙事结果、不复述 Methods；给出独立可读所需定义、精确 n、重复定义、中心/离散、检验、校正、P 值显示；第三方材料注明改编与许可。
- **主图生产文件**：89/183 mm 宽、≤170 mm 高、正文 5–7 pt、panel 标签 8 pt 加粗小写直立、一致 sans-serif（Arial/Helvetica）、氨基酸序列用 Courier 类等宽字体、文字可编辑不转曲、嵌入 TrueType 2/42、优先 `.ai`/`.eps`/可编辑 `.pdf`，也接受分层 PSD/PPT 转 PDF/`.svg`/Excel/`.ps`；不得把扁平 jpeg/tiff/png 当主图终稿；组件内嵌、单文件 <50 MB。
- **Extended Data 生产文件**：RGB、≤300 dpi、≤10 MB、优先 `.jpeg`（或 `.tiff`/`.eps`）、每项一页留图注位、按通讯作者姓氏+ED 编号命名。两路线不得互串。
- **可及性与图像完整性**：轴与刻度齐全、单位括号、无背景网格/阴影/装饰/彩字、可及调色板（不靠红绿或彩虹）、RGB、照片 ≥300 dpi（最高在线打样 450 dpi）、比例尺可编辑且用比例尺而非放大倍数。
- 交付审计表（Item/Stage/Required contract/Current file/Status/Action）+ 官方来源清单（verified 2026-08-08）。
- **不从 Nature Communications 语料或本 skill 视觉示例推断旗舰 Nature / NMI 要求**。

图注文字约定（figure-legend-conventions.md，2025 Nat Commun CS/AI 语料 20 篇蒸馏，勿抄原文措辞）：固定骨架 `Fig. N | 加粗名词短语总题` → `a/b/c` 现在时电报式分面 → 统计写进图注（`mean ± 95% CI (n = 1373) … one-way ANOVA with Tukey correction`）→ 末尾 Source Data 套语；视觉事实现在时、制作方法过去时；自足性（颜色/形状映射、样本量、PDB/RMSD/单位进图注）；图内图例按展示标签首字母大写且保 canonical 拼写；末句可给推断结论但须有 panel 支撑；综述图注逐子图一句定性+授权行；表格 `Table N | 名词短语`。**勿给图题行放数字/结果**。

### 7. AI 示意图路由细节

**ai-graphical-abstract-workflow.md**：
- 政策门：Careers 专栏非许可；生成投稿候选前核验目标期刊当前 GA/AI/图像完整性/版权/披露官方页面并记录期刊/URL/访问日期；Nature Portfolio 风险评估框架（assistive 可用；interpretive/generative 需人工监督+核验+透明披露；opaque/不可核验/泄密/替代判断不允许）；政策不明时标 "**internal design draft — submission eligibility unverified**"；内部有用性与投稿资格分开判定。
- 五步：①定义传播目标（读者该记住的一句话、图件类型、受众词汇、证据边界、要省略的细节）；②建视觉简报（读同类 GA 学视觉语法不抄受保护作品；显式阅读路径；有限高对比色盲安全配色；最强 accent 留给中心因果步）；③给 AI 划界（可蒸馏 claim/比术语/提构图/查配色/把作者草图转草稿/头脑风暴隐喻；不得发明测量、机制、结构、临床效应、引用、标签、证据；未授权不把保密稿件发公共 AI 服务）；④prompt 即图件契约（十字段，含 `concept draft` 输出角色声明）；先生成布局备选再打磨，先改科学结构再改字体配色，关键文字/箭头/化学结构/定量标记尽量用确定性工具重画；⑤人工科学与出版 QA（实体/方向/因果/尺度/解剖/时序、标签拼写单位、无编造数据、终尺寸可读层级、原创性/许可/署名、期刊资格与披露措辞、provenance bundle：prompt+模型版本+日期+参考图权利+候选+选定+人工修改日志；作者对最终资产负责）。

**本地版 AI 示意图生成约定（替代原 openrouter-image-generation.md 的 API 路由）**：
- 不访问 `https://openrouter.ai/api/v1/images` 或任何 OpenAI 兼容图像端点；不设置 `OPENROUTER_API_KEY`、`ORCAROUTER_API_KEY`、`SCHEMATIC_IMAGE_API_URL` 等环境变量；`generate_openrouter_schematic.py` 停用（其 `--dry-run` 生成 payload 的能力如纯本地需要可仅作 payload 参考，但绝不发起网络请求）。
- 生成途径只有两条：宿主内置图像能力出概念草稿（平台自带能力，不另配凭据），或直接走手工 SVG 矢量路由产出可编辑终稿。
- 原安全红线全部保留并加强：生成图是草稿概念非证据；不发明定量值/p 值/谱/显微发现/机构 logo；AI 常拼错文字——终版标签必须在 Inkscape/PPT 或 Python/R/SVG 矢量流中重画；可能被解读为数据面板时显式标 conceptual；保密稿件不离开本机/宿主环境。
- 原 prompt 契约八要素、画幅默认（16:9 GA、4:3 机制图、1:1 封面概念）、provenance 留存（prompt+模型版本+日期+参考图权利+候选+选定+人工修改日志）与 QA 六步（视觉检查、拼写、幻觉清单、矢量重画建议、provenance 留存、内部/投稿双判定）继续适用于内置能力产出的草稿；`request_metadata.json` 照常在本地落盘。

### 8. 手工 SVG 矢量示意图路由（vector-schematic-workflow.md + style-rules）

- 六步：①写简报并获用户确认（先推断后提问，一次至多三点、每点给猜测+备选；简报含组件 exact label/new-existing-external 标签、编号主路径、一个论点、布局意图、3–5 条假设、图注草稿；组件标签与手稿术语一致，**永不加简报没有的框或箭头**）；②按期刊定画布（旗舰 Nature：89 mm≈252 pt 单栏、183 mm≈519 pt 双栏、5–7 pt、文字 live 不转曲；带单位 width/height + 匹配 viewBox，一单位=一点）；③按内容结构选排布（阶段→管线；分层→堆叠容器；枢纽+工人→中心扇出；循环→环+一条虚线回边；对比→并排同构；预算约单栏 13 框/双栏 20 框，超重则合并/×N/拆图，**绝不靠缩字塞内容**）；④画 SVG（每组件一组带稳定 `id`；文字保 `<text>`；无渐变/滤镜/阴影/脚本/外部引用，位图以 data URI 嵌入；框尺寸=量得标签宽+padding）；⑤check-look-fix 循环（跑 `audit_svg_schematic.py` 查终尺寸字号/画布宽/画布外文字/禁用元素，非零退出阻断交付；渲染 PNG 终尺寸目检——审计看不见框体重叠与箭头交叉；对照简报走查）；⑥交付 SVG+PDF+PNG 预览+图注草稿+假设清单+编辑说明（元素 id 对得上简报），声明所用字体，作者审科学前一律视为草稿。
- 样式规则：白画布、至多两个饱和填充且恰好一个 accent 标记论文新组件、existing 用浅灰填充、external 白底灰描边、红只给错误/瓶颈/高亮路径；每条有向边有箭头、主路径最粗；实线=恒常流、虚线一图只表一意（反馈或逻辑分组二选一）、结构走正交线仅反馈用曲线；嵌套≤3 层、容器不比子件深、等距共栅格、×N 表重复；图例仅在 >4 种含义编码且图注说不清时加；主路径用圈数字编号且与图注一致；常见失败表（框太多/多个 accent/虚线二义/字号越界/箭头碰标题/静默决策）。

### 9. QA 与交付门（references/qa-contract.md + SKILL.md 第 4 节）

**四道自动门 + 一道人工门**，交付前、修订包前、含显微/印迹/临床亚组/统计声明时必跑：
1. **源码预检** `validate_figure.py`（无依赖；查语法、字体配置与字号下限、mathtext 缩小风险、图例大小写、危险色图、可编辑文字设置、矢量/栅格导出、DPI、期刊宽度、潜在采样/未报告缺失排除、模拟数据泄漏、log 守卫、插值单调性、随机不确定性编码、旋转文字锚定、跨 backend 引用；结果当确定性源码审计，不当正确性证据；FAIL 必解，WARN 逐条审）。
2. **PDF 字形下限** `audit_pdf_text.py --min-pt 5`（扫 `Tf`+`Tm`+`cm` 变换后的实际字形；抓 mathtext 上下标——7 pt `$R^2$` 的上标可低至 4.9 pt；优先用 Unicode `R²`）。
3. **多面板对齐门**（每个含 ≥2 可比 panel 的图，最终版式绘制后、导出前；任何影响几何的改动后重跑）：Python 调 `require_matplotlib_panel_alignment()`（须复制脚本或加 PYTHONPATH），R source `panel_alignment.R` 调 `require_patchwork_panel_alignment()` 写 layout manifest 后由同一 Python 审计器审计（Python 不重画 R 图）。默认物理容差 **1.5 pt**（≈0.53 mm）。检查：同行共顶/底边+等高；横排 3/4 等跨 panel 等宽；同列共左/右边+等宽；不等跨共享外边界（自动推断"左二右一/左一右二"镜像两式，跨行 panel 与上小 panel 比顶边、与下小 panel 比底边，不误判等高）；≥3 可比 panel 的重复 gutter 一致；粗体小写左上 panel 标签锚点；矩形不重叠。结果语义：`NOT APPLICABLE`/`PASS` exit 0 继续；`FIX BEFORE DELIVERY` exit 1 阻断导出；`REVIEW REQUIRED` 逐条解决或记录（`--strict` 使 WARN 阻断）；`NOT AUDITABLE` exit 2 不得声称对齐通过。豁免须指明 panel、确切检查项、非空理由（如 `{"panels":["b"],"checks":["panel-width"],"reason":"middle hero panel"}`）；**不得放宽全局容差掩盖单一有意例外**；hero/inset/colorbar 走排除或豁免。对齐 SVG 是 QA-only 诊断物。
4. **渲染碰撞审计** `audit_figure_collisions.py`（每次生成或任何影响布局的修订后必跑——文字/字体/图例/注释/轴/数据/误差/panel 尺寸或排布变动都触发；不得复用旧报告；后端中立读 PDF 几何）：`text-text`/`text-stroke`/`text-page-clipping` 为 FAIL 阻断（修后重导再跑）；`text-fill-edge`/`text-image-edge` 为 WARN 终尺寸逐条复核；contained overlay 仅信息性。`--strict` 使 WARN 阻断；exit 2/NOT AUDITABLE 须报告依赖/PDF blocker；需 PyMuPDF（`pip install -r requirements.txt`）；marked PDF 仅诊断，永不替代选中 backend 的源文件或投稿文件。
5. **人工逐面板终审**：最终物理尺寸下逐 panel 看再整图看；每 panel 记一行（unique claim / center / spread+reason / replicate unit / 显示标签 / 对齐组结果 / 碰撞发现）；心里遮住每个 panel 论证是否仍完整；可比 panel 并排比术语/不确定性/轴/色映射；加误差后删掉编码同一空隙的箭头/括号/填充。

QA contract 另有：投稿前清单表（23 行：核心结论、原型、backend 排他、最终尺寸、字号、字形下限、panel 标签、可编辑文字、字体、色彩、图例策略、大小写、统计、可比不确定性、注释净空、panel 对齐、视觉层级、数值变换、source data、栅格分辨率、显微比例尺、图像完整性、导出捆绑）；统计图注最小集（n 定义/生物与技术重复/中心/离散/检验/校正/P 值显示/source-data 文件；ML 图另加 split/种子折数/指标定义/CI 定义/基线定义）；图像完整性最小集（raw/processed 文件、crop、亮度对比 gamma、伪彩、尺度校准、拼接、跨图复用、量化链接；全局调整优于局部选择性编辑）；不确定性一致性（一个 `fill_between` 调用不证明覆盖）；几何与注释放置（量实际对象间距、用 tight bbox、标签位置由 `max(center+spread)+margin` 导出而非固定 `LABEL_Y`、旋转文字 `rotation_mode="anchor"`、不用不透明白 bbox 遮线、等像素 y 不等于视觉等距）；颜色分离（ΔE 与对比必要非充分、渲染后验证层级）；变换与配对检查（`np.interp` 需递增 xp，用 `figure_safety.py::interp_monotone`；`label_y_above`）；导出块按选中 backend 各跑各的，导出后打开 SVG/PDF 验证文字可选、标签不叠、终尺寸可读。

## 四、输出契约与质量检查

- 必产出：可运行的 Python 或 R 绘图脚本；SVG（主，文字可编辑）+ PDF + TIFF/PNG；面板说明、source data 映射、排除计数、逐面板视觉审查表、alignment JSON/诊断 SVG、碰撞 JSON/诊断 PDF、QA 记录；示意图任务（手工 SVG 或宿主内置能力草稿）另产出可编辑 SVG 终稿/概念草稿与需人工重画、核实元素清单。
- 命名/路径约定：输出默认 `./figures/`；对齐产物 `figure.alignment.json` / `.alignment.svg` / `.alignment-layout.json`（R）；碰撞产物 `figure.collision-audit.json` / `.collision-audit.pdf`。
- 硬性数字契约：单栏 89 mm / 双栏 183 mm；高 ≤170 mm；正文 5–7 pt；panel 标签 8 pt 加粗小写；**每个渲染字形 ≥5 pt**；对齐容差 1.5 pt；旗舰 Nature 图注 <250 词；NMI 6 主 display/≤10 ED/300 dpi/180 mm/图注历史建议 <300 词（目标 150–250）；Extended Data RGB/≤300 dpi/≤10 MB/`.jpeg`；主图生产文件优先 `.ai`/`.eps`/可编辑 `.pdf`，单文件 <50 MB；照片 ≥300 dpi（450 dpi 优先）。
- evals 的 `delivery-runs-static-and-visual-qa` 汇总交付门：对齐门→validate_figure→audit_pdf_text→audit_figure_collisions→解 FAIL→审 WARN→终尺寸目检；自动校验不当作统计或完整视觉正确性证明。

## 五、关键规则与红线

- **Backend 排他**：选定 Python/R 后，一切绘图脚本、预览、导出、视觉 QA 渲染都出自该 backend；缺 runtime 只报告 blocker + 给脚本与安装指引，绝不跨语言替代出图。
- **零第三方图像服务（本地版）**：不配置/调用 OpenRouter、GPT Image API、OpenAI 兼容端点等任何外部图像服务，不设置相关环境变量，不运行 `generate_openrouter_schematic.py` 发请求；AI 草稿只能来自宿主内置图像能力，示意图终稿优先走手工 SVG；内置能力缺失时直接告知用户，不得寻找外部替代。
- **不问第二次**：backend 选择在任务及其后续中复用；未解决时只问一次并持久化；只读审查不需选择。
- **5 pt 字形下限**作用于每个渲染字形（含 mathtext 上下标），非仅源码 parent fontsize。
- **数据完整性**：默认全量观测与请求变量；任何排除记录精确规则与前后计数并保留原始输入；不为渲染便利降采样；生产路径绝不残留模拟值。
- **对齐门与碰撞门分离**：碰撞 PASS 不是对齐通过的证据；两个 FAIL 级都阻断交付；`NOT AUDITABLE` 不得声称通过；不为单一例外放宽 1.5 pt 全局容差。
- **不发明**：统计检验、n、误差定义、机制、实验数值、logo、机构标记；AI 图/示意图按草稿对待；可能被读作数据面板时显式标 conceptual。
- **隐私规则**：不向用户暴露私有路径/文件名/内部参考文件名/模板标识/来源；用户显式要求审计轨迹时才给确切路径。
- **期刊边界**：不从 Nat Commun 语料推断旗舰 Nature 或 NMI 要求；Nature Careers 文章不是投稿许可；政策不明标 internal design draft。
- **版权**：figures4papers 等第三方材料不适用仓库 MIT License，重用前查许可并记录 provenance。
- **最高优先级**：the chart serves the scientific logic——美观、模板匹配、复杂版式都从属于让核心结论清楚、可辩护、可评审。

## 六、配套资源

**references/**（一句话主题）：
- `figure-contract.md` — 把请求转成核心结论、证据层级、panel 映射与评审风险检查的契约方法。
- `multipanel-evidence-architecture.md` — 从一个 Results 级问题到互补 panel 角色、四种证据链原型与跨图 claim escalation 的规划/审计（语料蒸馏，非官方要求）。
- `backend-selection.md` — Python/R 推荐决策表、排他规则、默认栈与混合工作流规则。
- `r-workflow.md` — R 轨道全流程：包清单、contract scaffold、patchwork 版式、对齐门、ComplexHeatmap 导出。
- `r-template-index.md` — 私有 R 模板集合的隐私化适配（按图族检索、改写成自包含脚本、不暴露来源）。
- `asset-adaptation.md` — 模板/示例复用的四级分类、数据契约映射、变换守卫与数据完整性。
- `qa-contract.md` — 交付前全部检查项、四道自动门用法、统计/图像完整性最小集、逐面板审计表。
- `nature-article-requirements.md` — 旗舰 Nature 的 stage gate、图注 <250 词、主图/Extended Data 两套生产文件契约与官方来源。
- `design-theory.md` — 字体/轴/配色/版式/各图型规则与导出政策的原创性综合及复现 checklist。
- `api.md` — Python PALETTE 常量、helper 函数签名、对齐门集成与验证规则。
- `common-patterns.md` — 16 个版式/编码模式（hero 示意、legend-only 面板、暗色 image plate、临床三联、非对称 hero 等）。
- `nature-2026-observations.md` — 2026 Nature 采样页面原型与按模态的配色指引。
- `openrouter-image-generation.md` — **本地版停用**：原 OpenRouter/GPT Image 2 的 API、key、prompt 传输契约不再执行；仅其画幅/prompt 要素/QA 清单可作为"内置图像草稿"的参考。
- `ai-graphical-abstract-workflow.md` — AI 辅助 GA 的简报、政策门、AI 角色边界、prompt 契约与人工 QA/provenance。
- `vector-schematic-workflow.md` — 手工 SVG 示意图的"简报→画布→排布→绘制→检查→交付"六步流程。
- `vector-schematic-style-rules.md` — SVG 示意图的字体/颜色/线/箭头/容器/图例规范与常见失败表。
- `figure-legend-conventions.md` — 图注文字骨架（Fig. N | 总题→分面→统计→Source Data 套语）、时态、大小写与期刊词数门。
- `tutorials.md` — 分组柱状、alpha 消融柱状、共享图例趋势、双色温热图四个端到端教程。
- `template-catalog.md` — volcano/roc/dotplot/marginal/paired 五个已验证 CSV 模板及守卫。
- `chart-types.md` — 雷达、3D 球、散点聚类、概率/manifold 概念面板、消融线、fill_between、log 柱状等专门图型模式。
- `demos.md` — figures4papers 第三方示例索引、版权边界与原创重实现路径。

**scripts/**（用途一句话，源自 docstring）：
- `nature_figure_backend.py` — 读写用户默认 plotting backend 偏好（`get`/`set python`/`set r`，配置路径支持 `NATURE_FIGURE_CONFIG` 覆盖）。
- `validate_figure.py` — Python/R 绘图源码的无依赖静态预检（PASS/WARN/FAIL，`--json`/`--strict`）。
- `audit_pdf_text.py` — 扫导出 PDF 内容流 `Tf`/`Tm`/`cm` 实际字形尺寸，执行 5 pt 下限。
- `audit_panel_alignment.py` — 多面板渲染几何对齐审计器；消费 backend 中立 JSON manifest，Matplotlib 可直接调 `require_matplotlib_panel_alignment()`，1.5 pt 默认容差。
- `panel_alignment.R` — R/patchwork 端 helper：在同物理尺寸设备上量 gtable panel 矩形、写 manifest、调用 Python 审计器。
- `audit_figure_collisions.py` — 后端中立的 PyMuPDF PDF 几何碰撞审计（text-text/text-stroke/clipping FAIL，fill/image 边缘 WARN），输出 JSON 与可选标注 PDF。
- `figure_safety.py` — 数值/标注安全 helper：`interp_monotone`（严格单调插值）与 `label_y_above`（标注置于不确定性上界之上）。
- `plot_templates.py` — 五个已验证 CSV 驱动 Python 图型模板（volcano/roc/dotplot/marginal/paired），`--demo` 仅冒烟测试并留痕。
- `generate_openrouter_schematic.py` — **本地版停用，不运行**；原用途是经 OpenRouter Images API 生成示意图草稿。本地版同类需求改走宿主内置图像能力或手工 SVG。
- `audit_svg_schematic.py` — 手工 SVG 示意图终尺寸审计：字号上下限、画布宽度、画布外文字、script/foreignObject/渐变/滤镜/外部引用等禁用构造；不检测重叠与交叉箭头。

**tests/**：`test_figure_safety.py` — figure_safety helper 测试。

**assets/**（概述）：`assets/chart-atlas/` 10 张图型图集预览（bar/line/heatmap/scatter-bubble/radar/distribution/forest/stacked-area/image-plate/network-matrix）；`assets/gallery/` 5 张富信息多面板示例图；`assets/figures4papers/` 九个第三方项目的参考脚本与预览图，另附 `THIRD_PARTY_NOTICES.md`，不自动适用仓库 MIT License。

**agents/openai.yaml**：display_name "Nature Figure"；default_prompt "Use $nature-figure to create and audit a submission-ready scientific figure from these data."

**evals/evals.json**：20 条，代表主题——backend 排他（R/Python runtime 缺失不跨语言出图）、大数据不静默抽样、模板语义不匹配识别、排除计数、log 变换守卫、demo 数据隔离、交付四道门、碰撞审计必跑、对齐门 1.5 pt（含 2+1 跨行与 1×3/1×4 等宽）、中文触发、~~OpenRouter 路由跳过后端门~~（本地版该 eval 随路由一并停用）、AI GA 政策与问责门、多面板图级 claim 与角色多样性、三类不触发（纯统计/交互仪表盘/纯照片编辑）、SVG 示意图简报门（89 mm、5–7 pt、确认后绘制）。
