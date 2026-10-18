# nature-image2ppt（本地版）

> **本地化改造说明**：本版锁定"本地 + 宿主内置"后端——① 不配置、不调用百度 PaddleOCR 云端 OCR（`PADDLE_OCR_TOKEN` 停用），文字几何一律用本地 `builtin-ink` 测量，文字内容由页面 worker 直接查看源图辨识；② 不配置、不调用任何第三方 OpenAI 兼容图像端点（`openai-compatible-api` / `codex-oauth` / OpenRouter 等均停用），图像生成只允许使用**当前 agent 宿主自带的内置图像工具** `builtin-imagegen`；宿主不提供该能力时，相关页面按契约阻塞，不寻求外部替代。③ 不创建 `config.yaml`、不填写任何 token / API key。其余确定性本地 CLI 工作流保持不变。

## 一、定位与触发场景

**用途**：把已有的幻灯片图片、截图、扫描 PDF 或图片型 PPTX **重建为对象级可编辑的 PowerPoint**——文本框、卡片、圆形节点、连接线、箭头、公式、可测量曲线全部还原为原生 PPT 对象，实在无法可靠原生重建的复杂局部保留为有出处的可替换图片资产。它是"还原现有视觉页面"，不是"根据笔记从零创作新演示文稿"（那是 `nature-paper2ppt`）。

**description 原文摘录**（SKILL.md frontmatter）：

> "Reconstruct slide images, screenshots, scanned PDFs, or image-only PPTX files as object-level editable PowerPoint. Use for 图片转可编辑PPT、截图还原PPT and diagram reconstruction; not authoring a new deck from research notes."

**典型触发请求**（README）：

- "把这些幻灯片截图还原成可以逐个编辑元素的 PowerPoint。"
- "把这个扫描 PDF 转成可编辑 PPTX，保留原来的版式。"
- "修复这份图片型 PPT，让流程图、知识图谱和箭头可以编辑。"

**明确不做什么（边界）**：

- 不从论文/提纲/笔记创作新 deck；不做投稿级科研图绘制（`nature-figure`）。
- 不把整页、整个 dashboard/表格/卡片/图表直接截图贴回充数；不把整个知识图谱压成一张位图。
- 不用 Unicode 箭头字形、线段+三角拼头、emoji/文本符号近似任何前景视觉对象；不把"缺失的分离工作流"降级为 warning。
- 本地版不使用任何在线图片服务与在线 OCR：任务提示词与页面图片不离开本机/宿主环境；文字几何用本地 `builtin-ink` 测量，文字内容由 worker 直接查看源图辨识。敏感材料亦可安全处理。
- 照片、复杂插画、密集知识图谱、低分辨率输入、缺失字体与复杂曲线可能部分保留为图片资产或限制还原精度。

## 二、运作模式（加载/路由机制）

本 skill 与其他 nature-skills 结构不同：**它是一个自包含运行时（self-contained runtime）**，没有 axes 碎片路由，所有确定性动作只走一个本地 CLI：

```bash
python <image2ppt-root>/cli/image2ppt/cli.py <command> ...
```

**SKILL.md 作为路由与总纪律**：frontmatter 仅 name + description；正文给出五块规则——①渐进式阅读契约的顺序；②单一事实源（`page_jobs.json` 是唯一页面状态源、`pages/page_NNN/manifest.json` 是唯一页面内容源、`deck_manifest.json` 是最终组装源）；③目录写入边界；④保持迁移前行为（self-containment 是路径/入口迁移，不是重建行为重设计）；⑤运行工作流（图像后端选择 + 5 步流程 + 交付）。并明令：**不得发现或调用另一个 Skill、CLI、Prompt、Schema、模块或状态机**；`doctor` 失败时不得继续，只装所报缺失依赖后重跑。

**manifest.yaml 结构**（version 1.1.0）：无 always_load/axes，只有 `source`（上游仓库 Paul-Jeo/Image2PPT + commit + MIT license）与 `runtime`（python ">=3.10"、entrypoint `cli/image2ppt/cli.py`、requirements、config_template `config.example.yaml`）。

**references 渐进加载契约**（SKILL.md 明确定义的读取顺序）：

- 总是读：`references/workflow.md`；
- 仅安装或 `doctor` 失败时读：`references/runtime-dependencies.md`；
- 仅选择或排查 OCR 时读：`references/ocr-text-hints-contract.md`；
- 写页面 manifest 之前必读：`references/page-decision-tree.md` + `references/manifest-schema.md`；
- 结构化/复合页面追加：`references/region-decomposition.md` + `references/object-routing.md`；
- 有箭头追加：`references/manifest-arrow-extension.md`；
- 涉及位图资产或图像后端追加：`references/assets-provenance-contract.md`；
- 验收或交付输出前必读：`references/qa-contract.md`。

