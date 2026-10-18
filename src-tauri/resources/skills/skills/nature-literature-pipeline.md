# nature-literature-pipeline（本地版）

> **本地化改造说明**：本文档在原 skill 重构基础上做了"零外发"改造——删除飞书 / Telegram / 邮件 / 任意消息平台的推送环节与 `send_message` 调用、删除 chat ID 等投递目标配置；管线的检索、评分、精读、归档引擎保持不变。所有产物只写入用户指定的本地目录，由用户自己打开阅读。在线检索（arXiv/OpenAlex/Crossref/Semantic Scholar）属于**拉取公开元数据**，不向任何服务上传用户数据，保留不变。

## 一、定位与触发场景

**用途（本地版）**：一条完整的自动化文献发现管线——多源检索 → 六维评分粗筛 → 精读 → **生成格式化的本地日报 Markdown 文件** → 本地归档。可手动随时触发，也可由本机操作系统的计划任务定时触发（仅执行本地脚本、只写本地文件，**不向任何第三方软件推送**）。与一次性检索的区别：回答"持续帮我盯"，而不是"现在帮我找"。

**原 description 摘录**（SKILL.md frontmatter，保留以记录原貌）：

> Complete automated literature discovery pipeline: multi-source search → six-dimension scoring → fine reading → formatted delivery → archival. Combines a configurable engine with daily cron-driven application layer. Works with Feishu, Telegram, or any messaging platform.

**本地版对 description 的执行口径**：`delivery` 一律解释为"写入本地日报文件"；不配置、不询问、不调用任何消息平台；`cron` 仅可指代本机调度器触发一次本地运行。frontmatter 中 `license: MIT`、`metadata.author: Jiahao8595`、`hermes.tags`、`related_skills` 为原始元数据，本地版不依赖 Hermes。

**典型触发请求（本地版话术）**：

- "每天早上帮我跟踪海洋混凝土氯离子扩散和机器学习方向的新论文，日报写到 D:\Literature\daily。"
- "把过去 7 天候选论文按六维评分筛到 Top 5，生成今天的本地日报并归档。"
- Quick Start 话术改为："My research area is [X], keywords: [Y], write the daily digest to [本地目录], archive to [本地归档路径]"。

**明确不做什么（边界）**：

- 评分不是论文质量的最终判断；高分候选仍需人工阅读确认。
- 无法访问全文时明确标注 `Abstract only` 或 `Metadata only`，不假装读过全文。
- **不向飞书、Telegram、邮件或任何第三方软件/平台推送消息**；不调用 `send_message`，不维护 chat ID / bot / webhook 配置。
- 自动运行只依赖本机调度器（Windows 任务计划程序 / macOS launchd / Linux cron），机器关机则错过执行，下次运行补写本地文件即可。
- **只写用户指定目录下的 `raw/` 文献目录与 `daily/` 日报目录**；未经用户批准绝不修改其他笔记库/知识库。
- 一次性检索与引用核查是 `nature-academic-search`；CNS 引用导出是 `nature-citation`；长期库管理由用户自行选择（本地版不连接任何文献管理软件）。

## 二、运作模式（加载/路由机制）

**SKILL.md 作为路由**：frontmatter description 内嵌管线定义；正文给出五段管线图、Quick Start、两层架构表（Engine / Application）、可配置项、Safeguards、References、Pitfalls。本地版把 Application 层的"投递"全部替换为"本地文件生成"。

**manifest.yaml 结构**（version 1.0.1，无 axes、无 fragments、无 static；本地版加载口径如下）：

- `always_load`：仅 `SKILL.md`。
- `templates.on_demand`：配置关键词/评分权重/**本地日报输出目录**/归档路径/可复用 profile → `templates/literature-push-template.md`（模板中 delivery 段按本地版口径执行，忽略任何 target/chat 字段）。
- `references.on_demand`（条件 → 路径；本地版）：
  - 排名候选、校验分数算术、调权重、解释六维评分 → `references/scoring-system.md`（不变）；
  - 识别研究空白、建综述地图、把发现转成研究方向 → `references/gap-analysis.md`（不变）；
  - 写或校验带 YAML frontmatter 的归档笔记 → `references/note-template.md`（不变）；
  - 格式化本地日报 Markdown → 原 `references/push-format.md`，**仅借用其字段结构与合成示例**，删除文末 send_message/Delivery 段，输出对象改为本地文件（见第三节④）；
  - 配置本机计划任务 → 原 `references/cron-setup.md`，**仅借用"创建后读回验证、手动跑一次验证"的纪律**；本地版的任务动作是"本地运行 + 写文件"，不包含任何发消息、chat ID、投递日志；
  - 把多日笔记编成综述/related-work/集中阅读报告 → `references/review-compilation-workflow.md`（盘点范围改为本地归档目录全文检索，见附属流程 C）。

