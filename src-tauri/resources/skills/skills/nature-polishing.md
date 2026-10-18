# nature-polishing

## 一、定位与触发场景

- **用途**：对**已有**学术文稿做润色、学术翻译（中译英）或精简，保持事实、术语和证据边界不变；也覆盖 manuscript LaTeX 排版修复（layout/typesetting）。
- **description 原文摘录**（SKILL.md frontmatter）：

  > "Polish, translate, or tighten existing academic prose while preserving facts, terminology, and evidence boundaries. Use for 论文润色、学术翻译、正文精简, or manuscript LaTeX layout fixes. Use nature-writing when the main task is drafting new sections or rebuilding the manuscript argument."

- **典型触发请求**：
  - "把这段中文结果翻译成 Nature 风格英文，保持克制。"
  - "润色 abstract，不要改变事实和引用意图。"
  - "这段 introduction 太像 AI 写的，帮我重构逻辑和语言。"
- **明确不做什么（边界）**：
  - 不从零起草章节、不重建论文论证——那是 `nature-writing` 的活。
  - 不替作者新增结果、机制、统计意义或未给出的引用；不为更像 Nature 而夸大 novelty、causality、generality。
  - 段落的结构问题若不改无法在不虚构内容的前提下修复，则在 `Revision notes:` 中**标记而非掩盖**（"flag it instead of papering over it"）。
  - **排版请求走旁路**：松版/空页、 stranded headings、图不填满页或跨页、"Float too large"、多 panel 排布、SI 空页等**跳过四个 prose 轴**，直接加载自包含的 `references/latex-layout.md`。

## 二、运作模式（加载/路由机制）

### SKILL.md 作为路由

frontmatter 仅 `name` + `description`。正文为 5 步路由协议：

1. 读 `manifest.yaml` + `always_load` 全部文件（默认 stance、failure-mode 诊断、ethics、输出格式）。
2. 逐轴判定取值（用 manifest 的 `detect:` 提示 + 用户输入），并**先用一行告知用户检测到的轴值**，可低成本纠正；这是 progress update 而非 approval gate。
3. 只读被选中轴值映射的 fragment 文件；"Do **not** read every fragment in `static/`"。`section` 轴仅在用户提供无章节语境的独立散文时可跳过。
4. 按优先级润色：paper-type playbook → section 任务与 failure mode → journal 约束 → language 句子规则（最后）→ core stance/ethics 全程。
5. `references/` 按 manifest 的 `references.on_demand` 表按需打开。

### manifest.yaml 结构（version 6.6.0）

- **always_load**（7 项，含 4 个共享层文件）：
  - `../nature-shared/core/reader-workflow.md`（5 步读者问题序列）
  - `../nature-shared/core/paper-type-taxonomy.md`
  - `../nature-shared/core/ethics.md`
  - `../nature-shared/core/terminology-ledger.md`
  - `static/core/stance.md`、`static/core/failure-modes.md`、`static/core/output-format.md`
- **axes**（4 个）：
  - `paper_type`（multi: false，default: research）：`research` / `methods` / `hypothesis` / `algorithmic` / `review` → `static/fragments/paper_type/<值>.md`
  - `section`（**multi: true**，无 default）：`abstract` / `intro` / `results` / `discussion` / `conclusion` / `title` / `methods` → `static/fragments/section/<值>.md`；不清楚时先问用户
  - `language`（default: en）：`en` → `language/en.md`；`zh-to-en` → `language/zh-to-en.md`（中文原文或明显中式英语结构时用）
  - `journal`（default: generic）：`nature`（仅旗舰 Nature）/ `nat-comms` / `nat-mach-intell`（NMI）/ `generic` → `static/fragments/journal/<值>.md`；其他 Nature Portfolio 子刊**不得**误走旗舰 Nature 规则