**worker Prompt 双层结构**：每页 worker 的 Prompt 由 `scripts/build_page_worker_prompt.py` 从完整的本地基础层 `prompts/page-worker-base.md` 加保留的 Image2PPT profile 层 `prompts/page-worker.md` 合成，只替换路径与 CLI 绝对入口（`{{RUN_DIR}}`、`{{PAGE_ID}}`、`{{PAGE_DIR}}`、`{{SOURCE_IMAGE}}`、`{{SKILL_ROOT}}`、`{{CLI}}` 占位符），**不得压缩、重述或替换任一层**。

**对 nature-shared 的依赖**：无；skill 目录即完整运行时，不导入其他 Skill。

## 三、核心工作流（实际功能流程）

### 步骤 0：图像后端选择（先于一切图像作业）

- 唯一允许的图像后端是 `builtin-imagegen`（agent 运行时自带的 `image_gen.imagegen` 工具）：worker 可先 `view_image` 检查编辑输入，再导入显式的本地结果路径。
- 当内置工具不可用、调用出错、无法读取输入、或未返回有效本地输出时（`fallback_policy.on` 的四个事件），**本地版不切换到任何外部服务**：页面保持阻塞/失败状态并向用户报告"宿主内置图像能力不可用"，由用户决定是否稍后重试；**缺少可选参数（model/mask/size/quality/out）永远不构成回退理由**。
- **停用一切第三方传输配置**：`codex-oauth`、`openai-compatible-api`、`auto` 以及任何自定义 Base URL / 模型 ID / API key 在本地版均不得配置或传入；`prepare` 一律带 `--image-backend builtin-imagegen`；不得从任务语言模型推断图像后端。
- 实际生产者与回退原因仍记录在页面级 `imagegen-jobs.json`（本地版合法生产者只有 `builtin-imagegen` 与"未产出（阻塞）"两类）。

### 步骤 1：预检与 OCR 选择

`cli.py doctor --json` 是唯一运行时预检（报告 Python 导入、本地 CLI/资源、LibreOffice/PowerPoint 渲染、**本地** OCR 回退、图像后端就绪、字体、Pillow、ImageMagick）。OCR（本地版）：**永不配置百度 AI Studio `PADDLE_OCR_TOKEN`、永不发起云端 OCR 请求**；统一使用本地 `builtin-ink`——它只测量文字几何（位置/字号）不识字，因此页面上"是什么字"一律由 worker 直接查看 `source.png` 自行辨识并写入文本框；doctor 若报告云端 OCR 未配置属正常状态，不需要处理。

OCR 契约细节（本地版）：本地墨迹测量逐张在规范化后的 `source.png` 上进行（有 PDF 时同样逐页渲染为图后测量，不拼成临时 PDF）；只保留 `text`/`paragraph_title`/`vision_footnote` 版式块，worker 自辨文本仅作参考，**每个块都要对本地源墨迹重新测量**得到 `box_px`、字形高度、行数、CJK/Latin 字号候选与稳定 size group。阈值不得在重建中改动。

### 步骤 2：prepare 一次 run

```bash
python <image2ppt-root>/cli/image2ppt/cli.py prepare <input...> --out-root output/image2ppt --image-backend builtin-imagegen
```

`prepare` 规范化输入（单图/多图/PDF/图片型 PPTX）、提取 speaker notes 元数据（写入 `notes_manifest.json`，保留 note XML 与哈希）、创建 `deck_manifest.json`/`page_jobs.json`、逐页生成 `pages/page_NNN/source.png` + `page_request.json` + `text_hints.json`/`text_hints.png`，并记录图像后端契约。`page_request.json` 只含任务边界（页 id、页目录、源图、`slide` 与 `content_box`、并发上限、写入范围、必需产出、用户约束、图像后端契约），**不得含页面类型预测或对象级决策**；接近 16:9 的输入用标准宽屏画布，其余按源图像素换算自定义画布，worker 必须原样照抄 `slide`/`content_box` 不得重算。`--no-text-hints` 仅在有意关闭 OCR 时使用；补生成提示用 `run hints <run-dir>`（不新建 run）。

### 步骤 3：推进与认领页面