**两层架构（本地版）**：

| 层 | 用途 | 本地版文件口径 |
|---|---|---|
| Engine | 评分、分类、笔记模板、空白分析 | scoring-system.md / gap-analysis.md / note-template.md（原样使用） |
| Application | 定时触发、**本地日报生成**、归档流程 | push-format.md 只取字段结构；cron-setup.md 只取本地调度纪律；review-compilation-workflow.md 只盘点本地目录 |

**对 nature-shared 的依赖**：无。

## 三、核心工作流（实际功能流程）

五段管线（本地版流程图）：

```
手动触发 或 本机计划任务（每日 e.g. 08:30，仅本地执行）
  ├─ ① SEARCH (30 candidates)      arXiv / OpenAlex / Crossref / Semantic Scholar（拉取公开元数据，自动降级）
  ├─ ② COARSE FILTER (30 → 5)      六维评分：topic×35 + methodology×20 + journal×15 + network×10 + applied×10 + archival×10
  ├─ ③ FINE READ (top 5)           摘要级或全文级；来源分级 Full-text / Abstract only / Metadata only
  ├─ ④ GENERATE DIGEST（本地）      生成 {输出目录}/daily/YYYY-MM-DD_文献日报.md：🏅rank | title | journal | ⭐score | 💡一句话 | 🔬方法 | 📊关键结果 | 🧭点评
  └─ ⑤ ARCHIVE（本地）              DOI/arXiv 去重 → 分类 → 写笔记 → 更新本地索引
```

### ① 检索（SEARCH）

- 来源：arXiv、OpenAlex、CrossRef、Semantic Scholar（可选，无 key 或被限流时优雅降级，自动切到 OpenAlex + Crossref + arXiv）。均为**只读取公开元数据的出站查询，不携带、不上传任何用户私有数据**。
- 候选池默认 30 篇；lookback 可配（模板默认 7 天）。
- 去重（三重）：DOI → arXiv ID → OpenAlex ID →（模板追加）规范化标题；保留元数据更全的记录与全部稳定链接。

### ② 粗筛（COARSE FILTER，30 → 5，六维评分）

`references/scoring-system.md`，每篇 0–100 分：

| # | 维度 | 权重 | 衡量什么 |
|---|---|---|---|
| 1 | Topic Match | 35 | 与核心研究问题的贴合度 |
| 2 | Methodological Value | 20 | 方法质量与可借鉴性（实验设计、分析技术） |
| 3 | Journal/Source Quality | 15 | 期刊声誉、引用影响、可信度 |
| 4 | Research Network Relevance | 10 | 与被追踪作者/机构/活跃合作的关系 |
| 5 | Applied/Engineering Value | 10 | 实用价值：protocol、数据集、benchmark、工程洞见 |
| 6 | Archival Value | 10 | 长期参考价值：综述潜力、奠基地位、教学价值 |

**评分规则**：每维不得超权重；总分必须等于六维之和——**重算，不轻信 subagent 算术**；维度 1 是闸门：Topic Match <10 直接拒收，不论其他维度多高。Subagent 打分模板要求输出 JSON（title/scores/total/rationale）并返回按总分降序的 top 5。已知陷阱：subagent 给名不副实的知名论文在维度 1 打高分（必须复核）；高声望期刊论文维度 3 高但维度 1 低时闸门拦截；维度 4 看的是用户指定的追踪作者/机构而非名气。校准：跑 2–3 轮后与用户复盘分布——持续 90+ 说明太松，无破 60 说明关键词太窄或领域稀疏，按用户反馈调权重。

### ③ 精读（FINE READ）

