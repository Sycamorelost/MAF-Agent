# nature-data

> 本文档是对 `skills/nature-data/` 目录全部功能性文字内容的忠实重构，引用路径均以该目录为根。

## 一、定位与触发场景

**用途**：起草或审计手稿的 Data Availability / Code Availability 声明、数据集获取路径、仓库与标识符方案、FAIR 元数据，产出可直接粘贴的声明文本 + 未决字段清单。

**description 原文摘录**（SKILL.md frontmatter）：

> "Draft or audit manuscript Data/Code Availability statements, dataset access routes, repository plans, and FAIR metadata. Use for 数据可用性声明、数据共享、数据仓库选择 and dataset citations; not general data cleaning or statistical analysis."

**典型触发请求**：写/改"数据可用性声明"；选择数据仓库与登录号策略；审计现有声明是否达到 Nature 投稿标准；为复用公开数据补数据集引用；中英对照措辞。

**明确不做什么（边界）**：
- 不做通用数据清洗或统计分析；不改写 methods、不做统计、不润色手稿（除非用户单独要求）。
- 只关注 availability 与 metadata。
- 对已有声明的局部措辞修改/审计：只检查所给的仓库标识符与访问条件，报告相关缺口；**不得**要求全研究范围的数据清单或仓库重设计，也不得因一条声明合格就推断整个研究已 FAIR/可投稿。

## 二、运作模式（加载/路由机制）

### SKILL.md 作为路由

SKILL.md 标题为 "Nature Data Availability — Router"：

1. 读 `manifest.yaml`，再读 `always_load` 全部文件。
2. **无内容轴**（与 nature-citation 同款设计）：变化在运行时内联处理——
   - **journal/article type**：期刊专属说明与本 skill 冲突时，从期刊；
   - **access route**：每个数据集归入一条路径（public repository / controlled access / within paper / reused public / third-party restricted / justified request / not applicable）；
   - **user language**：用户写中文或要求中文指导时读 `static/core/chinese-mode.md`，并在输出加 `中文核对` 块（除非用户只要声明文本）。
3. 执行 `core/workflow.md` 八步流程。
4. references 按需加载；两个特定期刊特例：
   - 目标是旗舰刊 **Nature** → 加载 `references/nature-article-requirements.md`（声明位置、强制寄存路由、central-code 审稿访问、材料与结构文件检查）。
   - 目标是 **Nature Machine Intelligence** → 加载 **`../nature-shared/journal-formats/nature-machine-intelligence.md`**，强制 Data Availability 声明 + 其后、参考文献之前的独立 `Code availability` 节；检查审稿人访问、精确限制、仓库/标识符质量、新开发 central code 的 Software Submission Checklist。

### manifest.yaml 结构

- `always_load`：`static/core/stance.md`（data-availability package 定义、默认立场、来源层级）、`static/core/workflow.md`（八步流程与输出格式）。
- **无 axes**（manifest 设计说明：线性参数化流程 inventory → classify access route → choose repository/identifier → draft → cite → FAIR audit）。
- `references.on_demand`（条件 → 路径）：
  - 中文用户/中文指导 → `static/core/chinese-mode.md`
  - Nature/Springer Nature 数据共享治理规则与边缘案例 → `references/policy-principles.md`
  - 目标是旗舰 Nature / 需精确 Nature Article 数据、代码、材料、强制寄存、审稿访问、声明位置规则 → `references/nature-article-requirements.md`
  - 目标是 NMI / 需 NMI Data Availability、Code availability、central-code、software checklist 规则 → `../nature-shared/journal-formats/nature-machine-intelligence.md`
  - 中文措辞/双语/中文 availability 草稿 → `references/chinese-author-alignment.md`
  - 可直接改写的声明模板 → `references/statement-patterns.md`
  - 仓库选择、登录号、DOI、embargo、版本、数据集引用 → `references/repository-and-identifiers.md`
  - FAIR 检查、README 元数据、文件组织、许可、provenance、DataCite 字段 → `references/fair-metadata-checklist.md`
  - 用官方来源为规则作证 → `references/source-basis.md`

### 对 nature-shared 的依赖

仅一处：`../nature-shared/journal-formats/nature-machine-intelligence.md`（NMI 期刊事实层：阶段门 initial_submission/revision/accepted_in_principle/proof、Article/Analysis 契约、数据/代码/reporting standards 等）。manifest 同时声明：本 skill 不使用 prose 导向的 nature-shared 层（它治理数据可用性政策，有自己的 source basis）。

## 三、核心工作流（八步，static/core/workflow.md）

1. **识别目标期刊与文章类型**；期刊说明与本 skill 冲突时从期刊。
2. **盘点全部支撑数据集**：生成的原始数据、处理后数据、figure source data、二次数据、软件输出、模型、表格、图像、统计分析底层文件（主文与补充材料都算）。
3. **为每个数据集分类一条 access route**：`public repository` / `controlled access repository` / `within paper or supplement` / `reused public source` / `third-party restricted` / `available on justified request` / `not applicable`。
4. **起草前先定仓库与标识符策略**：优先 DOI、登录号、Handle、ARK 或稳定仓库记录，优于个人网站与临时云链接。
5. **起草声明**：显式的 dataset-to-location 映射。
6. **为支撑结论的公开数据加正式数据集引用**（进参考文献表）。
7. **定稿前跑 FAIR 与元数据审计**。
8. **返回可直接粘贴的声明文本 + 作者须确认的未决字段**。

