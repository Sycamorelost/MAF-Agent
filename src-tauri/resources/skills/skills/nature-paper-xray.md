# nature-paper-xray

## 一、定位与触发场景

**用途**：把一篇给定论文读到能"复述作者的推理"的程度——作者当初卡在哪、押了什么、哪个设计是承重的、公式在一组具体数字上长什么样、证据到底够到哪儿。交付物默认是一份结构化的 Markdown 长文（long-read）。README 中的比喻是：像一位研究者在白板前向同事讲解这篇论文——作者看到了什么、为什么相信、在哪犹豫、放弃了什么、哪些数字会改变结论。

**description 原文摘录**（SKILL.md frontmatter）：

> Read one supplied paper closely enough to reconstruct how its authors actually got there, rather than restating what the abstract claims. Use for 精读论文, 讲透这篇论文, 把这篇读懂, 作者为什么这么设计, 这个公式怎么推出来的, paper x-ray, and deep single-paper comprehension before building on, reviewing, presenting, or citing that paper. ... Delivers a structured Markdown long-read by default. Not a summary, not a translation, not a peer review.

**典型触发请求**（README）：

- "帮我精读这篇论文，我想知道作者当初是怎么想到这个设计的。"
- "这个公式每一项是什么意思？给我一个能自己算一遍的例子。"
- "这篇论文的第一张图想让我相信什么？站得住吗？"
- "它的基线对比公平吗？有没有只在测试集上挑超参？"
- "论文里哪些设计是承重的，哪些拿掉也不影响结论？"

**明确不做什么（边界）**：

- 只读不判：不评分、不排名、不判录用成熟度/新颖性/重要性——那是 `nature-reviewer`。
- 不提炼写作范式、不产出可复用行文模板（`nature-writing` 的职责）。
- 不做全文翻译或中英对照阅读稿（`nature-reader` 的职责）。
- 不检索文献、不核验引用、不替论文补引用（`nature-academic-search` / `nature-citation` / `nature-ref-verifier`）。
- 不代写正文；读出来的内容可作为 `nature-writing` 的输入。
- "逐节摘要"被明确定义为**失败模式**而非降级选项（"A section-by-section summary of the paper is a failure mode, not a fallback."）。

## 二、运作模式（加载/路由机制）

**SKILL.md 作为路由**：frontmatter 只有 `name` 和 `description` 两个字段，description 内嵌了触发关键词（精读论文、讲透这篇论文、paper x-ray 等）。正文包含：默认立场（Default stance）、7 步 Workflow、Paper-type adaptation、Boundaries、Output discipline。Workflow 第 4、5 步分别显式指向 `references/deep-reading-protocol.md` 和 `references/evidence-boundary.md`；paper-type 一节指向共享层 `../nature-shared/core/paper-type-taxonomy.md`。整个技能没有独立入口脚本，SKILL.md 本身就是执行契约。

**manifest.yaml 结构**（version 1.0.0，无 axes、无 fragments，是本批 6 个 skill 中最简的清单）：

- `always_load`：仅一项 `references/deep-reading-protocol.md`。
- `references.on_demand`（条件 → 路径）：
  - 当请求涉及"结果是否站得住 / 证据支持什么不支持什么"（数字、基线对齐、选择压力、泄漏、成本、范围）→ `references/evidence-boundary.md`；
  - 当论文类型不明确，或需在 algorithmic / methods / hypothesis / research / review 之间调整阅读重心 → `../nature-shared/core/paper-type-taxonomy.md`。
- 设计意图（manifest 注释原文）：每次阅读都需要 protocol 和重构立场，但对抗性证据检查与论文类型行动层按需加载——"a question about one formula should not load the whole audit list"，避免只为问一个公式就把整张审计清单全部加载。

**静态层与动态判断的分工**：本 skill 没有 `static/` 目录，没有 axis/fragment 机制；SKILL.md 本体即"常驻核心"（立场 + 7 步流程），deep-reading-protocol 常载，evidence-boundary 与论文类型表按需。

