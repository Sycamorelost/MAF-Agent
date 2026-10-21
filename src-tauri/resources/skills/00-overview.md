# NATURE SKILLS 运作模式总览（本地版）

> 本文是对开源仓库 `nature-skills`（源路径：`d:\EdgeDownloads\nature-skills-main`）运作机制的深度重构文档。
> 仓库包含 23 个面向 Nature 系科研工作流的 agent skill，本目录下的 `skills/` 收录了每个 skill 的独立重构文档。

> **本地化口径（第二轮改造）**：本文描述的架构机制（路由、分层、加载协议）保持原样；但凡涉及外部软件/服务的运行时行为，`skills/` 下各文档已统一改造为**不外推、不连接第三方**：无飞书/Telegram/邮件推送、无浏览器 CDP/登录态、无第三方图像/OCR API 与密钥、无 Zotero/MCP 连接；只保留对公开免费来源的只读查询与宿主平台内置能力，所有制品写本地文件。

---

## 1. 项目定位

`nature-skills` 是一套面向 AI 编码/科研代理（agent）的**技能包仓库**，覆盖科研全流程：读论文、翻译、润色、写作、审稿、返修回复、文献检索、引用核验、科研制图、幻灯片生成、数据可用性声明、实验记录、基金申请书、专利转化、统计分析等。

核心设计理念：**技能的最小单元是完整目录，而不是单个 `SKILL.md` 文件**。接入任何 agent 框架时都必须保留整个技能目录（含 `references/`、`static/`、`scripts/` 等），只复制 `SKILL.md` 会导致技能残缺。

## 2. 仓库顶层结构

```text
nature-skills-main/
├── docs/        # 接入教程（OpenClaw/OpenCode/Hermes）、范围冒烟评估报告
├── scripts/     # 仓库级校验与维护脚本（validate-*.py、autoupdate-skills.sh 等）
├── skills/      # 23 个 nature-* 技能目录（核心资产）
└── tests/       # 仓库级测试
```

23 个技能一览：

| 技能 | 功能域 |
| --- | --- |
| nature-reader | 全文中英对照精读稿（图文对应、公式处理、source map） |
| nature-polishing | 论文润色（按论文类型/章节/语言/期刊四轴加载规则） |
| nature-writing | 摘要、引言、结果、讨论等章节写作 |
| nature-reviewer | 模拟审稿人预审稿 |
| nature-response | 返修回复信、cover letter、审稿意见逐条回复 |
| nature-figure | 科研图、论文示意图生成 |
| nature-academic-search | 文献检索、严格他引、引用者画像 |
| nature-citation | 引用格式处理 |
| nature-ref-verifier | 参考文献真实性核验 |
| nature-data | Data Availability 声明等数据相关文本 |
| nature-downloader | 论文/资料下载（含 src、assets、examples 的完整工具型技能） |
| nature-experiment-log | 实验记录模板化 |
| nature-literature-pipeline | 文献处理流水线 |
| nature-paper-card | 论文卡片（结构化摘要卡） |
| nature-paper-to-patent | 论文转专利 |
| nature-paper-trans | 论文翻译 |
| nature-paper-xray | 论文透视分析 |
| nature-paper2ppt / nature-image2ppt | 论文/图片转幻灯片 |
| nature-proposal-writer | 基金/项目申请书写作 |
| nature-social-science | 社会科学研究专用 |
| nature-statistics | 统计分析 |
| nature-shared | **内部共享参考层**，不作为独立工作流调用 |

## 3. 单个技能的解剖结构

典型技能目录（以 `nature-reader` 为例）：