- **references.on_demand**（条件→路径，含共享层）：
  - Results/全文主文压缩、main-vs-SI 分配、防 revision 堆积、统计/claim 重复 → `../nature-shared/core/main-text-discipline.md`
  - Nature Portfolio 目标的 Results/Discussion 润色（claim 递进、局部解释、必要回顾 vs 重复论证）→ `../nature-shared/core/nature-results-discussion.md`
  - 任何 Discussion 润色（反向漏斗、情态动词校准、局限具体化）→ `../nature-shared/core/discussion-argument-language.md`
  - Nature Portfolio Introduction/全稿叙事 → `../nature-shared/core/nature-introduction.md`
  - Nature Portfolio Abstract → `../nature-shared/core/nature-abstract.md`
  - Nature/Nat Comms 文章级模式（abstract/intro/Results 开场/Discussion/conclusion/title）→ `references/published-article-patterns.md`
  - 2025 Nat Comms 用词校准（连接词频率、significantly 红线、hedge/booster/时态）→ `references/nat-comms-2025-diction.md`
  - 章节 move 顺序与短语模式 → `references/section-moves.md`
  - hedging/过渡/证据/局限/未来工作短语 → `references/phrasebank-playbook.md`
  - 学术风格、register、冠词、机械检查 → `references/style-guardrails.md`
  - 全文一致性审计/术语单位数值漂移 → `../nature-shared/core/consistency-sweep.md`
  - 更深的写作策略原则 → `references/writing-strategy.md`
  - LaTeX 排版问题 → `references/latex-layout.md`
  - NMI 的精确 content-type、字数、data/code、投稿与生产要求 → `../nature-shared/journal-formats/nature-machine-intelligence.md`

### 静态层与动态判断分工

静态 core 管"立场+诊断+输出格式"；四个轴的 fragment 管"论文类型架构 / 章节任务 / 语言句法 / 期刊约束"的组合；共享层提供 Nature 语料归纳的段落级写作默认（声明为 corpus-derived guidance 而非官方政策）。

### 对 nature-shared 的依赖

`core/reader-workflow.md`、`core/paper-type-taxonomy.md`、`core/ethics.md`、`core/terminology-ledger.md`、`core/main-text-discipline.md`、`core/nature-results-discussion.md`、`core/nature-introduction.md`、`core/nature-abstract.md`、`core/discussion-argument-language.md`、`core/consistency-sweep.md`、`journal-formats/nature-machine-intelligence.md`、`journal-formats/nat-comms.md`（nat-comms fragment 要求先读）。

## 三、核心工作流（实际功能流程）

### 总流程（SKILL.md 第 4 步）

按 `core/failure-modes.md` 的优先级链执行修复：

`paper type -> section job -> evidence placement -> paragraph necessity -> claim/evidence/boundary -> sentence polish`

**禁止**对 section job 都错的稿子做句子级润色——先暴露结构问题。动手前先诊断主要 failure mode：paper type 逻辑错误、缺 gap 或定位差、有 claim 无证据、有证据无明确 claim、缺边界/局限、Results 与 Discussion 混杂、title/abstract 信号弱、跨章节术语不一致、Results 证据完整但主文臃肿、审稿人驱动的只增不删、Results 与图注重复同一统计报告、纯句级杂乱。

首次接触稿子即建 **Terminology Ledger**，术语/缩写/单位/符号全程一致，**不得为变化文风而引入同义词**。

### 各 paper_type fragment 要点

- **research**：沙漏结构（Introduction 由宽到窄，Discussion/Conclusion 再放宽）；建议写作顺序 Results → Intro+Conclusion → Title → Discussion → Methods → Authors → Abstract。
- **methods**：Results 必须展示方法相对现有方法的优势（更可靠/更快/更省资源/公平可复现比较）；Methods 需含公理条件假设、软硬件环境、数学推导、评测协议、数据集/baseline/指标/split/超参。写作顺序 Methods → Results → Intro → Conclusion → Discussion → Abstract。
- **hypothesis**：假设须显式可定位（不能埋在引言第三段）；给出证伪路径；区分 supporting 与 consistent-but-non-discriminating 证据；Discussion 先处理 rival explanations 再推广；hedging 强度匹配因果声称强度，相关数据不得过度声称机制。
- **algorithmic**：分离"系统是什么/为何有效/效果如何"三股线；性能声称必须带数据集、指标、baseline、条件；避免 marketing verbs（`leverages`/`enables`/`empowers`）；Discussion 要点名实验暴露的 failure modes 而非只报喜。
- **review**：有明确 scope 声明；按论证而非按论文组织文献（替换"Author A reported X. Author B reported Y."）；综述可以有立场但要给出推理；避免 `furthermore`/`additionally`，用能信号逻辑关系的连接词（`in contrast`、`building on this`）；结尾给读者一张可用的地图而非复述。