**对 nature-shared 的依赖**：仅一处——`../nature-shared/core/paper-type-taxonomy.md`（五类论文的标准词汇表：research / methods / hypothesis / algorithmic / review）。该文件说明判定触发词与 legacy 词汇映射（例如 legacy 词 "empirical study" 归入 research，"system" 归入 methods），用于在阅读重心需在不同类型间切换时提供一致分类口径。

## 三、核心工作流（实际功能流程）

按 SKILL.md 的 7 步 Workflow + deep-reading-protocol.md 整合：

**前置默认立场**（SKILL.md Default stance，贯穿全程）：方法节是教学顺序而非工作发生顺序；引言故事是事后编排的；贡献列表是写给审稿人的。决定论文的判断（作者注意到了什么、为什么信、放弃了什么）大多没被写下来，恢复它们正是任务。每张图都是被挑选的，先问作者想让读者相信什么，再找图内不支持该读法的细节。图往往自带反证：后交叉的曲线、宽过方法间差距的误差棒、对数轴掩盖的常数因子、单种子充当方差声明。说出机制，或明说论文没给机制；不得提供作者从未声明的动机。

**第 1 步：先读完整个来源**。读正文、附录、脚注、图表标题，以及用户提供的补充材料。附录常装着正文不肯印的东西：真实超参搜索范围、不好看的消融、审稿回复材料。正文与附录口径不一致（配置、数据集规模、指标不同）要**记录为发现**，不是抹平的转录错误。阅读顺序：正文 → 附录 → 带着已知结果回到引言，对比 framing 与实际证明的东西。读图与表时把它们当证据而非插图：先声明作者希望读者从中相信什么，再检查图内不支持该读法的细节。

`references/deep-reading-protocol.md` 进一步说明：打印顺序是教学顺序而非工作发生顺序；每读到一个分歧就记录下来，因为正文报告与附录不同的配置、数据集规模或指标本身就是发现，不是要抹平的转录错误。

**第 2 步：重构研究路径**。说出这项工作回应的具体失败是什么、作者在最强的证据上押了什么注。第一张图通常承载这个赌注，需要单独处理。重构检查清单（`references/deep-reading-protocol.md`）：

- 这项工作之前 SOTA 是什么、具体哪里坏了？
- 哪个 claim 是头条、哪个结果扛着它？
- 为了头条结果放弃了什么——普适性、成本、简化假设？
- 哪些选择被呈现为任意，实际上是被设定逼出来的？
- 什么会证伪中心 claim，作者跑了这个测试吗？
- 如果主结果只是某个超参的产物，它会在哪里露出来？

这六个问题来自 protocol 的 Reconstruction checklist，目的是把作者没写下来的决策压力显性化。

**第 3 步：区分承重与装饰**。对每个有区分度的设计选择，三选一判定并给出证据：

- **Load-bearing（承重）**——拿掉结果就塌；需消融、受控比较或附录结果支撑。
- **Contributing（有贡献）**——拿掉会明显削弱但不塌。
- **Decoration（装饰）**——没有它结果依然成立，或论文从未隔离过它。

论文没做隔离实验时，结论就是 "not established"，照实报告；不得从组件被呈现的显著程度推断承重性。

**第 4 步：把数学落到实处**（worked micro-example 标准，见 `references/deep-reading-protocol.md`）：

- 每个符号给出 shape、单位/含义、来源。
- 先说公式是干什么的，再给公式。
- 每个关键公式跟一个具体数字的 micro-example，小到几行能算完，且**必须算到底**——"Substituting gives" 不算 worked example。
- 宁用一个暴露整个机制的例子，不用多个重复定义的例子。
- 公式能退化为熟悉情形时，指出是哪个情形并展示退化过程。

**第 5 步：检查证据边界**（`references/evidence-boundary.md`，按需加载），六个子项：

