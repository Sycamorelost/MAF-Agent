# nature-ref-verifier（本地版）

> **本地化改造说明**：本版**不连接任何第三方软件或服务**——不安装/调用 pyzotero、zotero-cli、zotero-mcp，不访问 Zotero 本地 API/Web API，不接收 Zotero item key，不回写 `zotero.sqlite`；不使用 kimi-datasource / kimi-webbridge 等带登录态或私域的插件。文献元数据只从**公开、免费、无需凭据**的渠道只读拉取（CrossRef API、出版社公开页面、通用网页检索）；需要库内比对的条目，请用户自行从 Zotero 等管理器**导出成 BibTeX/RIS 文件**后作为普通文件输入。产出只有"给用户看的内容"：Markdown 审查报告、BibTeX patch、供人工执行的修改建议清单——不向任何软件推送修改。

## 一、定位与触发场景

nature-ref-verifier 是 nature-skills 仓库中体量最小的一个技能，专门做**学术参考文献的多源交叉验证**：对文献条目逐字段（作者、标题、年份、卷期、页码、DOI）比对多个独立来源，按严重程度分级标记差异，输出可直接执行修正的结构化报告。目录：`skills/nature-ref-verifier/`。

SKILL.md frontmatter 中的 description 原文（中文）：

> 对学术文献逐条执行多源交叉验证，逐字段对比作者、标题、年份、卷期、页码，标记卷年/DOI年冲突、作者顺序异常、页码偏差等问题，输出结构化验证报告。可批量处理整篇论文/开题报告的参考文献列表，也可单条校验，支持与 Zotero 同步修正。

**本地版执行口径**：前半句（多源交叉验证 + 结构化报告）原样保留；末句"支持与 Zotero 同步修正"在本地版改为"输出可供人工执行的修改建议清单与可导入的 BibTeX patch，不与 Zotero 建立任何连接"。

**典型触发请求**（SKILL.md 明确列出的触发词）：`verify references`、`校验文献`、`check references`、`核对参考文献`、`文献验证`、`ref check`。

README 给出的典型说法：

- “帮我逐条检查这 50 条参考文献，标出必须修正的。”
- “这几条 DOI 好像不对，帮我查真实题名和页码。”
- “把 BibTeX 里的错误字段整理成 patch 建议。”

**适用场景**（SKILL.md 原文）：开题报告/论文提交前对全部参考文献做最终核查；收到审稿意见指出引用错误时精准定位问题；从旧项目迁移参考文献到新论文时批量验证有效性；Zotero 库定期健康检查（**本地版**：用户先自行导出 BibTeX/RIS，本技能只出报告与 patch，不直接连库体检）。

**明确边界（不做什么）**：

- 不会把单一搜索结果当作最终事实；关键字段必须回到 DOI、出版社或权威数据库核对。
- DOI、卷期和在线发表年份不一致时，**解释冲突而不是强行合并**成一个值。
- 无法访问的数据库或中文来源会被标注为"未验证"，不编造验证结果。
- 产品手册/专利不验证元数据，只确认来源可访问。

## 二、运作模式（加载/路由机制）

这是 6 个技能中加载机制最简单的一个，没有静态/动态分层、没有轴（axis）。

**SKILL.md 自身的路由约定**：SKILL.md 即核心工作流本体（约 196 行），始终整体生效。`references/common-patterns.md` 是唯一的按需资源，SKILL.md 开头明确规定：

> 需要处理批量差异、反复出现的字段错误、DOI/标题冲突、作者顺序异常、页码漂移或报告措辞时，读取 `references/common-patterns.md`。其他任务不要预加载该文件；`manifest.yaml` 记录了同一按需路由，新增资源时必须同时在本文件中写明加载条件。

也就是说：SKILL.md 与 manifest.yaml 的按需路由必须**双写保持一致**，新增资源两边都要登记。

**manifest.yaml 结构**（version 1.0.1）：

- `always_load:` 只有一项——`SKILL.md` 自身。设计意图写在注释里：验证任务可以只跑单条引用，也可以跑整篇文献列表，默认路径保持轻量，只有模糊或批量场景才打开模式库。
- 没有 `axes:` 段（无内容轴）。
- `references.on_demand:` 只有一条映射——条件为"诊断反复出现的文献错误、字段级不匹配模式、DOI/标题冲突、作者顺序异常、页码区间漂移，或批量报告措辞"时加载 `references/common-patterns.md`。

**对 nature-shared 共享层的依赖**：无。manifest 和 SKILL.md 均未引用 `../nature-shared/` 下任何文件，是本批 6 个技能中唯一完全自包含的一个。

**agents/openai.yaml**：仅定义展示层——display_name 为 "Nature Reference Verifier"，short_description 为 "Verify scholarly references across independent sources"，并给出默认 prompt。

## 三、核心工作流（实际功能流程）

SKILL.md 定义了五步流程，全部字段名、阈值和规则如下。

### Step 1: 解析输入

支持三种输入格式：