`run next <run-dir> --json` 只读状态并给出下一步：`rebuild_page_locally`（仅恰好一个 pending 页时出现，父 agent 用 `--local` 认领自建）、`dispatch_pages`（列出须各自派给 worker 的 `suggested_pages`）、`wait`（已派发页面占槽，**不得因慢而 reset**）、`finalize`、`configure_backend`。派页流程：`scripts/build_page_worker_prompt.py <run> --page <id> --out <页目录>/worker-prompt.md` → `run dispatch <run> --page <id> --agent-id <id> --prompt-file <绝对路径>`（`--local` 仅限单页 run）。`run dispatch` 记录一条活跃执行租约；已派发页面在明确完成/失败/取消/证实丢失前一直活跃，时间流逝本身不等于丢失。

### 步骤 4：重建与门控每一页（worker 侧核心）

worker 的**强制第一动作**是完整读 `page-decision-tree.md`、`manifest-schema.md`、`cli-helper.md` 三个文件（profile 层再加 region-decomposition/object-routing/manifest-arrow-extension/qa-contract 四件），任何未读先做的决策无效。

**页面三步决策（page-decision-tree.md，严格按序，先做对象来源决策再写 manifest）**：

1. **背景识别与修复**：纯色/简单渐变/普通卡片容器/表格线/规则纹理/无遮挡空白区 → 原生或脚本重建（`background_strategy.mode: native-or-script`），不得浪费图像后端调用；含复杂照片/空间/产品图且被前景文字图标遮挡的 → 走图像编辑"净底"（clean base：同一背景去掉待重建前景，prompt 必须要求保持原比例/构图/透视/物体位置/颜色/光照/材质/纹理/景深，禁止新场景、伪文字、水印、模糊块）；已有背景区域仅在无文字图标遮挡、不产生"背景一份+可编辑对象一份"重复、非整页截图时才可原样复用。**dashboard 默认不是背景**：标题、数字、表格、坐标轴、图例、指标卡、筛选器、标签都要在第 3 步拆为原生对象，只有地图/热力图、复杂截图底图、数据无法可靠还原的复杂图表区、起视觉背景作用的复杂纹理可作背景或图像区。
2. **前景资产分离**：每个非文本前景视觉对象（前景照片/截图/视频封面/地图碎片、图标/象形/logo 样标记、徽章贴纸胶带角标、手绘痕迹/手绘箭头/装饰下划线、复杂箭头、仪表盘语义小图标、植物人物设备等）**必须**经图像编辑 asset-sheet 工作流分离——一张稀疏排布的色键（chroma-key）资产表装尽量多对象，键色取与主体/描边/阴影/高光都远的高饱和纯色（绿主体禁用 `#00ff00` 等）；prompt 要求忠实分离而非重绘：保原形/笔触/颜色/比例/间距/纹理，对象完整互不接触、留白充足、数量顺序与 `visual_inventory` 一致、无可读文字、无整卡整表整页碎片、禁止美化简化或同义符号替换。分离后用 `image import`（记录实际生产者、哈希、角色、回退原因）+ `image process-sheet`（从图边采样键色、去底色、拆成透明资产）处理，再对照 `visual_inventory` 逐件对账；缺件/错符/变形/粘连/文字污染须重新生成。**没有回退路径**：分离做不出合规资产时该页阻塞（`validation.json.passed=false`），不得降级为 warning、不得用原生形状/emoji/直接裁源图顶替、不得用本地 Python/Pillow/SVG/HTML 手绘前景对象（确定性工具仅用于规范化、记录、去底、拆分、公式渲染、构建、验证、QA）。
3. **PPT 原生元素重建**：
   - **文本**：所有可读文本默认原生文本框；禁止用生成图片承载可编辑文本，禁止隐藏/透明/1pt/画布外文本凑数。例外（logo 字标、产品包装品牌字、地图底图地名、UI 截图内不要求可编辑的小字、照片背景招牌、报纸/书页/代码纹理、OCR 置信度极低且不影响主意的微小文字）须在 `visual_inventory` 或 `asset_provenance` 说明。字号位置**不目测**：用 `text_hints.json` 的 `box_px` 与 `font_pt_if_cjk`/`font_pt_if_latin` 填入 `text_boxes`，并加 `"font_size_source": "measured"`（否则确定性构建器会做保守收缩使文字系统性偏小）；hints 不完备，漏检行要补、误合并行要改；同一层级文字统一一个字号（同 `size_group` 同号）；`fit_text` 保持开启作防溢出护栏。完成后记 `quality_checks.font_size_calibrated=true`。
   - **公式**：从源转写为 LaTeX，用 `cli.py formula render-latex <page_dir> --tex ... --out assets/xxx.svg --box ... --id ... --fragment ...` 渲染为图像资产（优先 SVG），把 fragment 的 `images`/`asset_provenance`/`formula_inventory` 合入 manifest。**禁止**用 Unicode 上下标或多个手写文本框拼公式，禁止用源图公式截图。无 TeX 引擎/转换器或编译失败时保持页面 failed 并记录公式 id、LaTeX 源、确切报错与所需修复；仅当用户明确批准省略该特定公式且条目记录 `user_approved_exception: true` + 具体 `approval_note` 时才可交付。
   - **结构原件**：直线/虚线/折线、矩形/圆角矩形/圆/椭圆、普通箭头与连接线、纯色卡片/面板/分隔线、表格/坐标轴/网格线、简单柱状图/进度条/状态色块、简单标注可用原生形状；原生形状只承载版式结构，绝不承载语义图标或视觉身份（圆圈里的 DNA/锁/靶心不是结构原件，回第 2 步）。
   - **圆角几何**：先分类 `straight`/`small-radius`/`large-radius`/`pill`；`straight` 用 `rect`，其余用 `roundRect` 并记录 `source_corner_radius_px`（对象级属性，8–12px 微圆角不得做成 70px pill；不确定就放大看并取较小半径）；普通矩形不得凭设计偏好加圆角。
   - **文字笔画与装饰拆分**：可读字符笔画只属于其原生文本框，不得再用形状重描；独立装饰线/分隔线/按钮下划线确认非文字后才可作形状。
   - **分组与层级**：保留分组关系（图标+圆形底、徽章+数字、气泡+文字等）；圆形底内居中文字复用底形的 `box_px` 加水平垂直居中对齐。推荐 z-index：净底 0、原生结构 10–20、分离前景资产 30、原生可编辑文字 40+、须压在文字上的圈/贴纸/手绘 50+；背景不得盖住文字，同一文字/图标/装饰不得同时出现在图片层和原生对象。