1. **Number agreement**：正文/表格/图是否报告同值同精度；每个性能数字是否带取得条件（数据集划分、模型规模、运行次数）；误差类型与样本量是否说明；图与表不一致时两者都是发现，报告分歧而不是二选一。
2. **Baseline alignment**：同训练数据、同步数预算、同调参投入？基线是作者复现的还是从不同设置的论文抄的？基线报的是最优配置还是默认配置？基线原论文需要的 warmup/schedule/trick 是否应用？欠调基线不构成提升；缺基线预算说明是 gap 而非 pass。
3. **Selection pressure**：超参/checkpoint/prompt 是否在测试集上（或带着测试集知识）选的？报告结果是否是多次运行的最优且其余未提？评估指标是否是看哪个有利才选的？阈值/停止规则在什么数据上调的？
4. **Leakage**：预处理是否见到测试数据（直接或经由拟合统计量如归一化常数、词表、聚类中心）？训练/测试是否可能有近重复（近重复文档、重叠受试者、重复上传图像）？预训练组件的评测集是否可能在其预训练语料中？划分是随机的还是尊重科学问题要求的分组？
5. **Cost and scope**：算力、显存、延迟与部署形态；claim 在论文所跑规模 vs claim 暗示的规模是否成立；limitations 是结果支持的还是只是口头承认；证据覆盖哪个子群体/领域/区间，结论在哪里停止适用。
6. **Reporting gaps**：保留原文的情态强度——"may" 不是 "does"，"suggests" 不是 "shows"，"some settings" 不是 "all settings"，未报告的结果不是无效果的证据。论文没报告而结论需要的东西，写"论文没有报告"，不得用看着合理的数字、假设的默认值或相似论文的记忆值顶替；超出所给材料的判断必须标注为推断，并与原文所述可见地分开。

**严重度分级**（作为阅读观察报告，不作录用判断）：

- **Changes the conclusion**——头条 claim 不成立。
- **Weakens the conclusion**——方向成立但幅度或普适性不成立。
- **Limits the scope**——按字面成立，比读者设想的窄。
- **Presentation only**——证据没问题，表述不清楚。

**第 6 步：保留 hedging 强度**。同上 Reporting gaps 规则，贯穿全文执行。

**第 7 步：交付长文**，固定六段结构（README、SKILL.md 与 `references/deep-reading-protocol.md` 三者一致）：

1. 论文真正主张什么（一段，撇开宣传措辞）。
2. 重构出的研究路径：失败 → 赌注 → 赌注的证据。
3. 承重判定：逐组件，附证据。
4. 关键公式的可算 micro-example。
5. 证据边界：结果建立了什么、没建立什么。
6. 读完能带走什么、哪些问题仍开着。

同时**声明阅读范围**（body / appendix / supplementary 或明确子集），绝不暗示超出实际完成的覆盖度。

**论文类型适应**（SKILL.md Paper-type adaptation；类型不明时加载 `../nature-shared/core/paper-type-taxonomy.md`）：algorithmic 重公平比较纪律——基线是否像提案一样被认真调参、在哪里失败；methods 重复现性——换家实验室行不行、哪里没写明；hypothesis 重因果证据强度——能否排除替代解释；research 重发现了什么、观察能推广到多远；review 重领域如何组织、来源分歧在哪、什么仍开放。

**失败模式清单**（protocol，共 6 条）：用更长的话复述摘要；把引言叙事当作者真实推理复述；逐符号描述公式但从不算一遍；把贡献列表当作论文证明了什么的摘要；脱离取得条件报告结果；用貌似合理的假设填论文的坑而不点名坑。

`references/deep-reading-protocol.md` 还给出了交付物的六段式结构说明（deliverable shape），与 SKILL.md 第 7 步完全对应：先写论文真正主张什么，再写重构路径、承重判定、可算 micro-example、证据边界，最后写带走什么与仍开放的问题。

## 四、输出契约与质量检查