审计已有声明时：先列阻断性问题，再给修订版。

### 关键子流程细节

**七条 access route 的声明必备要素**（policy-principles.md）：

| Route | 声明必须包含 |
|---|---|
| Public repository | 仓库、DOI/登录号、数据集标题或范围、许可（如已知） |
| Controlled repository | 仓库、登录号/记录、访问委员会或程序、限制 |
| Supplementary/source data | 精确的文件/表/源数据映射 |
| Reused public data | 原仓库/来源、标识符、版本/访问日期（如相关） |
| Third-party restricted | 所有者/来源、为何不公开、申请路径、许可条件 |
| Request-based access | 原因、负责团体、资格、预期条件、联系方式 |
| Not applicable | 简明原因；有任何经验数据的研究不可用此项 |

**Minimal dataset test**（policy-principles.md）：独立读者能否凭可得材料检查/复现核心发现？应含主图与关键补充图 source data、符合社区规范的 raw 或可复用数据、用于统计/绘图/模型训练/验证的处理后数据、第三方数据集的来源+版本+访问日期+许可、受限数据集的代表性元数据。可辩护的排除：未支撑任何结果的数据、纯理论工作、依法/知情同意不能共享的可识别人类数据。

**敏感/人类受试者数据**：必须说明为何不公开、可否共享匿名/聚合/合成/代表性数据、元数据记录在哪、谁审申请、适用何种审批/数据使用协议/伦理条件、是否限非商业/学术/特定辖区用户。避免：把单个个人当唯一持久访问路径（有机构路径时）、暗示依赖不可能权限的数据"可用"、无仓库/日期/责任方地承诺"以后公开"。

**旗舰 Nature 附加门**（nature-article-requirements.md）：每篇原创 Article 需有标题的 `Data Availability` 声明并按现行 Nature Article 顺序放在 Methods/Methods 末尾区；强制寄存门（蛋白序列、DNA/RNA 序列与测序数据、多态性与基因型/表型、大分子结构与电镜图、基因表达含 MIAME、小分子晶体学、蛋白质组学、有社区仓库的地学/空间/环境数据）；未报告过的核心自定义代码需独立 `Code Availability` 声明、审稿期间应要求向编辑/审稿人提供、优先有 DOI 的归档版本仓库；材料用 RRID、细胞系报来源/鉴定/分发限制；临床数据 `Undecided` 不是可接受的最终共享计划；审计表加列 `Dataset/code/material | Claim supported | Required repository/file | Reviewer access | Publication access | Statement placement | Status`，缺强制寄存/核心数据代码不可访问/受控访问未定义/缺结构验证文件时标 `blocked`。

**仓库选择决策树**（repository-and-identifiers.md）：强制仓库 → 学科社区仓库 → 可信通用/机构仓库（须有持久标识符与持久元数据）；个人网站/实验室网站/临时云盘/未归档 GitHub 分支不得作为唯一路径。仓库记录应含：持久标识符、公开 landing page（标题/创建者/描述/日期/版本/许可）、文件清单含大小格式、README/数据字典、provenance、与手稿及代码的关联、受限数据访问程序、版本政策。标识符规则：投稿前尽量用最终公开标识符；评审期私密记录提供匿名审稿链接；不引用临时分享链接；登录号严格按仓库所赋；一个连贯数据集一个标识符；变更后做版本化；有 DOI 引 DOI。数据集引用格式（DataCite 风格）：`[Creator(s)] ([Year]) [Dataset title]. [Repository]. [Identifier].`（可加 version）。

**FAIR 快测**（fair-metadata-checklist.md）：Findable（持久标识符+丰富标题摘要关键词+元数据指明标识符）；Accessible（标准协议可解析、访问条件明确、数据受限时元数据仍公开）；Interoperable（社区格式、共享词汇/单位/标识符、与相关数据/代码/论文的限定链接）；Reusable（许可、provenance、方法、变量、质控、版本清晰）。DataCite 必填字段：Identifier/Creator/Title/Publisher/Publication year/Resource type。附数据集 README 模板（Summary/Files/Variables and units/Methods and provenance/Software and environment/Access and licence/Citation）与文件组织规则（raw 与 processed 分开、source data 映射到具体图 panel、记录缺失值编码、大文件附 checksums）。

**中文模式**（chinese-mode.md + chinese-author-alignment.md）：中文输入照收，最终投稿声明默认英文；"翻译意图而非措辞"——`可向通讯作者索取` 之类太模糊，须补限制原因+审核路径+条件；术语映射表（数据可用性声明→Data Availability、源数据→source data、受限数据→restricted data、合理请求→reasonable request 须带 reason 和 review route 等）；常见中文草稿修复表（"数据暂未上传"→现在寄存或列为 blocking 行动项；"使用了公开数据库"→给出库名/登录号/版本/访问日期并引用数据集）；双语问诊五问；中文解释不放进最终英文声明正文（除非期刊允许双语）。