**区域分解与复合图（region-decomposition.md / object-routing.md）**：结构化页面先划 3–5 个语义区域（真正简单的页面可 1–2 个），逐区域清点对象再路由（原生文字 / 可测量原生形状 / 简单连接线 / 有界源忠实或无文字资产 / 混合重建），区域策略取值 `native-object-decomposition`、`mixed-reconstruction`、`bounded-local-asset`、`native-text-layout`；复合图（知识图谱、节点-连接图、工作流、矩阵）**不得整区用 `bounded-local-asset`**。复合图须按源像素逐个测量：每个节点圆心与宽高、真圆/椭圆/文档节点/复杂象形之辨、每条关系的端点/方向/线型/箭头归属、标签锚定对象、z 序与遮挡归属、实现它的 manifest id；每个节点和边都要进 `protected_visual_anchors`，去底不得擦除/裁切/移动/复制这些锚点。区域证据写入 manifest 的 `image2ppt_region_decomposition` 扩展块（是构建证据，不是控制器状态，也不是第二个重建计划——禁止创建 `source_reconstruction_plan.json`）。

**箭头规则（manifest-arrow-extension.md）**：细箭头 = 一条 `type: line` 连接线（`connector: straight|elbow|curve`，`start_arrow`/`end_arrow` 至少一个非 `none`，箭头 `arrow|triangle|stealth|diamond|oval`，宽长 `sm|med|lg`），后处理把它转为**一个** `p:cxnSp`；填充箭头 = 一个 Arrow AutoShape（`preset: rightArrow|leftArrow|upArrow|downArrow|leftRightArrow|upDownArrow|quadArrow|leftRightUpArrow|bentArrow|uturnArrow|stripedRightArrow|notchedRightArrow|chevron|homePlate`），居中标签文字写进同一对象的 `text`（并计入 `text_inventory` 供精确文本校验），不得另建重复文本框。手绘/带纹理/渐变/插画风格的箭头按前景资产走分离工作流。永远失败的拼法：线段+独立三角头、轮廓/填充/头/阴影拆成多对象、Unicode 箭头字形、多段线拼弯头、普通箭头用位图。