- **A. 整篇论文/开题报告**：提取所有 `[N]` 编号的参考文献列表，自动识别每条的 DOI（如有）。
- **B. 单条参考文献**：用户直接粘贴一条引用文本。本地版不接收 Zotero item key（不连接 Zotero）；库内条目请用户粘贴文本或提供其导出的 BibTeX/RIS。
- **C. BibTeX 文件**：直接解析 `.bib`，逐条校验。

### Step 2: 多源并行查询

**Step 2.0（最便宜、最先做）：DOI 可解析性检查。** 对每条有 DOI 的文献先请求 `https://api.crossref.org/works/<DOI>`：返回 404 → DOI 本身错误（高度疑似错号）；返回 200 → 比对返回记录的标题/作者/卷期页与条目，不一致即判定"DOI 张冠李戴"。SKILL.md 强调这是单次请求即可发现实践中最常见错误的手段（一次 35 条核查中 7/9 的错误属于此类）。

随后对每条文献并行查询以下来源（可用性取决于环境配置）：

| 来源 | 适用文献 | 典型覆盖率 |
|------|---------|-----------|
| **Crossref**（公开免费 API） | 有 DOI 的外文期刊 | ~70%（IEEE 老文献、中文期刊常有缺失）|
| **IEEE Xplore 公开页面** | IEEE 期刊/会议 | IEEE DOI 404 时的备选 |
| **WebSearch（Bing/Google 等通用检索）** | 无 DOI 或来源缺失的文献 | 兜底方案 |
| **CNKI/万方公开检索页**（仅无需登录的公开元数据） | 中文期刊、学位论文 | 中文文献必查；查不到即标"未验证" |

> 本地版删除原表中"Zotero 本地库（zotero-mcp）"一行，CNKI/万方查询不再经 kimi-datasource / kimi-webbridge 等带登录态插件，只访问公开页面；查不到的条目标 ❓ Unverifiable，不寻求登录态通道。

**查询策略**：优先用 DOI 查 Crossref，失败后降级用标题+作者搜 Web。

**批量策略**：超过 20 条时按 10–15 条一组拆给并行子代理，每组独立查询后汇总——SKILL.md 注明 35 条规模实测 3 个并行子代理可在一次往返内完成，串行会慢一个数量级。

### Step 3: 字段级对比（三级严重程度）

**🔴 Critical（必须修正）**：作者姓名/顺序不一致（第一作者不同、顺序颠倒、完全编造，典型案例 Smogavec→Leckebusch、Vainikainen→Mikhnev）；作者漏人（条目作者数少于出版方记录，常见于 5+ 作者只录前 3-4 人，如 TGRS 2019 PCA 文漏第 5 作者 Ge）；页码差异 ≥5（Noon: 1309-1319→1444-1449）；文章号字母误判（T/7、l/1、O/0 形近字符，如 SPIE 101880T 误作 1018807）；标题核心词不一致；DOI 指向不同论文（Abidi 1995: 10.1109/4.482157→10.1109/4.482187）；DOI 张冠李戴（DOI 可解析但解析到另一篇论文）。

**🟡 Warning（建议核对）**：卷年 ≠ DOI 年（Dadrass 卷2025/上线2024）；Early Access 元数据漂移——IEEE 等期刊 Early Access 转正式出版后卷/期/页/年会变化，**以 CrossRef 现值为准**（TGRS autoencoder 文 59(12):10437-10449,2021 → 60:1-13,2022）；作者中间名缺失/多余；期号缺失（Smogavec 2025: 17→17(10)）；页码偏差 ≤4；出版社名写法差异（"IET" vs "IEE"）。

**🟢 Info（仅供参考）**：标题大小写不同；期刊名缩写 vs 全称；标点/连接词差异。

### Step 4: 置信度评估

每条文献给出一个综合置信度：✅ **Verified**（多源一致，无需修改）；⚠️ **Check suggested**（存在 🟡 级差异，需人工判断）；❌ **Needs fix**（存在 🔴 级差异，必须更正）；❓ **Unverifiable**（所有来源均无法查到，如内部报告、老旧学位论文）。

### Step 5: 输出报告

三种输出格式：

1. **Markdown 摘要报告**：先给总数统计（Verified / Check suggested / Needs fix / Unverifiable 各多少条），再用两张表分别列出"必须修正"（列：#、问题、当前值、正确值、来源）和"建议核对"（列：#、问题、详情）。
2. **BibTeX Patch**：直接生成修正后的 `.bib` 文件内容。
3. **手工修改建议清单（本地版替代原"Zotero 更新指令"）**：按条目列出"当前值→建议值+证据链接"的修改指令表，供用户在 Zotero/EndNote 或稿件中**自行手工**执行；不调用任何写入通道。原 SKILL.md 记录的 Zotero 写入路径知识仅作历史背景、本地版一律不使用：
   - ~~Zotero 7 本地 HTTP API（localhost:23119）~~：本地版不连接（其写操作本身也返回 501，只读）；
   - ~~pyzotero Web 模式（需 userID + API key）~~：本地版不安装、不配置、不调用；
   - ~~`/connector/saveItems` 端点新增条目~~：本地版不调用；
   - ~~直接改 `zotero.sqlite`~~：本地版不触碰任何第三方软件的数据文件；
   - 用户如确需批量入库，自行用本技能产出的修正版 BibTeX 在 Zotero 里导入。

