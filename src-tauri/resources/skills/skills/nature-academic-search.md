# nature-academic-search（本地版）

> **本地化改造说明**：本技能的全部网络行为都是**只读检索公开元数据**（PubMed E-utilities、CrossRef、arXiv、OpenAlex、Semantic Scholar、出版社公开页面），不上传、不推送任何用户数据。本地版口径：① **默认走 no-MCP 回退**——`scripts/academic_search.py` 纯 stdlib 直连 OpenAlex，免费、无需 key、无需启动本地 MCP server；② 不安装/配置 MCP server 亦可完成全部主流程，MCP 仅作为用户本机可选的便利层（它本身是本地进程，不是第三方服务）；③ Scopus/ScienceDirect/WoS 等需 key 或机构代理的来源**默认不配置、不访问**，用户主动提供 Elsevier key 时才可启用，绝不引导注册；④ CNKI/万方/Google Scholar 等 T3 来源不使用登录态、不抓取，查不到即标"结果可能不完整"；⑤ 所有产出（检索表、RIS/BibTeX、审计报告）只写本地文件。

## 一、定位与触发场景

**用途（本地版）**：多源学术文献检索、引文核对、参考文献/引文文件管理、MeSH 检索式构建，以及"严格他引"（strict other-citation）影响力审计；检索渠道限定公开免凭据来源为主，付费/机构来源默认不接。

**description 原文摘录**（SKILL.md frontmatter）：

> "Search literature across sources, verify or manage citations, and build MeSH strategies or citation-impact audits. Use for 文献检索、引文核对、参考文献管理、严格他引 and evidence-backed citer profiles; not for translating a paper or drafting manuscript prose."

**典型触发请求**（README.md）：
- "帮我查这篇论文的严格他引，并列出高影响力引用者。"
- "按这个主题找 20 篇近五年的 Nature/Science/Cell 相关论文。"
- "把这些 DOI 转成 Nature 格式引用，并导出 RIS。"
- "为 PubMed 检索式补 MeSH 词和同义词。"
- WF6 专用触发词：`严格他引`、`他引判定`、`排除自引`、`谁引用了我的文章`、`引用我的文章的人有没有大牛`、`院士引用`、`杰青引用`、`长江学者引用`、`Fellow citation`、`influential citer`、`citation context`、`文章引用表`、`指定文章引用数`、`严格他引数`、`整理成表格`。

**明确不做什么（边界）**：
- 不翻译论文、不撰写手稿正文（"not for translating a paper or drafting manuscript prose"）。
- 二级学术索引只能作发现线索，关键字段必须回到 DOI、PubMed、出版社页面或数据库记录核实。
- 严格他引判断需要明确排除规则；缺作者/机构信息时标注不确定。
- 不绕过无权限数据库，只报告缺失来源或提示用户登录。
- 中文文献（CNKI / 万方）不被 CrossRef/PubMed 索引，只能手工处理（T3 manual）。

## 二、运作模式（加载/路由机制）

### SKILL.md 作为路由

SKILL.md 标题即 "Academic Search — Router"，其 routing protocol 为：

1. **特例短路**：若用户提供完整引文记录、仅要求本地格式转换（offline conversion），直接加载 `references/workflows/wf4-citation-file-mgmt.md` 和格式参考；不要求来源查询、API 配置、网络 preflight。
2. **加载 manifest 与核心层**：读 `manifest.yaml`（声明 workflow 轴），并读 `always_load` 下的全部文件。
3. **检测 workflow**：把用户需求映射到一个或多个 workflow 值，用一句话声明检测到的 workflow 后再继续。
4. **加载对应 workflow fragment**：只读检测到的 workflow 对应文件（在 `references/workflows/` 下），不要读全部。
5. **按序执行**：先 core（工具与路由），再 workflow fragment 步骤，最后按需调用共享模块与脚本。
6. **references 按需加载**：`references/` 与 `scripts/` 是深度参考而非默认加载，按 manifest 的 `references.on_demand` 表打开。

### manifest.yaml 结构

- `always_load`：
  - `static/core/tools.md` — MCP 工具清单（core search / Scopus·ScienceDirect / extended search / PubMed utilities）与共享模块地图。
  - `static/core/routing-and-ops.md` — T1→T2→T3 来源路由速查、环境配置、错误处理、局限性。