**manifest 字段契约（manifest-schema.md，新 manifest 一律 `schema_version: 2`）**：必含 `slide`、`content_box`、`source`（含 width_px/height_px）、`text_inventory`、`visual_inventory`、`background_strategy`、`quality_checks`、`quality_evidence`、`text_boxes`、`shapes`、`images`、`asset_provenance`、`page_strategy`。`visual_inventory` 每项为结构化对象，`kind` ∈ `background|foreground-asset|native-structure|formula`，`representation` ∈ `native|asset-sheet-separated|source-preserving-local-cleanup|imagegen|latex-rendered-formula`，且配对合法（background: native/source-preserving-local-cleanup/imagegen；foreground-asset: asset-sheet-separated；native-structure: native；formula: latex-rendered-formula）。`quality_checks` 四旗标（`font_size_calibrated`/`visual_inventory_matched`/`background_strategy_checked`/`shape_corner_geometry_checked`）须全 true，且每项在 `quality_evidence` 中有 ≥12 字符的具体 `observation`。所有 `box_px`/`points_px`/`polygon_px` 用 `source.png` 像素坐标（运行时映射进 `content_box`，不拉伸到整页）；每个定位对象必须有坐标，缺坐标即页面违约。`asset_provenance` 每个 `images[].path` 须有一条匹配记录：`source` 指向存在的源文件、`source_type` 五选一（`asset-sheet-separated`/`imagegen`/`latex-rendered-formula`/`user-provided`/`user-approved-rasterization`）、非空 `provenance_note`（`user-approved-rasterization` 另需 `approval_note`）。`formula_inventory` 每项是硬契约：标记 `failed`/`missing`/`blocked` 或缺渲染图/出处对的公式使页面失败，唯一例外是上述用户显式批准记录。

**页面收尾门（worker Prompt 末尾序列）**：`page build <page-dir>`（构建 page.pptx + preview.png，同目录暂存、成功后原子发布）→ `run_image2ppt_qa.py <page-dir>`（首次写出 `visual-review-evidence.template.json` 并保持 pending，返回码 2 且仅 pending visual review 属预期）→ 对照 `source.png` 与 `render/rendered.png` 逐区域目检（复合图查节点数/圆心/大小/圆形几何/边端点/方向/虚线节奏/标签锚/z 序；箭头查弯曲/头型/粗细/对象数/内嵌标签）→ 修复后把模板复制为 `visual-review-evidence.json`、置 `reviewed: true`、每项检查填具体观察（"looks good" 永不合格），再带 `--visual-review-status reviewed --visual-review-evidence ...` 重跑 → `page contact-sheet <page-dir>` 生成原图-预览对照图 → 确认 `validation.json` 顶层 `passed: true` 且 `image2ppt_profile.passed` 为 true → 写 `page_result.json`。证据文件须绑定当前 source/render 的 SHA-256 哈希并覆盖每项必需检查；`--visual-review-notes` 只是补充，不能替代证据文件。页目录最终须含：manifest.json、imagegen-jobs.json、page.pptx、preview.png、split_assets_contact.png、validation.json、page_result.json，外加 arrow_postprocess_report.json、arrow_inspection_report.json、region_decomposition_report.json、render/rendered.png、render_report.json、image2ppt_qa.json、visual-review-evidence.json。

**记录与修复**：标准验证 + 区域/箭头/渲染门全部通过后才 `run record <run> --page <id> --agent-id <id>`（校验产物、manifest 结构、哈希、顶层 `passed=true`；失败页绝不记录）。被拒/失败/取消/证实丢失的页用 `run reset`（已派发页需 `--agent-id` + `--confirm-lost`）→ 重新 dispatch → 重新 record 的同一生命周期修复；禁止手工编辑 `page_jobs.json`。

### 步骤 5：finalize 与最终复验

`run next` 报 `finalize` 时：`run finalize <run-dir>`（只接受全部已记录页，按页序从各页 manifest 重建整个 deck——`page.pptx` 只是页级可交付性产物，**不是**最终组装输入；恢复 speaker notes：有保留 XML 则还原 note slide，否则写保留的 note 文本；同目录临时文件构建、成功后原子发布、回写 `deck_manifest.json`）→ `run_final_image2ppt_qa.py <run-dir>` 两遍模板/证据流（首遍写 `final/visual-review-evidence.template.json`，逐页对照源与最终渲染填好 `final/visual-review-evidence.json` 后带 `--visual-review-status reviewed` 复跑）。最终 QA 报告须含：本地 deck 验证仍通过、页数与渲染数一致、speaker-note 哈希经本地验证器仍匹配（缺失或不匹配是硬失败）、所有箭头在重建后终稿中通过单对象检查、每个 manifest 通过区域/复合图检查、每张渲染幻灯片已与源对照、`final/image2ppt_qa.json.passed` 为 true。最终 QA 失败时通过同一生命周期修权威页面 manifest 后重新 finalize，**不得只手改终稿 deck**。