### 各 section fragment 要点

- **abstract**：是最短证据链 `precise problem/gap -> answer-enabling design -> main discovery -> decisive support/boundary -> implication`；删掉标题已暗示的背景；gap 与贡献各一句可定位；围绕一个主 claim + 至多 1-2 个决定性支撑/边界；方法细节仅保留到"解释问题为何可回答"的程度；数字仅在其定义/支撑/界定主发现时保留；末句讲意义而非重复结果。
- **intro**：收敛漏斗 `important problem -> specific phenomenon -> what prior work establishes -> exact unknown -> research question -> study route`；文献服务于 known–unknown 转换而非编目；novelty 从问题与设计中长出来而非形容词；不总结 Results/Conclusion。常见失败：开头像教科书、gap 从未明言、缺"已知→本文"转换、方法细节提前、把 gap 定义为"缺作者的方法"、背景不收窄到 Results 回答的问题、结尾列贡献而不预告 question–answer 路线。
- **results**：是建立并推进 claim 的证据链；主体过去时；报告"观察到什么、在什么条件下、有什么定量支持"；统计正确而克制；核心发现留主文，非中心的稳健性/异质性/provenance/替代推断/边缘情况移 SI（除非改变结论）；主要描述量与主推断支撑只报一次；每个证据单元以其支撑的有界推断收尾；保留直接回答当前实验的局部解释，跨结果综合与广泛含义移 Discussion。句子语法：`was detected`/`increased`/`showed`/`enabled`/`achieved`；解释性语法（`may reflect`/`suggests that`）仅在有意识、有证据、有校准时用。重组后对每个 Results 段落做 paragraph necessity test 并记录 retained/compressed/relocated/deleted。
- **discussion**：加载共享层 `discussion-argument-language.md`；回答"如何嵌入领域/增加了什么理解/归功前人/支持复杂化还是修正前人/如何解读/何时解读失效"；可用简短 central-finding anchor 开场后跨 Results 综合，**不得回放**每个 claim 的比较、效应量、统计检验。短规则：`Results = 证据链`，`Discussion = 这些 claim 合起来意味着什么`。句子语法：`may reflect`/`suggests that`/`could indicate`/`is likely due to`/`may facilitate`；不得把"consistent with"提升为"demonstrates"。常见失败：重述 Results、跳过 rival explanations、省略边界、窄结果直接跳到领域级含义、堆叠 hedge 或使用超出设计允许强度的 `must`/`cannot`/`demonstrates`/因果动词、只点名局限而不说对 claim 的后果、给出不解决已识别不确定性的泛泛 future work。
- **conclusion**：三段式收束——重申中心贡献、总结关键证据/结果、带边界地陈述含义；不得引入新数据；必跑 overclaim check（每个 claim 可回溯到本文证据？`demonstrates`/`proves`/`establishes` 有相应设计支撑？含义范围 ≤ 证据范围？）。
- **title**：`curiosity with credibility`；要可预期、少术语、可检索、有数据支撑；若需备选，给 3-5 个候选（declarative/question/finding-led）并标出最可辩护者；核对标题中每个定量声称。
- **methods**：specific/complete/transparent/reproducible；禁用模糊短语清单——`under standard conditions`、`using routine methods`、`data were analyzed statistically`、`differences were significant`、`samples were randomly assigned`、`the method was validated`——全部替换为实际可复现信息；引用早前报告缩写仅在该报告确有所需细节时可用。

### language fragment 要点