- `axes`：仅一个轴 `workflow`，`multi: true`（组合请求可加载多个），无显式 default。取值与映射：
  - `multi-source-search` → `references/workflows/wf1-multi-source-search.md`
  - `citation-verification` → `references/workflows/wf2-citation-verification.md`
  - `mesh-strategy` → `references/workflows/wf3-mesh-strategy.md`
  - `citation-file-mgmt` → `references/workflows/wf4-citation-file-mgmt.md`
  - `reference-mgmt` → `references/workflows/wf5-reference-mgmt.md`
  - `strict-other-citation-impact-audit` → `references/workflows/wf6-strict-other-citation-impact-audit.md`
- 注：manifest 明确说明 workflow 文件就地放在 `references/workflows/` 而未移入 `static/fragments/`，因为文件间用 `../` 相对链接互引；**本 skill 不使用 prose 导向的 nature-shared 共享层**。
- `references.on_demand`（条件 → 路径）：
  - 完整 T1/T2/T3 可靠性分级与回退规则 → `references/source-tiers.md`
  - 跨来源去重（WF1/2/5a）→ `references/dedup-engine.md`
  - 从文档抽取引文（WF2）→ `references/citation-parser.md`
  - 查询构造、来源选择、结果排序 → `references/search-strategy.md`
  - 严格他引/高影响力引用者/引用上下文审计 → `references/workflows/wf6-strict-other-citation-impact-audit.md`
  - RIS/BibTeX 格式规范与字段映射 → `references/ris-bibtex-format.md`
  - MCP 不可用时的 no-MCP 回退检索（OpenAlex, stdlib）→ `scripts/academic_search.py`
  - 多源 .nbib/.ris/.bib 下载与转换 → `scripts/format-converter.py`
  - 批量操作前的 API 端点 preflight 检查 → `scripts/preflight.py`

### 静态层与动态判断的分工

- 静态 core（始终加载）：工具清单 + 来源路由/运维规则，回答"用什么工具、按什么顺序调源"。
- 动态判断：SKILL.md 只做 workflow 检测与文件加载决策；具体步骤由被选中的 workflow fragment 提供。

### 对 nature-shared 的依赖

无。manifest 注释明确："nature-academic-search does not use the prose-oriented nature-shared layer."

## 三、核心工作流（实际功能流程）

### 通用基础设施

**T1→T2→T3 来源路由**（`static/core/routing-and-ops.md` + `references/source-tiers.md`）：

| 用户需求 | T1（首选） | T2 | T3（最后手段，须警告用户） |
|---|---|---|---|
| 医学/临床 | PubMed | Semantic Scholar | Google Scholar |
| 跨学科 | CrossRef | Semantic Scholar | Scopus |
| 预印本/CS/物理 | arXiv | bioRxiv / medRxiv | — |
| 穷尽式综述 | PubMed + CrossRef + arXiv | Semantic Scholar + bioRxiv/medRxiv | WoS / Scopus |
| 被引数敏感 | Semantic Scholar | CrossRef | — |
| 中文文献 | — | — | CNKI / 万方（manual） |

回退规则伪码：选匹配领域的 T1 → 并行检索 → 找到且相关性达标则接受 → T1 不足升级 T2 → T1+T2 用尽升级 T3 并警告用户 → 全部用尽返回部分结果并建议优化查询。

**环境配置（本地版口径）**：默认**零配置**——no-MCP 回退直连 OpenAlex 无需 key；PubMed/CrossRef/arXiv 公开端点免 key（PubMed 只需在联系信息中带邮箱礼貌访问）。`SEMANTIC_SCHOLAR_API_KEY`、`NCBI_API_KEY` 仅为提速、用户主动提供才设置；Elsevier/Scopus 的 `~/.config/pybliometrics.cfg` 默认不配置（**禁止把 API key 写进本仓库，也不向用户索取**）；不设机构代理；批量网络操作前可跑 `python scripts/preflight.py` 自检公开端点连通性。

**错误处理通则**：MCP 工具不可用或未安装 → **本地版常态，直接走 no-MCP 回退**，无需向用户提及安装 MCP；无结果→放宽词、换公开源、建议用户细化查询；付费/机构源不可用不寻求登录态替代，明确报告来源覆盖缺口。

### WF1 多源检索（wf1-multi-source-search.md）