**交付**：返回最终 PPTX 路径、标准最终验证结果与 `final/image2ppt_qa.json`，并报告哪些复杂视觉仍以可替换位图资产保留；任何页面/最终门 pending 或 failed 时不得宣称 deck 完成。

## 四、输出契约与质量检查

**产出物**：对象级可编辑 `.pptx`（`final/origin_edited.pptx` 或 deck_manifest 记录的路径）；每页的 manifest、文字提示、渲染预览、QA 报告；最终文件验证结果与仍保留为可替换图片资产的复杂视觉清单。页面输出严格限制在各自页面目录，最终文件限制在本次 run 目录，原子写出避免半成品。

**关键文件与所有者**（workflow.md/manifest-schema.md）：`deck_manifest.json`（prepare 创建，finalize 读并写完成时间；含输入类型、页序、画布、图像后端契约、notes 路径、最终输出路径、原始用户请求）；`page_jobs.json`（唯一页面状态/派单/结果源）；`run_state.json`（运行历史）；`pages/page_NNN/manifest.json`（唯一页面构建输入，机器可读 v2 契约在 `schemas/page-manifest-v2.schema.json`）；`validation.json`（顶层布尔 `passed` 是记录门，补充 QA 在 `image2ppt_profile` 下原地扩展，不得另建 profile 专用结果文件替代它）；`page_result.json`（worker 返回契约，最小键集 page_manifest/imagegen_jobs/page_pptx/preview/contact_sheet/validation/page_result，全为页目录内相对路径）；`notes_manifest.json`（prepare 创建，finalize 读取，notes 不交给 worker、不翻译/摘要/改写/删除）。

**质量检查三层**：

1. **确定性本地验证**：`page validate`（与 `run record` 相同的 manifest 契约检查，worker 返回前先在页内修完）；缺 `box_px`/`points_px` 即页面失败；`validation.json` 顶层 `passed` 为 false 永不记录。
2. **Image2PPT 补充门**：箭头原子性检查（每个 profiled 箭头解析为恰好一个命名 PPT 对象）、区域/复合图检查（拒绝：结构化页无 3–5 区域、区域引用缺失对象、manifest 对象未分配区域、高风险区域无保护锚、复合图整区压成位图、节点/边测量与已审源测量不符、声明圆实现成畸变椭圆、缺方向箭头或虚线样式、复合节点/边无锚点）、渲染对照审查（哈希绑定的逐项证据）。
3. **最终门**：如上"步骤 5"的 7 项报告要求。

**Fix versus Warning 分界**（page-decision-tree.md）：自检任何失败项、输入无法规范化、无可构建 manifest/page.pptx 或 PPTX 打不开、文字字号位置可见偏差致拥挤/溢出/遮挡、公式无法渲染且无用户显式豁免——均为当前页硬修复，绝非 warning；分离资产的轻微线宽/抗锯齿/比例/阴影差异、非关键装饰的轻微视觉漂移、已记录的低风险字体差异才可作为记录在案的 warning 随当前 PPT 交付（前提仍是对象来源工作流已成功）。

## 五、关键规则与红线