- **en**：句子目标 10-30 词、硬上限 ≤30 词；除标题/标签/固定术语外不写 <10 词整句；>20 词须检查是否含多个主命题，拆分超载句；每段一个 controlling idea + 支撑；段末句常最长最弱须显式检查；**默认不用 em dash**（改逗号/括号/短句），冒号仅在结构上有明确价值时用。
- **zh-to-en**：不逐句对译。流程：1) 先抽取核心命题用平实英文列出；2) 重建显式逻辑链（对比/因果/含义/局限——中文学术常省略连接词，须补回）；3) 对照原文核对术语、因果、hedging 强度；4) 技术术语、基因/蛋白名、模型名、数据集名、统计术语保持稳定；5) 逻辑重建后才应用 en.md 的句段规则。常见中式问题：topic-comment 链改主谓句、逗号串短句须拆分或加连接词、`many studies have shown` 类泛化改具体引用或删除、hedging 不对称（中文常 understated，Nature 风要求精确匹配证据强度）、话题名词重复处英文应用代词或省略。

### journal fragment 要点

- **nature（旗舰）**：多学科宽读者，首段与摘要须让外行抓住 claim/gap/意义；避开 Nature News 头条都不会出现的术语；长度纪律严苛——**prefer cuts to compressions**；正文不用 em dash；hedging 避免 overclaim（`proves`/`definitive`）与怯懦（`might possibly suggest`）两端；若依赖外行无法跟随的专门机制，**标记给作者而非过度转译**。
- **nat-comms**：先读共享层 `journal-formats/nat-comms.md` 的格式事实。要点：**~5,000 词上限含 Methods**；display items >10、参考文献 >~60、abstract >150 词或含引用/结构化——都要先诊断并在 `Revision notes:` 标记，而非默默删减；实质删 Results/Methods/论证需先获批准。
- **nat-mach-intell（NMI）**：先读共享层 NMI 格式文件与对应 Nature 语料 guidance。Article 检查：Introduction+Results+Discussion ≤3,500 词（不含 abstract/Methods/references/legends）；abstract ≤150 词且无引用；图表合计 ≤6；introduction 无小标题，Results/Methods 可有、Discussion 不可有。claim 校准：benchmark-only 的一般智能/部署就绪声称改为评测范围；区分架构收益与数据/监督/算力/调参收益；点名稳健性与迁移边界；不把关联/回顾性/仿真结果转成因果、临床或社会有效性。保留会议论文实质扩展的显式披露、Data Availability 与独立 Code availability 节；代码是核心结论时标记缺失的 reviewer access 或 Software Submission Checklist；不套用旗舰 Nature 的标题/Methods/图注限制（图注以 2018 NMI 300 词为历史参考上限，目标 150-250 词）。
- **generic**：未指定期刊时用 Nature 倾向的默认但不强制最严长度/意义框架；用户后来指定期刊时套用对应 fragment；只在缺失信息会改变科学含义或必需交付物时提问，不搞 intake checklist。

### 共享层触发规则（SKILL.md 第 4 步内嵌）

- Results、全主文压缩、main-vs-SI 分配、revision 新增文字 → 句子润色前先加载 `main-text-discipline.md`：逐条分类结果、保留最短充分证据链、**每处新增必触发受影响段落的删除/替换检查**。
- Nature Portfolio 目标 → 按章节加载 `nature-results-discussion.md` / `nature-introduction.md` / `nature-abstract.md`，保留 claim 递进、快速问题漏斗、Intro–Results 对齐、发现中心摘要压缩、证据绑定局部解释、跨 Results 综合；这些是 corpus-derived guidance，与期刊现行规则冲突时服从期刊。
- 任何 Discussion 任务 → 加载 `discussion-argument-language.md`，用其功能标签删 Results replay、修复从具体发现有界含义的推进、校准情态与报告动词、让局限与未来工作解析到具名 claim 边界。
- NMI 且涉及精确限制/availability/会议扩展披露/生产检查 → 加载 `journal-formats/nature-machine-intelligence.md`。
- 全文一致性审计或局部漂移迹象 → 加载 `consistency-sweep.md`；局部后续修改只查受影响术语/数字/claim，不每轮都重启全量审计。

### LaTeX 排版旁路（references/latex-layout.md）