1. 分析主题、定领域，查 source routing。
2. 按 tier 选源：永远先试 T1，不足才升级 T2，T3 为最后手段并明确警告。
3. 并行调用相关 MCP 检索工具：生物医学→`pubmed_search_articles`；跨学科→`search_crossref`；预印本→`search_arxiv`/`search_biorxiv`/`search_medrxiv`；穷尽式加 `search_semantic_scholar`/`search_webofscience`/`search_scopus`。
4. 用 Dedup Engine 对合并结果去重。
5. 按用户偏好排序（relevance / date / citations；系统综述可用组合分 `score = relevance*0.5 + recency*0.3 + citations*0.2`）。
6. 以统一表格呈现：Title / Authors / Journal / Year / DOI / PMID / Citations / Abstract 前 200 字符。

### WF2 引文核对（wf2-citation-verification.md）

1. 用 Citation Parser 从 .docx/.tex/.txt 抽取引文（.docx 需跳过 EndNote 域代码 `ADDIN EN\.\w+`、Zotero 域代码，剥离 `{citation:\d+}` 标记；正则可识别 DOI、PMID、PMCID、arXiv ID；.tex 抽取 `\cite{}`/`\bibitem{}`/`@article{` key 并从 .bib 解析，无 .bib 则标 `manual_needed`）。
2. 逐条解析：DOI→`search_crossref`/`get_paper_by_doi`；PMID→`pubmed_fetch_articles`；arXiv ID→`search_arxiv`；标题+第一作者→`pubmed_search_articles`/`search_crossref`。首选 T1 验证（CrossRef DOI → PubMed PMID 确认），T2（Semantic Scholar）用于交叉核对。
3. 比对检索元数据与文档元数据（标题、期刊、年份）。
4. 分类：`verified` / `mismatch` / `not_found` / `suspicious`（元数据不完整）/ `manual_needed`。
5. 生成报告：汇总计数 + 每条状态、DOI/PMID、解析备注的明细表。

### WF3 MeSH 检索式（wf3-mesh-strategy.md）

1. 用 `pubmed_lookup_mesh` 探索主题相关 MeSH 词；2. 展示词层级（broader/narrower/related）；3. 构造 Boolean 查询：MeSH 词 + 关键词（模板如 `("disease"[MeSH] OR "disease"[tiab]) AND ("treatment"[MeSH] OR "treatment"[tiab])`；>500 结果加过滤器，<10 放宽）；4. 可选 `pubmed_spell_check` 拼写检查；5. `pubmed_search_articles` 执行。产出：最终 PubMed 查询串、结果数、top 结果。

### WF4 引文文件管理（wf4-citation-file-mgmt.md）

- 离线转换特例：用户提供完整记录时，读 `ris-bibtex-format.md` 直接转换，并检查记录数、作者顺序、标识符、转义；不抓元数据、不做 preflight。
- 在线下载：用 `scripts/format-converter.py`，支持 `--pmid`、`--doi`、`--arxiv`、`--input refs.txt` 批量（refs.txt 行格式：`PMID:`/`DOI:`/`ARXIV:`/`QUERY:`/`AUTHOR:x TITLE:y`，`#` 为注释），输出 `.nbib`（MEDLINE）/`.ris`（EndNote/Zotero）/`.bib`（BibTeX）/`.enw`，存到 `./references/` 目录，验证输出数与输入数一致。
- 格式规范要点（ris-bibtex-format.md）：RIS 每条以 `ER  -` + 空行结尾；BibTeX citation key 用 `pmid<PMID>` 或 `firstauthorYEAR`（重名加 a/b）；`@article` 必填 author/title/journal/year；清理规则：作者字段 `&`→`\&`、特殊字符标题包 `{}`、摘要去 HTML 标签、页码 `-`→`--`、按 citation key 字母排序；自由文本字段做 ris_escape（去 HTML、归一空白、RIS/ENW 摘要截断 500 字符）。格式选择：EndNote→ENW/RIS，Zotero→RIS，LaTeX→BibTeX，PubMed/MEDLINE→.nbib。

### WF5 参考文献管理（wf5-reference-mgmt.md）