1. **单一事实源**：`page_jobs.json` 是唯一页面状态源、`manifest.json` 是唯一页面内容源、`deck_manifest.json` 是最终组装源；只许用 CLI 的 `prepare`、`run next/dispatch/record/reset/hints/finalize` 与 page 命令推进状态；永不创建第二个 job 文件、重建计划、OCR 规范化器、页面控制器、打包器或 finalize 路径；补充 QA 只报告失败，永不改生命周期状态。
2. **目录边界**：页面构建/验证/hints/QA 只在所属页目录内读写；manifest 路径、资产、公式、报告、预览、`--out` 覆盖不得用 `..`、符号链接或绝对路径逃逸（唯一例外：`image import` / `process-sheet --asset-sheet-source` 的显式图像工具结果，先复制进页目录才成为构建依赖）；run 级 manifest 与最终输出限制在 run 目录内；任何边界拒绝都是硬失败，不得把被拒文件拷回冒充运行时输出。
3. **对象来源契约**：确定性验证通过（`validation.json.passed=true`）**绝不豁免**对象来源规则；每个非文本前景视觉必须走 asset-sheet 分离，无回退、无降级为 warning、无源图直接裁剪、无原生形状/emoji/Unicode 符号近似、无本地代码手绘。
4. **文本纪律**：可读文本一律原生可见文本框；字号位置以 `text_hints.json` 测量为准并标 `font_size_source: "measured"`；同层同号；不凭设计偏好放大标题/正文/标签。
5. **公式硬门**：缺引擎/转换器/编译失败保持页面 failed；唯一豁免是用户对该特定公式的显式批准 + manifest 记录 `user_approved_exception: true` 与具体 `approval_note`；内部决定、泛化 warning 或 `passed=true` 都不能制造豁免。
6. **箭头原子性**：一个简单箭头 = 一个 manifest 形状 = 一个 PPT 对象；细箭头用带原生箭头字段的一条连接线，填充箭头用带内嵌文字的一个 AutoShape；禁止线段+三角、Unicode 箭头、分组碎片。
7. **复合图纪律**：逐节点逐边源像素测量并记入 `protected_visual_anchors`；可测的简单圆/卡片/连接线保持原生；有界资产只用于真正复杂局部；不得从漂亮模板推断布局，行列规律只能交叉验证不能替代测量。
8. **净底忠实**：clean base 是"去掉待重建前景后的同一背景"，不是同主题新图；复杂背景净底须保持构图/透视/物体位置/颜色/光照/关键细节，"看着像"的近似图即使验证通过也是当前页修复项。
9. **视觉证据纪律**：视觉结论以绑定当前源/渲染哈希的逐项结构化证据为准，每条观察 ≥12 字符且具体；"看起来不错"与 `--visual-review-notes` 均不能替代证据文件。
10. **零第三方网络与密钥（本地版）**：不创建 `config.yaml`、不设置 `PADDLE_OCR_TOKEN` / `OPENAI_API_KEY` / `OPENAI_BASE_URL` / 任何 OAuth 或 bearer 凭据；不向 openai-compatible、PaddleOCR 云端或其他第三方端点发起请求；诊断输出中外部端点一项应永远为 unset。唯一允许的"生成"动作是宿主内置 `builtin-imagegen`；内置能力缺失时页面阻塞并报告，不得私自寻找外部替代。
11. **保持迁移前行为**：不为了证明运行时独立而重作已验收的基线页；多条路线都满足契约时取先前验证过的视觉策略；worker Prompt 双层不压缩不重述。
12. **不重写确定性动作**：能用 `image2ppt` CLI 完成的确定性动作直接调 CLI，不另写临时 Python 脚本。

## 六、配套资源

### references（11 件）

- `references/workflow.md`：run 契约——阶段-实现归属表、权威文件清单、本地入口命令、完整序列、7 条生命周期不变式、speaker notes 与最终组装规则、重试行为。
- `references/runtime-dependencies.md`：Python ≥3.10 与本地 requirements（pypdfium2/Pillow/NumPy/PyYAML/defusedxml 等；清单中的 openai/requests 即便随包安装，本地版也不配置任何凭据、不发起外部调用）、系统程序（Windows PowerPoint 自动化 / Linux·macOS LibreOffice、CJK 字体、可选 ImageMagick 与 TeX+dvisvgm/pdf2svg）、doctor 职责。
- `references/ocr-text-hints-contract.md`：**本地版仅参考其本地墨迹测量、文本过滤与 hints 产物部分**；其中 PaddleOCR token 配置、云端提交-轮询-下载等章节停用。
- `references/page-decision-tree.md`：页面对象决策的唯一权威——预清点、背景三步、前景分离与 asset-sheet 规则、原生重建（文本/公式/结构/圆角/笔画/分组层级）、最终自检、Fix versus Warning 分界。
- `references/manifest-schema.md`：`deck_manifest.json`/`page_jobs.json`/`page_request.json`/`page_result.json`/`validation.json`/`manifest.json`/`imagegen-jobs.json`/`notes_manifest.json` 八类 JSON 的所有者、用途与字段契约（含 v2 结构化清单、provenance、公式、圆角、字号拟合、对齐取值）。
- `references/region-decomposition.md`：语义区域两级路由、复合图测量清单、保护锚、`image2ppt_region_decomposition` manifest 扩展格式与区域审查门。
- `references/object-routing.md`：区域决策顺序、箭头决策顺序与源箭头→manifest 策略对照表、拼碎箭头的失败形态清单。
- `references/manifest-arrow-extension.md`：细连接线与填充箭头的 manifest 字段（connector/箭头端/宽长/14 种 preset/内嵌文字）与后处理命名规则。
- `references/assets-provenance-contract.md`：图像生产与页面组装分离、`imagegen-jobs.json` 记录规则、`asset_provenance` 五类 source_type、安全与网络边界；**本地版其网络边界收紧为"不产生任何外部图像请求"**。
- `references/qa-contract.md`：页面门必需与补充产物清单、箭头检查、区域/复合图检查、渲染审查与哈希绑定证据流、最终门 8 项要求。
- `references/cli-helper.md`：`image2ppt` 命令手册——命令树（setup/doctor/config/prepare/run next·status·backend·dispatch·record·reset·hints·finalize/page hints·build·contact-sheet·validate/image generate·edit·import·process-sheet/formula render-latex）与各命令用法示例。**本地版只用内置后端与本地处理子命令；`image generate/edit` 的外部 provider 参数一律不传。**