诊断工作流：渲染 → contact-sheet → 读 log；处理松散 float 页（顶部对齐 glue）、宽扁图永远填不满竖版页（从源头重新生成更高的图）、禁止转 landscape（Nature house style）、float 积压导致 stranded headings、`[H]`/`\clearpage`/`placeins` 的锋利边界、多 panel 主图"stack, don't cram"、页数不是指标饱满度才是。**必须渲染并目检前后页面，绝不只看 .tex 判断版式**。

## 四、输出契约与质量检查

默认输出（`static/core/output-format.md`）：

1. 润色后正文以普通散文给出，**不放代码块**；
2. `Revision notes:` 附 3-5 条要点，说明主要结构与风格改动；
3. 若改动了章节逻辑须明确声明；
4. 用户要求对照时给 `Original` / `Polished` / `Why changed` 三段；
5. 无法在不虚构内容下修复的段落结构问题，写进 `Revision notes:` 而非掩饰；
6. 触发 main-text discipline 时附加紧凑的 `Main-text discipline audit:`：标明保留/替换/压缩/迁至 SI 或图注/删除的内容，声明哪些描述量与主推断量留在主文，报告前后词数；**不得让 audit 淹没润色正文**。

无独立验证脚本；质量靠诊断顺序、术语台账、一致性扫描与各 fragment 检查项保障。

## 五、关键规则与红线

1. 不虚构数据、引用、机制、novelty 声称；不让 AI 从零代写论文核心科学论证；论证弱就暴露，不用漂亮语言掩盖（`core/stance.md`）。
2. 语言服务论证：论证断了不润色句子；诊断顺序先于编辑。
3. 术语一致：首接触即建 Terminology Ledger，全程执行 canonical form，不为文风变化引入同义词。
4. 默认避免 em dash；hedging 强度匹配证据强度，两头都不行（overclaim 与 timidity）。
5. 期刊路由红线：`nature` 仅旗舰；其他 Nature Portfolio 子刊走 `generic` 并查该刊现行指南；NMI 独立路由。
6. 长度诊断是 surface 而非 silent cut：超限/错结构写进 Revision notes，实质删减须批准。
7. 排版任务必须渲染目检前后页面，不凭 .tex 判断。
8. 共享层 Nature 写作默认是 corpus-derived guidance 而非官方政策，与期刊现行规则冲突时服从期刊。

## 六、配套资源

- **references/writing-strategy.md**：写作策略层——core stance、沙漏结构、写作顺序≠阅读顺序、claim/evidence/boundary、章节职责、引用作为定位、对前人公平、overclaim 控制；管段落与章节级改写。
- **references/published-article-patterns.md**：从已发表 Nature/Nat Comms 文章归纳的 abstract/intro/Results/Discussion/conclusion/title/句子级模式及 overclaim 检查。
- **references/section-moves.md**：基于 Academic Phrasebank 的各章节 move 顺序与短语级支持（须在主章节逻辑已定后使用）。
- **references/phrasebank-playbook.md**：按功能分族的短语库——evidence strength/collocations、过渡族、段落衔接、gap 语言、与前人比较、局限、含义、未来工作。
- **references/style-guardrails.md**：主改写后的机械与风格检查——谨慎精确文风、避免缩略式与反问句、缩写首用定义、默认英式拼写、Nature 风图注 ≤300 词、标题 ≤75 字符、冠词用法等。
- **references/nat-comms-2025-diction.md**：基于 20 篇 2025 年 Nat Comms CS/AI 文章的用词频率校准数据（连接词偏好序如 However≫Furthermore、significantly 红线、hedge/booster/时态选择），是校准数据而非盲用规则，且禁止照抄源文措辞。
- **references/latex-layout.md**：自包含的 LaTeX 排版诊断与修复手册（渲染→contact-sheet→读 log；float glue、`[H]`、placeins、宽图源头重生成、多 panel 堆叠）。
- **agents/openai.yaml**：display_name "Nature Polishing"，default_prompt 引导改成 concise Nature-leaning English 且不改事实含义。
- **README.md / README_EN.md**：用户向说明（含 NMI 独立路由、Results 主文必要性审计、Discussion 四类功能检查等能力清单，方法来源与相关技能 nature-writing/nature-response/nature-statistics）。
- 本 skill 无 scripts/、templates/、examples/、evals/ 目录。