- **5a 找相关文献**：`pubmed_fetch_articles` 取源文元数据 → `pubmed_find_related` 发现相关 → 按相关性/日期/期刊过滤 → Dedup Engine 去重 → 带上下文备注呈现。
- **5b BibTeX 生成**：DOI→CrossRef→BibTeX；PMID→PubMed→BibTeX；批量走 format-converter.py；按 citation key 去重、排序、校验必填字段。
- **5c ID 转换**：接受 DOI/PMID/PMCID（至多 50 个），`pubmed_convert_ids` 转换，新解析出的 ID 再 `pubmed_fetch_articles` 取元数据。
- **5d 引文格式化**：PMID → `pubmed_format_citations` 输出 APA/MLA/BibTeX/RIS。
- **5e 全文获取**：`pubmed_fetch_fulltext`（PMC JATS 结构化全文）→ 付费墙回退 `download_paper`；报告结构化文本/PDF 文本/仅元数据。

### WF6 严格他引影响力审计（wf6-strict-other-citation-impact-audit.md）

**分类标签**（保守，证据不足不得升级）：`confirmed_strict_other_citation`（无作者重叠、无同实验室/团队/机构冲突、无近期合作tie）、`probable_external_citation`（无可见重叠但元数据不全）、`self_or_team_citation`、`collaborator_or_affiliation_overlap`、`unknown_metadata`。默认口径比"非自引"更严：排除直接自引、同团队引、明显近合作网络引。

**流程**：
1. 解析目标论文：标题、DOI、PMID/arXiv、年份、期刊、作者表、通讯作者、机构、ORCID/Scopus Author ID 等；目标有歧义时列候选并请用户给 DOI 或精确标题。
2. 检索施引文献并声明来源覆盖：优先 Scopus / WoS / Semantic Scholar / OpenAlex / CrossRef cited-by / 出版社 cited-by 页；多源 + Dedup Engine 去重；报告哪些源查了、哪些不可用。
3. 判定严格他引：规范化姓名 + ORCID/作者 ID 比对作者重叠；查机构、实验室/中心名、通讯作者组、近期合作 tie；按标签分类并附排除证据。
4. 抽取 citation context：优先全文（PMC JATS、ScienceDirect、出版社 HTML、合法 PDF 文本）；在参考文献表定位目标文献→映射文内引用标记→抽取引用句及前后各一句；仅有摘要时设 `citation_context: unavailable`，**不得从标题相似度推断措辞**。
5. 识别高影响力独立引用者（只对 confirmed/probable 两类做）：身份证据顺序为 ①官方高校/科学院/政府/基金委/学会页面 ②官方 fellow/人才计划名单、编委页、任命公告 ③ORCID/Scopus/Semantic Scholar/Google Scholar 稳定档案 ④新闻/个人页仅作辅助。认可信号：院士、校长/副校长、院长/系主任、杰青/长江学者/优青、IEEE/AAAS/RSC/ASCE Fellow、高被引学者、重要编辑职务等。同名需靠机构、领域、ID、合作者消歧，证据不足标 `unverified`。
6. 评估引用方式：function 分类（`background_support`/`method_used`/`data_or_code_used`/`benchmark_or_comparison`/`direct_extension`/`replication_or_validation`/`limitation_or_critique`/`review_summary`/`incidental_mention`）+ stance（`positive`/`neutral`/`critical`/`mixed`/`not_assessable`）；不得仅因被引就推断赞扬。
7. 生成审计报告：先讲覆盖与局限；高影响力引用者与普通施引文献分开；每个身份声明都挂证据，不允许无证据的"大牛"标签。

**Article Summary Table Mode**（用户要求整理成表时）：列为 `Article title | Publication date | Authors | Author affiliations | Citation count | Strict other-citation count | DOI | Evidence/notes`（中文表头：文章名/发表时间/作者名/作者机构/引用数/严格他引数/DOI/证据备注）。计数规则：先解析目标再计数；引用数取单一来源（优先 Scopus>WoS>Semantic Scholar>OpenAlex>出版社/CrossRef）并在 evidence note 中命名，不混算；严格他引数只计 `confirmed_strict_other_citation`；元数据不足时填 `not_assessable`。表后必附来源说明（计数来源/引用数来源/严格他引判定来源/未覆盖来源）。

## 四、输出契约与质量检查

- WF1：统一文献表（Title/Authors/Journal/Year/DOI/PMID/Citations/Abstract 前 200 字符），带来源标签。
- WF2：核对报告 = 汇总（total/verified/mismatch/not_found/suspicious/manual_needed）+ 明细表。
- WF3：最终 PubMed 查询串 + 结果数 + top 结果。
- WF4：`.nbib`/`.ris`/`.bib`/`.enw` 文件存 `./references/`，验证记录数匹配、作者顺序、标识符、转义。
- WF6：固定文本结构报告（Target paper → Coverage → Citation classification summary → High-profile independent citers → All citing-paper classifications 表 → Evidence gaps/next checks）。
- 验证脚本：`python scripts/preflight.py`（检查 PubMed E-utilities / CrossRef REST / arXiv 三端点连通性）；`python scripts/format-converter.py --test`（验证转换流水线，纯 stdlib）。