```text
skills/nature-reader/
├── SKILL.md          # 路由入口：YAML frontmatter + 路由协议（唯一被 agent 框架直接加载的文件）
├── manifest.yaml     # 声明式清单：always_load、axes（检测轴→fragment 映射）、references.on_demand
├── README.md / README_EN.md   # 人类阅读的配置与依赖说明
├── agents/openai.yaml         # agent 框架接入声明（display_name、short_description、default_prompt）
├── evals/evals.json           # 评估用例：prompt + expected_output
├── references/     # 深度参考，按需加载（grounding 规则、输出 schema、公式处理……）
├── static/
│   ├── core/       # 核心层：每个任务必加载（原则、工作流、输出契约）
│   └── fragments/  # 条件片段：按 axis 检测结果选择性加载
└── scripts/        # 技能自带校验/功能脚本
```

各部件职责：

- **SKILL.md = 路由器（Router）**。frontmatter 含 `name`、`description`（触发场景与边界）、`metadata.version`。正文是"路由协议"：先做任务类型判断（例如"精读全文 vs 回答单个问题"），再决定加载哪些文件。它明确告诉 agent：**不要读全部 fragment，只读被选中的那个**。
- **manifest.yaml = 声明式加载清单**。三部分：
  - `always_load`：每次任务必读文件（共享层 + 技能本地 core）。
  - `axes`：检测轴。每个轴有 `detect`（如何根据用户输入判断取值）、`values`（取值 → fragment 文件路径映射）、`default`、`multi`（是否可多值）。例如 nature-reader 有 1 个轴 `source_format`（pdf-text / scanned-pdf / html / doi-arxiv / pasted-text）；nature-polishing 有 4 个轴（paper_type / section / language / journal）。
  - `references.on_demand`：条件 → 路径对照表，仅当任务触及对应条件时才打开。
- **static/ = 静态层**：core（普适原则与工作流）与 fragments（按轴分片的专门规则）分离，是"静态/动态分层"（static/dynamic split）架构的主体。
- **references/ = 动态深参考**：不是默认加载项，由 SKILL.md 第 5 步"按需取用"。
- **agents/openai.yaml**：面向 agent 框架的界面声明（显示名、简介、默认提示词）。
- **evals/evals.json**：冒烟评估用例，验证技能在真实 prompt 下的行为是否符合预期（含边界行为，例如"小请求不得触发完整大流程"）。

## 4. 核心运作模式：分层渐进式加载（Progressive Disclosure）

技能的运行时遵循统一的五步路由协议（以 nature-reader 的 SKILL.md 为范式）：

```text
第 0 步：范围判断 —— 区分"完整制品请求"与"局部请求"（单点问题/片段翻译）。
        局部请求只加载 grounding 规则直接回答，绝不重新生成完整制品。
第 1 步：读 manifest.yaml + always_load 全部文件（共享层 + core 原则/工作流/输出契约）。
第 2 步：检测 axis 取值 —— 用 manifest 的 detect 提示 + 用户输入判定，
        并用一句话向用户声明检测结果（低成本纠错机会）。
第 3 步：只加载被选中取值映射的 fragment 文件，禁止读取全部 fragment。
第 4 步：按优先级组装执行 —— core 原则 > fragment 专门规则 > 工作流 > 输出契约。
第 5 步：references 按需取用 —— 仅在任务触及 on_demand 条件时打开对应深参考。
```

这种架构的收益：

- **上下文经济**：agent 只为当前任务加载必要文字，23 个技能的庞大规则库不会一次性灌入上下文。
- **声明式可维护**：新增输入类型 = 在 manifest 的 axis 中加一个取值 + 新增一个 fragment 文件，SKILL.md 路由逻辑不变。
- **可纠错**：检测结果被显式声明，用户可以廉价地纠正误判。
- **可评估**：evals.json + 范围冒烟评估（`docs/skill-scope-smoke-evaluation.md`）验证"小请求不触发大流程""不伪造数据/引用"等关键行为。

## 5. 共享层 nature-shared

`nature-shared` 是被其他技能依赖的**内部参考包**，禁止作为独立工作流调用。其 SKILL.md 明确规定：只加载被请求的那个文件，不预加载整个包。内容分两类：