## 四、输出契约与质量检查

**必产出**：字段级核查表（原始值、可信来源值、差异和证据链接）；严重程度四级分级结论；可选 BibTeX patch、**供人工执行的修改建议清单**（本地版不回写任何软件）或 Markdown 审查报告；对不确定条目的人工核查清单。

本技能**没有 scripts/ 目录和自动化验证脚本**——验证靠多源交叉查询本身完成，质量控制体现在对比矩阵和置信度分级上。SKILL.md 内嵌一张"多来源交叉验证策略"ASCII 流程图（用户输入 → 字段拆分 author/title/year/vol/pg → CrossRef/IEEE Xplore/WebSearch 三路并行 → 字段级对比+差异评级 → 置信度评估 ✅⚠️❌❓），作为执行时的心智模型。

**批量实测经验**（SKILL.md 原文注明）：35 条规模用 3 个并行子代理即可在一次往返内完成；一次 35 条核查中 7/9 的错误属于"DOI 张冠李戴"，这正是 Step 2.0 被放在最前的原因。

**README 补充的产出形态**：字段级核查表（原始值、可信来源值、差异和证据链接）、四级严重度分级、可选 BibTeX patch / 手工修改建议清单 / Markdown 审查报告，以及对不确定条目的人工核查清单。用户需提供：参考文献列表/单条引用/BibTeX/RIS（本地版不接收 Zotero item key）或论文参考文献页；本地版数据源固定为公开免凭据渠道（Crossref、PubMed、IEEE/出版社公开页面、CNKI/万方公开页、通用网页检索）；可按需产出可直接导入的修正 BibTeX 文件。

## 五、关键规则与红线

1. **不轻信单一来源**：任何字段的"正确值"必须有 DOI 解析记录、出版社页面或权威数据库支撑。
2. **Early Access 以 CrossRef 现值为准**，不沿用预印本或 Early Access 旧元数据。
3. **卷年优先原则**（common-patterns.md）：DOI 编号年通常是收稿年，引用应以期刊官方卷年为准；无卷号时以上线年为准。
4. **IEEE 老文献 DOI 404 不应标记为错误**——2000 年前的 IEEE 论文 DOI 常未在 Crossref 注册，需转 IEEE Xplore 查。
5. **中文 DOI 不在 Crossref**：中文期刊 DOI 注册在 CNKI 系统，API 查询必然返回空，应查 CNKI/万方**公开页面**；公开渠道查不到就标 Unverifiable，不借道登录态插件。
6. **西班牙/葡萄牙双姓的多种缩写都算正确**（`Álvarez López Y` = `López Y Á`），不误报。
7. **中文引用用"等"是可接受的**；若列全部作者则顺序必须与原文一致。
8. **零第三方软件连接（本地版红线，替代原 Zotero 写入红线）**：不安装/调用 pyzotero、zotero-cli、zotero-mcp，不访问 localhost:23119 与 Zotero Web API，不改 `zotero.sqlite`，不接收 item key，不使用 kimi-datasource/kimi-webbridge；一切修改只以报告/BibTeX patch 形式交给用户人工处理。
9. 已知局限的降级路径：中文 DOI → CNKI/万方公开页；学位论文无 DOI → CNKI/万方公开页/ProQuest 公开摘要验标题+作者+年份；谷歌学术搜不到 → 切换 Bing、百度学术、Semantic Scholar；所有渠道都查不到 → 标 ❓ Unverifiable，绝不编造验证结论。

## 六、配套资源

- `references/common-patterns.md`：唯一按需参考，8 类常见错误模式库——卷年与 DOI 年不一致（3 种子模式及"卷年优先"判断规则）、作者名问题（编造/顺序颠倒/中文形近字如廷↔延/双姓缩写/"等"用法）、页码错误（预印本 vs 正式出版、串行粘贴）、DOI 问题（指向错文、IEEE 404、中文 DOI、多 DOI 取正式出版版）、会议论文年份（取实际开会年份）、学位论文（中文格式 `作者. 论文名[D]. 学校, 年份.`、万方覆盖率高于知网）、产品手册用 `[Z]` 标记、出版社历史名称（2006 年前 IEE 不标 IET）。
- `agents/openai.yaml`：OpenAI 侧展示配置（display_name、short_description、default_prompt）。
- `README.md` / `README_EN.md`：中英双语使用说明，给出典型请求、用户需提供物（文献列表/BibTeX/RIS、可用公开数据源、是否需要修正文件；**本地版移除 Zotero key 输入**）和产出说明，并列出相关技能（nature-academic-search、nature-citation、nature-response）。
- 无 scripts/、templates/、examples/、evals/ 目录。

**环境依赖（本地版口径）**：原 SKILL.md 声明的可选依赖——`pyzotero[cli]`、`zotero-cli`/`zotero-mcp`、`kimi-datasource`、`kimi-webbridge`——在本地版**全部不安装、不调用**；只保留基础 WebSearch/FetchURL 类通用只读检索能力即可运行。