## 四、输出契约与质量检查

默认输出格式（workflow.md）：

```text
Data Availability
[ready-to-paste statement]

Repository and citation actions
- [specific actions or "None"]

Missing information / risk flags
- [specific flags or "None"]

中文核对
- [用中文列出作者需要确认的字段或 "无"]
```

**Submission-stage checks**（policy-principles.md）：所有登录号/DOI/URL 可解析；embargo/私密审稿链接可匿名访问；受限数据元数据记录公开；补充文件与声明措辞一致；数据引用出现在参考文献表；无结论依赖无法获得的数据而无解释。

**Repository readiness checklist**（repository-and-identifiers.md）：DOI/登录号解析到正确 landing page；标题与手稿术语一致；创建者与机构正确；许可存在且与预期复用兼容；文件尽量不需专有软件即可打开；README 解释列/单位/缺失值/变换/脚本；figure source data 映射到图 panel；限制条件与手稿声明一致；embargo/私密链接已在作者账号外测试。

**Final audit 阻断项**（fair-metadata-checklist.md）：原创研究无 Data Availability 声明；核心结论数据无标识符或稳定路径；敏感数据有限制但无访问程序；第三方数据无来源或许可路径；公开数据集无许可或 README；声称数据在文中但缺 figure source data；手稿声明、仓库记录、补充文件三者不一致。

## 五、关键规则与红线

- **绝不虚构** DOI、登录号、仓库名、许可、embargo 日期、伦理审批、访问委员会、数据使用条件（SKILL.md 与 stance.md 双重强调）。
- "available upon request" 是弱表述，除非有具体的法律/伦理/商业/第三方限制，必须标记。
- 声明必须同时覆盖本研究生成的数据与复用的二次数据。
- 数据、代码、材料、protocols 分开陈述，除非期刊要求合并 availability 节。
- 不把通讯作者邮箱当唯一持久访问路径（有机构路径时）。
- 不为第三方或受试者数据套开放许可（除非作者有权）。
- 反模式表（statement-patterns.md）：`Data are available upon request.`（无原因/路径/资格/持久性）、`available from the corresponding author on reasonable request`（"可向通讯作者合理索取"的直译）、`Data will be uploaded after acceptance.`（无现仓库与持久标识符）、`All data are in the manuscript.`（对图/统计常为假）、`Data are proprietary.`（没说谁控制访问）、`N/A.`（须解释）。
- 政策细节可能变化：给最终投稿建议前核对现行期刊页面。
- 受限数据仍需持久访问路径：具名数据访问委员会、机构、受控仓库、申请程序或负责团体。
- 仓库 red flags：GitHub 无 release DOI/归档；仓库记录无许可；zip 无 README/manifest；登录号存在但不公开、非 embargo、审稿人不可达；文件名用本地分析速记；手稿只引一个数据集但结果依赖多个未列二次来源。

## 六、配套资源

**references/**：
- `policy-principles.md` — 治理规则、minimal dataset test、七条 availability routes、数据/代码/材料/protocols 分工、敏感与人类受试者数据、投稿阶段检查。
- `nature-article-requirements.md` — 旗舰 Nature 的声明位置、强制寄存门、Code Availability、材料与 RRID、受控/临床/第三方数据、结构数据文件（.cif/CheckCIF）、Nature readiness 审计列。
- `statement-patterns.md` — 11 类可直接改写的英文声明模板（公开仓库单/多数据集、仅文内+补充、复用公开、混合、受控访问、第三方、商业限制、embargo、合理请求、无数据），每条约附中文作者指引，另含反模式表与审计五问。
- `repository-and-identifiers.md` — 仓库决策树、仓库记录应备要素、分学科仓库类别表（GEO/SRA/ENA、wwPDB、CCDC、PRIDE、MetaboLights、OpenNeuro/DANDI、dbGaP/EGA、PANGAEA、ICPSR/Dataverse、Dryad/Zenodo/Figshare/OSF 等）、标识符规则、数据集引用格式、readiness 清单与 red flags。
- `fair-metadata-checklist.md` — FAIR 快测、DataCite 核心字段、README 模板、文件组织、provenance 提问、许可指引、最终阻断项。
- `chinese-author-alignment.md` — 中英术语表、CN→EN 转换规则、双语问诊、常见中文草稿修复表。
- `source-basis.md` — 每条 skill 规则对应的官方来源映射表（Springer Nature research data policy、reporting standards、Scientific Data policy、Wilkinson et al. 2016 FAIR、DataCite Schema）及官方 URL 列表。

**共享层**：`../nature-shared/journal-formats/nature-machine-intelligence.md`（NMI 期刊事实）。

**其他**：`agents/openai.yaml` 装配声明；本 skill 无 scripts/、无 evals/、无 templates/。