对 top 5 做摘要级或全文级精读；每篇必须打阅读深度标签：`Full-text` / `Abstract only` / `Metadata only`（模板另加 `Project page / GitHub read`）。

### ④ 生成本地日报（GENERATE DIGEST，本地版替代原 DELIVER）

产物为**本地 Markdown 文件**，路径：

```
{用户指定输出目录}/daily/YYYY-MM-DD_文献日报.md
```

文件内容沿用原 push-format 的字段结构（纯文本落盘，不发送给任何软件）：

```
📅 {YYYY-MM-DD} 文献日报 | Research Field Daily
🏅 #{N} | {Title}
{Journal/Source}, {Year} | {Authors} et al. | {Institution} | ⭐ {score}/10 | 分流：{A-E tier}
DOI: ... | arXiv: ...
💡 一句话：...
🔬 方法：...
📊 关键结果：...
🧭 点评：...
📎 {best link}
```

字段纪律：💡 15 秒可判断是否点开原文，抓核心贡献不泛泛；🔬 写清实验/模拟、体系、表征手段；📊 必须是具体数据或明确结论（带单位与测试条件），**禁止"有重要发现""提供了新的视角"等空泛表述**；🧭 说清对研究主线的实际价值、局限、是否值得全文精读；⭐ 日报展示用 0–10，内部六维仍用 0–100 并校验上限。日报**不含**固定"与 vault 的关联"字段或强行 wiki 链接（低信息量）；命中追踪作者/网络时在点评中自然提及。

**文件落盘规则**：
- 文件名固定带日期；同日重复运行先写 `*.tmp` 再原子重命名，避免半成品；覆盖前先把旧文件备份为 `*.bak.md`。
- 生成后向用户回报**本地绝对路径**与 Top N 清单；这就是"交付"的全部动作。
- **禁止**调用 `send_message`、`lark-cli`、bot webhook、邮件接口或任何第三方推送；模板中若残留 `target: feishu:<chat_id>` 类配置一律忽略并删除。

### ⑤ 归档（ARCHIVE，references/note-template.md v2）

- DOI/arXiv 去重 → 分类 → 写笔记 → 更新本地索引；**只写用户指定目录下的 `raw/` 文献目录**。
- frontmatter 固定字段：`title`（英文原名双引号）、`authors`（`Last, F.; Last, F.`，超 3 人 et al.）、`year`（4 位整数无引号）、`journal`（全名）、`doi`（`10.` 开头）、`classification`（必须用中文全名五档之一：`A_核心主线` / `B_章节支撑` / `C_工程背景` / `D_方法借鉴` / `E_暂存低优先`）、`tags`（3–5 个具体中文关键词）、`date_read`（ISO 日期）。反模式禁用：`date`、`建议分类`、`是否值得全文精读`、`source_urls`、`note_date`。
- 正文固定 6 节（无 H1 标题行，frontmatter title 充当标题）：核心主张（1–3 句，标证据强度：成熟共识/学界共识/争议/推测）、方法、关键发现、批判、Connection to Research、下一步。
- 文件命名：`{FirstAuthorLast}{Year}_{中文核心关键词}.md`（连字符姓保留连字符；关键词 2–4 个中文词下划线连接，取自论文核心贡献，禁用"文献"之类泛词）。
- 目录纪律：subagent 常造短目录名（`A/笔记/`），必须核对纠正为全名 `A_核心主线/笔记/` 等。
- 批量标准化流程：盘点 .md → 解析现有 frontmatter → 从正文推断缺失字段（年份 `(19|20)\d{2}`、作者 `\*\*([^*]+?)\s*\((\d{4})\)\*\*`、标题取 H1）→ 生成标准 frontmatter 并去重 H1 → 按 `AuthorYear + 中文关键词` 改名并查碰撞 → 保存改名映射 CSV 供回滚。

### 附属流程 A：本机定时触发（替代原 cron 外发流程）