### prompts（2 件）

- `prompts/page-worker-base.md`：页面重建者基础 Prompt 模板——强制先读三参考、7 条硬规则、图像后端执行要点、6 步工作顺序、返回物清单与失败时的页面失败写法。
- `prompts/page-worker.md`：Image2PPT profile 增补层——追加四参考必读、9 条附加硬规则（区域分解、复合图测量、箭头单对象、公式硬门等），并替换基础层的收尾序列为"build → run_image2ppt_qa 两遍证据流 → contact-sheet"扩展序列。

### scripts（9 件，一句话功能）

- `scripts/build_page_worker_prompt.py`：从本地 base + profile 两层资源合成原始两层结构的页面 worker Prompt（打印 prompt_file 与 dispatch 命令模板）。
- `scripts/run_image2ppt_qa.py`：在标准产物上跑 Image2PPT 补充页面门（箭头/区域/渲染），管理 visual-review 证据模板与验收。
- `scripts/run_final_image2ppt_qa.py`：对 `run finalize` 产出的终稿做后处理与最终验证（页数、notes 哈希、箭头原子性、渲染对照）。
- `scripts/render_image2ppt_qa.py`：用 PowerPoint 或项目本地 LibreOffice 回退渲染 Image2PPT 输出。
- `scripts/inspect_arrow_atomicity.py`：验证每个 manifest 箭头映射为恰好一个命名 PowerPoint 对象。
- `scripts/inspect_region_decomposition.py`：对照标准 manifest 对象与源坐标验证区域分解证据。
- `scripts/postprocess_manifest_arrows.py`：把箭头扩展应用到 image2ppt 构建的 PPTX 上（本地构建器仍是唯一打包器，仅改写对应 slide XML 对象，保留其余包部件）。
- `scripts/visual_review_evidence.py`：创建并校验逐页视觉审查证据（SHA-256 哈希绑定）。
- `scripts/runtime_paths.py`：只解析捆绑在技能目录内的资源路径。

### cli / schemas / 配置 / 测试

- `cli/image2ppt/cli.py` + `cli/image2ppt/runtime/`（约 25 个模块）：唯一 CLI 入口与其运行时——输入规范化、prepare/finalize、页面状态机、文本 hints（**本地版仅本地 ink 几何测量**）、内置图像后端对接与导入/拆表、公式渲染、PPTX 构建与验证、去底色与 alpha 拆分等；构建器直接写 OOXML，不依赖 python-pptx。
- `schemas/page-manifest-v2.schema.json`：页面 manifest v2 的机器可读 JSON Schema（必含 15 个顶层字段；`visual_inventory` 项的 kind/representation 枚举等）。
- `config.example.yaml`：原始配置模板（含 `PADDLE_OCR_TOKEN`、`OPENAI_API_KEY`/`OPENAI_BASE_URL`/`IMAGE2PPT_IMAGE_BACKEND`/`IMAGE2PPT_IMAGE_MODEL` 等字段）。**本地版不复制为 `config.yaml`、不填写任何字段**；仅保留 `--image-backend builtin-imagegen` 这一命令行口径；云端相关字段视为不存在。
- `agents/openai.yaml`：display_name "Nature Image2PPT"，default_prompt 引导把幻灯片图片重建为高保真可编辑 PPT 并完成渲染 QA。
- `tests/`（约 28 个测试文件）：覆盖控制台可移植性、slide 尺寸、派发并发、公式渲染、独立性、输入规范化、隔离生命周期、manifest 箭头、迁移等价性、多 agent 后端、OCR HTTP、路径围栏、PDF 后端、质量契约、区域分解、渲染护栏、运行时路径/配置、脚本清单、状态端到端、结构重建、视觉审查证据/运行时等。
- `README.md`/`README_EN.md`：中英文使用说明（适用场景、典型请求、所需输入、工作方式、产出、依赖与边界）；`LICENSE`：MIT（实现同步自上游 Paul-Jeo/Image2PPT）。