- `core/`：跨技能共享定义 —— 术语台账（terminology-ledger）、伦理（ethics）、读者工作流（reader-workflow）、论文类型分类（paper-type-taxonomy）、Nature 语料派生的摘要/引言/结果-讨论写作法则、讨论段论证语言、正文纪律（main-text-discipline）、一致性扫描（consistency-sweep）、研究合规门（research-compliance）。
- `journal-formats/`：期刊格式约束 —— `nature.md`（旗舰刊 Nature 专用）、`nat-comms.md`、`nature-machine-intelligence.md`（NMI 的文章类型/字数/数据代码义务，明确禁止与旗舰刊或 NC 的限制混用）。

其他技能通过 manifest 的 `always_load` 或 `references.on_demand` 以相对路径（`../nature-shared/...`）引用共享文件。因此**接入框架时必须连同 nature-shared 一起安装**。

## 6. 跨框架接入模式

仓库 docs 给出三类开源 agent 框架的接入方式，共同原则是**保留完整目录 + 符号链接指向稳定 clone**：

| 框架 | 技能目录 | 推荐方式 |
| --- | --- | --- |
| OpenClaw | `~/.openclaw/workspace/skills/<skill>/` | 符号链接每个 `nature-*` 目录 |
| OpenCode | `~/.agents/skills/` 或项目级 `.agents/skills/` | 符号链接，或在 `opencode.json` 声明 `skills` 路径 |
| Hermes | `~/.hermes/skills/` | `config.yaml` 的 `skills.external_dirs` 声明外部目录（注意可写风险） |

关键规则：保留完整技能目录；引用共享文件时保留同级 `nature-shared/`；涉及脚本/MCP/API 的技能仍需按各自 README 配置依赖与凭据。**本地版补充**：不接入需要外连或凭据的组件——MCP server 默认不装、付费 API key 不配、浏览器/消息类集成不启用；需要凭据的能力在缺失时直接降级为"只出内容/如实报告"，而非引导用户配置外部服务。

## 7. 仓库级质量基础设施

`scripts/` 下的校验脚本构成仓库的 CI 质量门：

- `validate-repository.py` — 仓库整体结构校验
- `validate-skill-metadata.py` — SKILL.md frontmatter / manifest 元数据校验
- `validate-skill-index.py` — 技能索引一致性
- `validate-readmes.py` / `validate-readme-mirror.py` — 中英文 README 镜像一致性
- `validate-workflows.py` — 工作流校验
- `update-reference-tocs.py` — references 目录索引维护
- `autoupdate-skills.sh` / `update-codex-skills.sh` — 向 Codex 等目标分发/更新技能
- `generate-star-history.py` — 生成 star 历史图（含对应测试 `tests/test_generate_star_history.py`）

行为层面的质量则由每个技能的 `evals/evals.json` 与仓库的**范围冒烟评估**（2026-09-06，5 个隔离 agent 上下文 × 6 个短请求）承担，确认的边界行为包括：单点问题不产生完整 reader 也不伪造 source-map ID；局部回复不追问 Major/Minor；描述性段落不越过证据边界（不虚构显著性/机制）；语法修改不索要全研究数据清单；离线引用转换不伪造 DOI。

## 8. 设计原则总结

1. **目录即技能**：功能文字分布在 SKILL.md / manifest / core / fragments / references 中，协同构成完整技能。
2. **路由先于执行**：SKILL.md 只做判断与加载决策，不承载全部规则。
3. **声明式分片**：manifest.yaml 的 axis 机制把"检测什么输入"与"加载哪份规则"解耦。
4. **共享层单一来源**：跨技能的期刊格式与写作法则集中在 nature-shared，避免多份漂移。
5. **范围纪律（scope discipline）**：小请求给小程序，完整制品请求才走完整流水线；证据不足时输出草稿模式并标注缺口，绝不降级为摘要或编造内容。
6. **可验证性**：输出契约（output contract）+ 自检清单 + 校验脚本 + evals 四层保证。