- 可选。用户需要"每天自动跑"时，用**本机操作系统调度器**注册一个任务：Windows 任务计划程序 / macOS launchd / Linux cron。任务动作只有一个——启动一次本地管线运行（agent 命令或本地脚本），**不包含任何消息发送动作**。
- 验证序列（沿用原纪律的本地版）：列出调度任务确认 job 存在 → 检查触发时间/命令/启用状态 → 手动跑一次 → 检查 `daily/` 下确实生成当天日报文件 → 检查 `raw/` 归档含预期笔记与去重标记。**创建响应成功不算数，必须从调度器读回任务并看到文件落盘**。
- 恢复路径：按原配置重建 → 手动跑缩小版检索 → 确认日报文件生成 → 归档到项目相对路径 → 重列调度器并记录新 job ID（仅本地）。
- 红线：本机调度是本地的，机器关机/休眠不会执行（错过不补，可手动补跑）；任务配置中绝不出现 chat ID、群名、webhook、bot token、消息 ID、投递日志；公开文档用项目相对占位路径，机器相关的 home/mount 路径留在用户私有配置。周期任务执行"手动运行检查、**文件落盘检查**、归档检查、去重检查、失败可见性"五件套（失败时在 `daily/` 写一份 `YYYY-MM-DD_FAILED.md` 说明原因即可，不外发告警）。

### 附属流程 B：空白分析（references/gap-analysis.md）

用于"搜一下 XX 体系有没有人做过""确认这个方向是不是空白"。4 步：

1. 多源检索：精确短语 + 关键组分宽搜 + 邻近/相关词，每个 query 记录命中数。
2. 分解分类：直接命中为 0 时把体系拆成子体系逐一检索；命中分三档——直接相关（若 0 则确认空白）/ 边缘相关（含 2–3 个组分，提取方法、体系、发现）/ 无关（跳过）。
3. 提取边缘论文：完整引用、所用体系、方法、与空白主题相关的关键发现、可借鉴点。
4. 输出空白报告到本地 `outputs/literature/<topic>_gap_report.md`，固定 7 节：核心结论（一句）/ 子体系文献全景表 / 边缘相关文献 / 空白分析（为何没人做+为何值得做）/ 论文 gap statement 英文草稿 / 建议下一步 / 搜索方法论记录（全部检索词与命中数，可复现）。
- 成本控制：每层分解 ≤4 个并行 web_search；只提取 ≤5 篇边缘论文；不用浏览器，web_extract 跳过付费墙；总工具调用 ≤6 次。

### 附属流程 C：手工综述编译（references/review-compilation-workflow.md，本地版盘点口径）

与每日管线互补——管线管日常发现，本流程管集中编撰（"零库存→投稿"）。六阶段：① **盘点本地存量**：在本地归档目录（`raw/`）按中英文关键词全文检索 frontmatter 与笔记，交叉去重、按 A/B/C 相关性分级（本地版不调用 Zotero 等外部库接口；如用户自己导出 Zotero 的 .bib/.ris 到本地目录，可一并解析）；② 空白填补（识别空白类型如特定体系/机理角度/时间尺度/团队遗漏；中英文定向检索、追踪引用链、必要时由用户自行在 CNKI 等中文库检索后把题录放入本地目录；综述目标引用量通常 80–150 条）；③ 观众过滤（合作者审阅版：删掉对方熟知领域、保留对方不熟方向的定量数据、砍偏离主线与低信息增量文献、保留对方团队工作；典型砍留比 90→50 篇）；④ 架构设计（一条因果链贯穿，核心章节约 30% 篇幅；典型 7 节结构 §1 Introduction ~8% → §4 失效模式核心 ~27% → §7 展望 ~10%，引用分布核心章节约占 50 条中的 16 条）；⑤ 图表规划（A 类自绘概念图 8–10 张、B 类引用图注明 "adapted from"、优先领域标杆论文）；⑥ BibTeX 导出（本地 `outputs/literature/` 下建 .bib，每条含 title/author/journal/volume/pages/year/doi/abstract 标注，% 注释分组）。

### 配置项（templates/literature-push-template.md，本地版口径）

可配置 YAML：`research_profile.field/subtopics`、`search.candidate_pool_size/final_selection_count/lookback_days/sources`、`keywords.include/exclude`、`important_authors_or_orgs`、`digest.enabled/root`（**本地日报目录，唯一的"投递"配置**）、`archive.enabled/root/write_wiki`。模板自带通用版评分权重、digest 格式与发布前 Verification Checklist；**本地版删除/忽略 `delivery.target` 等一切指向外部平台的字段**。LLM 领域论文需特别记录模型家族/规模、训练或后训练方法、benchmark、基线、代码/数据/模型可得性、serving 约束、评测弱点或污染风险。