## 五、关键规则与红线

- 不虚构缺失字段；仅当验证或缺字段需要时才检索元数据（routing-and-ops.md）。
- T3 来源（Google Scholar 抓取、WoS/Scopus 机构代理、CNKI/万方）必须警告用户"结果可能不完整或过时"。
- 报告具体工具失败并继续其余工具；脚本失败 2 次后回退手工生成。
- WF6 红线（原文 Red Lines）：
  - 无同一人证据不得称某人为院士、Fellow、院长、校长、杰青、长江学者等。
  - 无机构或作者 ID 支撑，不得把同名当同人。
  - 不得仅凭被引数/h-index 证明引用者是"大牛"——只能作计量学上下文呈现。
  - 全文不可用时，不得从摘要、标题或参考文献表存在推断 citation context。
  - 不得隐瞒来源覆盖限制：Scopus/WoS/Semantic Scholar/OpenAlex 不可用或不完整时，报数之前先声明。
- 不把 Elsevier API key 写入仓库文件；本地版默认根本不配置该 key，用户主动提供时才只存本机安全配置（README + routing-and-ops）。
- **本地版边界**：不启动/连接机构代理、不借道浏览器登录态、不对 Google Scholar/CNKI 做抓取；检索是对公开服务的只读查询，除查询词与礼貌邮箱外不向任何第三方发送数据。
- 对 OpenAlex 礼貌使用：传 `--mailto` 或设 `OPENALEX_MAILTO`/`CROSSREF_MAILTO`。

## 六、配套资源

**references/**：
- `source-tiers.md` — T1/T2/T3 来源可靠性分级、速率限制与回退路由规则。
- `dedup-engine.md` — 统一去重：DOI 主键（正则 `10.\d{4,}/[^\s]+`，规范化小写）→ 缺 DOI 时用"标题规范化 + Jaccard ≥ 0.90 + 第一作者姓相同"回退键，及跨源合并偏好（元数据完整 > 出版社源 > 被引数高）。
- `citation-parser.md` — 按 .docx/.tex/.bib/.txt 格式的引文抽取策略、正则、解析优先级与五类分类标签。
- `search-strategy.md` — 查询构造（概念→同义词→MeSH→Boolean）、分领域模板、来源选择决策树、结果排序。
- `ris-bibtex-format.md` — RIS/BibTeX/ENW 模板、MEDLINE 字段映射、清理与转义规则、格式选择指南。
- `pubmed-28344011.bib/.nbib/.ris` — 同一 PubMed 记录的三种格式样例。
- `workflows/wf1..wf6` — 六个 workflow fragment（见上）。

**scripts/**：
- `academic_search.py` — no-MCP 回退检索，stdlib-only 直连 OpenAlex（免费无需 key），支持主题/作者/机构消歧/ORCID，输出排序 JSON。
- `format-converter.py` — 多源引文下载转换器（PubMed E-utilities / CrossRef / arXiv Atom → .nbib/.ris/.bib/.enw），支持批量 refs.txt 与交互模式。
- `converters.py` — 格式转换函数库（MEDLINE/CrossRef JSON/arXiv XML → RIS/BibTeX/ENW，含 ris_escape）。
- `preflight.py` — 批量网络操作前的 API 端点连通性检查。

**其他**：
- `mcp-server/` — academic_search MCP server（本地进程；`academic_search_server.py` + sources/ 下 arxiv、crossref、pubmed、scopus、sciencedirect、elsevier_common 六个源适配器 + utils）。**本地版默认不启动、不安装**，主流程走 `academic_search.py` no-MCP 回退；其中 scopus/sciencedirect/elsevier_common 三个付费源适配器在本地版视为休眠（无 key、不配置）。
- `config/` — MCP/settings/triggers 配置片段（`mcp-snippet.json`、`settings-snippet.json`、`triggers-academic-search.toml`）；**本地版不写入任何 agent 配置，仅在用户明确要求启用本机 MCP 时作参考**。
- `agents/openai.yaml` — agent 装配声明。
- 无独立 evals 目录。