- **交付物**：一份 Markdown 长文，按上述六段固定顺序。
- **文件命名**：未强制约定特定文件名，输出即最终交付；阅读范围必须在文中显式声明。
- **语言纪律**：用用户所用语言写作；中文请求产出中文行文，英文技术术语原样保留。
- **引用纪律**：只在锚点必需处引用原文，不大段复制论文。
- **公式格式**：行内 `$...$`，独立行 `$$...$$`；绝不放入反引号或代码块。
- **诚实性自检**：论文长不是降级为摘要的理由；阅读不完整就写明停在哪里；必须声明覆盖范围（正文/附录/补充材料）。
- **质量检查方式**：无脚本、无自动化验证；质量通过第 1–7 步流程纪律、承重判定三档、证据边界六子项、严重度四级与失败模式清单共同约束。
- **验证脚本**：无。本 skill 目录下没有 scripts/、tests/、evals/，质量完全靠流程纪律与失败模式清单约束。

## 五、关键规则与红线

1. 论文没报告的事就写"论文没有报告"，不用常识补一个看着合理的数字（README 红线，与 evidence-boundary 一致）。
2. 不得提供作者从未声明的动机（"Name a mechanism or say the paper does not name one. Do not supply a motive the authors never stated."）。
3. 情态强度不可升级：may≠does、suggests≠shows、some settings≠all settings。
4. 推断必须与原文可见地分开，并显式标注为推断。
5. 正文与附录/图/表不一致时，报告分歧，不得自行选一个抹平。
6. 每一张图都是被选出来的：先说作者想让读者相信什么，再找图内不支持该读法的细节（图常自带反证：后交叉的曲线、宽过方法间差距的误差棒、对数轴掩盖的常数因子、单种子充当方差声明）。
7. 方法节是教学顺序不是工作顺序；引言故事是事后编排的；贡献列表是写给审稿人的——不把这三者当事实来源。
8. 不评分、不排名、不做录用判断；severity 只作为阅读观察。
9. 绝不暗示超出实际完成的阅读覆盖度。

## 六、配套资源

- `references/deep-reading-protocol.md`（always_load）：深读协议——阅读顺序、worked micro-example 标准、承重/贡献/装饰三档判定、重构检查清单、六段交付结构、失败模式清单。
- `references/evidence-boundary.md`（on_demand）：证据边界审计——数字一致性、基线对齐、选择压力、泄漏、成本与范围、报告缺口、四级严重度。
- `../nature-shared/core/paper-type-taxonomy.md`（跨 skill 共享，on_demand）：五类论文标准词汇表及判定指引、legacy 词汇映射。
- `README.md` / `README_EN.md`：面向用户的用途、典型请求、需提供材料（论文本体、附录、阅读目的、可选官方代码仓库——代码会暴露论文没写的归一化/预热/数据过滤）、产出结构、边界与相关技能对照表。README 中的"适合用它做什么"包括组会/读书会/答辩前真正读懂并讲得出来、在别人工作上继续做前先弄清缺口、吃透方法论文公式、看清结果漂亮的论文的对照条件与适用范围、审稿或写相关工作前理清机制与证据边界。
- `agents/openai.yaml`：展示名 "Nature Paper X-Ray"，default_prompt 为 "Use $nature-paper-xray to closely read this paper and reconstruct how its authors actually got there."
- 技能根目录无独立 evals 文件，质量由内部流程与清单约束。
- `README.md` 中的相关技能对照表：`nature-reviewer`（审稿结论与评分，本 skill 只提供输入）、`nature-reader`（全文翻译与中英对照）、`nature-writing`（起草手稿章节）、`nature-academic-search`（检索相关工作）、`nature-ref-verifier`（核验引用元数据）。
- `README_EN.md`：英文版使用说明。
- 无 scripts/、tests/、evals/、templates/、static/；质量完全靠流程纪律、检查清单与失败模式清单约束。

---

**元信息**：技能名 `nature-paper-xray`，version 1.0.0；本批 6 个 skill 中结构最简（无 axes、无 fragments、无 static 层）。