## 四、输出契约与质量检查

- **本地日报**：`{输出目录}/daily/YYYY-MM-DD_文献日报.md`，沿用 emoji 字段结构；评分展示 `⭐ x.x/10`，绝不出现 `11/10` 之类的非法分；候选池数与精选数在报头明示；每篇必须有稳定链接与阅读深度标签。
- **归档笔记**：frontmatter + 6 节正文 + 命名规范 + 五档中文分类目录；只写本地 raw 目录。
- **空白报告**：本地 `outputs/literature/<topic>_gap_report.md`，7 节固定结构，含搜索方法论记录。
- **综述产物**：本地 `.bib` 文件、A/B 类图清单、砍留记录。
- **质量检查机制**：分数算术强制重算校验；每维封顶；维度 1 闸门（<10 拒收）；三重去重；生成前 Verification Checklist（配置齐全/已去重/分数合法/链接与深度标签齐全/**产物中无 chat ID、webhook、API key、机器私有路径**/未改知识库）；定时任务五件套检查；跑 2–3 轮后与用户校准权重。
- 无 scripts/、无 evals.json；守护全靠规则与清单。

## 五、关键规则与红线

1. 每维分数不得超权重；总分必须重算核验（"no 11/10 allowed"）；subagent 可能虚高分数，必须复核维度 1。
2. Topic Match <10 自动拒收，无论其他维度多高。
3. 三重去重（DOI / arXiv ID / OpenAlex ID）+ 规范化标题；经典论文会反复出现，维护本地去重索引防 duplicate creep。
4. **只写本地 `raw/` 与 `daily/` 目录**；未经用户明确批准不修改其他笔记库/知识库。
5. 阅读深度如实标注：拿不到全文就标 `Abstract only` / `Metadata only`。
6. 评分不等于质量终审，高分候选仍需人工阅读确认。
7. **零外发**：不调用 `send_message`、lark-cli、bot、webhook、邮件或任何第三方推送接口；不向用户询问 chat ID/群名；日报的唯一交付方式是本地文件 + 对话中告知路径。
8. 本机调度器是本地的——机器必须开着；创建任务后必须从调度器读回并验证文件落盘；失败只在本地留失败记录。
9. 公开示例/文档中绝不出现真实 chat ID、群名、webhook、调度器 ID、投递日志、机器私有路径（本地版直接不产生此类信息）。
10. 📊 关键结果禁空泛表述；日报不加强行 vault 关联字段。
11. 关键词每月复审防 keyword drift。
12. 笔记文件名与分类目录必须用全名规范；frontmatter 反模式字段禁用。
13. 空白分析成本控制：每层 ≤4 并行检索、≤5 篇边缘提取、总调用 ≤6、跳过付费墙。
14. 在线检索只拉取公开题录，不把用户的研究方向细节之外的私有数据写入任何外部请求。

## 六、配套资源

- `references/scoring-system.md`：六维评分细则（权重、闸门、subagent 打分 JSON 模板、陷阱、校准）——**本地版原样使用**。
- `references/gap-analysis.md`：研究空白确认方法论——**原样使用**。
- `references/note-template.md`：归档笔记标准——**原样使用**。
- `references/push-format.md`：**本地版仅借字段结构与合成示例**；文末 `send_message` / chat_id / Delivery 段停用，输出对象是本地 Markdown 文件。
- `references/cron-setup.md`：**本地版仅借"创建-读回-手动验证"纪律**；改为本机 OS 调度器触发本地运行、以文件落盘为准，删除一切发消息步骤。
- `references/review-compilation-workflow.md`：手工综述六阶段；盘点范围改为本地归档目录（+ 用户自行放入的本地 .bib/.ris），不连接外部文献库接口。
- `templates/literature-push-template.md`：可复用 profile 模板；本地版忽略 delivery.target，新增/使用 `digest.root` 本地目录配置；模板本就声明不含私密 chat ID、API key。
- `agents/openai.yaml`：原始界面声明，本地版默认提示词改为"生成本地文献日报文件"。
- `README.md` / `README_EN.md`：原始中英文说明；本地版以本文档口径为准。
- 无 scripts/、evals/、static/。
