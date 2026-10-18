# nature-downloader（本地版）

> **本地化改造说明**：本版**只保留"只读拉取公开内容"的路线**——开放获取（OA：PMC、Unpaywall、arXiv、出版商 OA 页、合法开放仓储）与用户**主动提供 API key** 时的出版商 API（Elsevier/Springer Nature/IEEE）；这些路线不连接、不控制任何第三方软件，也不上传/推送任何数据。**停用一切"连接外部浏览器软件"的路线**：不启动/控制 Chrome、不开远程调试端口（CDP）、不复用浏览器登录会话、不读取 cookie/profile、不做 CNKI 机构登录态下载、不做 Web Access/WoS 机构授权路由、不跑反机器人自动验证；不配置学校/图书馆入口。凡只能通过机构登录获取的付费文献，本地版只报告状态与合法 OA 替代建议，由用户自行获取后把文件放入本地目录。

## 一、定位与触发场景

**用途（本地版）**：通过合法、公开、无需登录的路径下载学术全文——开放获取（OA）为主，出版商 API 仅限用户主动提供 key 时启用；支持 Supporting Information（SI）下载；产出带审计 manifest 的本地化全文文件。

**description 原文摘录**（SKILL.md frontmatter）：

> "Use when a user needs lawful academic full text, CNKI institutional access, English OA retrieval, publisher API access, institutional browser fallback, or supporting information downloads."

**本地版执行口径**：保留 "lawful academic full text"、"English OA retrieval" 与用户主动给 key 时的 "publisher API access"；"CNKI institutional access" 与 "institutional browser fallback" 停用。

metadata.compatibility 原文要点："Requires Node.js 22+ and Python 3. CNKI, Web Access, and SI routes additionally require the user's authenticated Chrome session and remote debugging. Uses only lawful OA, publisher API, and user-authorized institutional access."——本地版只需 Node.js 22+/Python 3 本地运行时与 OA/出版商 API 路线；"authenticated Chrome session and remote debugging" 相关要求全部删除。

**典型触发请求（本地版）**：按 DOI/题名批量下载 **OA** 文献正文（±SI）；arXiv/PMC/出版社 OA 页全文获取；用户自带 Elsevier/Springer Nature/IEEE API key 时走 API；中文文献只找公开免费的合法副本，找不到即如实报告（不做机构登录态下载）。

**明确不做什么（边界）**：不绕过付费墙/DRM/双因子认证；不使用镜像站；不把宽泛关键词检索变成无限量自动下载；不下载整个期刊 issue/volume/大结果集；没有合法权限时只报告状态、失败原因和可能替代路线。

## 二、运作模式（加载/路由机制）

### SKILL.md 作为路由

本 skill 的 SKILL.md 本身是完整路由器（255 行），非薄壳。manifest 的 `always_load` 只有 `SKILL.md` 本身；无 axes、无 static/ 层。SKILL.md 内置若干**强制门**，再给两条按需 references（本地版仅其中验证/失败处置部分适用）：

- `references/institutional-browser-workflow.md` — **本地版整体停用、不加载**：其内容（Web of Science、机构授权浏览器会话、出版商验证、认证 handoff、浏览器上下文 PDF 传输）全部依赖控制用户 Chrome，本地版不做这些事。
- `references/delivery-verification-and-failures.md` — 当用户要求 SI、已下载文件需最终验证与命名、或某次 OA/API 访问进入 typed failure/retry 状态时加载；其中涉及浏览器/机构会话的处置段落不适用。
- 第二条 reference 被加载时，SKILL.md 的边界、SI 确认门、状态语义、token 纪律仍然强制；references 不构成绕过访问控制或暴露凭据的许可。

manifest `references.on_demand` 还把脚本列为条件资源。**本地版适用**：首次使用说明 → `README.md`；出版商凭据（仅用户主动给 key）→ `scripts/configure_credentials.py`；批量下载 → `scripts/batch_download.mjs`；出版商 API 路由/OA 回退/provider 状态 → `scripts/lib/publisher-providers.mjs`；状态码语义 → `scripts/lib/status-codes.mjs`。**本地版停用**：`scripts/configure_school.py`（学校/图书馆入口配置）、`scripts/browser_pdf_downloader.mjs`（CDP 抓 PDF）及一切 CDP/反机器人脚本。

### 对 nature-shared 的依赖

无引用。

## 三、核心工作流（实际功能流程）

### 0. SI 确认门（先于一切下载）

下载任何 PDF/CAJ/HTML/XML/附件前必须问一次："是否同时下载这些文献的 Supporting Information（SI，补充材料）？"。明确要 SI 算 yes，明确只要正文算 no，否则整批问一次。脚本必须带 `--si` 或 `--no-si` 恰好其一；都没有则返回 `si_confirmation_required` 且不创建输出目录；同时给两个则参数校验失败。批量任务的选择作用于整批。

### 1. 主路由（语言 + 出版商 + 凭据）

规范化 DOI/题名、识别语言与出版商后：

```text
├─ 中文文献：本地版不走 CNKI 机构登录态。只尝试公开免费的合法副本（CNKI/万方公开免费页、出版商 OA、合法仓储、明确开放的 PDF URL）；
│   找不到即记 no_authorized_pdf_found / oa_not_found 并报告，不启动浏览器、不复用登录会话
└─ 英文文献
   ├─ Elsevier / Springer Nature / IEEE 且【用户主动提供】有效 Key
   │  ├─ 可优先出版商 API 下载；成功记 accessMode: publisher_api、oa_status: not_checked_api_first，不再做 OA 判定
   │  └─ API 失败（无 entitlement/无可用全文）：自动查文章级合法 OA（PMC → Unpaywall → 出版商 OA 页 → arXiv → 合法仓储/明确开放 PDF URL），manifest 保留失败 API 尝试
   └─ 其他出版商/未提供 Key：先查 OA；OA 不可用 → 本地版即止，报告 no_authorized_pdf_found 与替代建议（原 Web Access 机构授权路由停用）
```

- API 与 OA 双失败后本地版**不再询问**是否用 Web Access（该路由已停用）：直接报告状态、失败原因和用户可自行获取的合法路线。
- 显式 `--route open_access` 时，OA 结果后即停。
- OA 检查无法确认时 manifest 记 OA assessment 为 `unknown`，不误标为非 OA。
- 精确题名或无 DOI 的会议/arXiv 论文优先 `--title "<exact title>" --open-access` 或 `--pdf-url`；`--topic` 在原 skill 中是 WoS 主题检索，本地版不使用 WoS，批量检索只用于 OA 来源，且仍需用户确认明确清单。

### 2. 首次配置（懒配置，仅当路由需要）

**学校/图书馆（本地版停用）**：不询问、不配置任何机构门户/图书馆/WoS/统一认证链接；不运行 `configure_school.py infer|url|health`，不写 `~/.config/lit-dl/school.json`；原"Resource URL 分诊"（CAS/SSO、CARSI/Shibboleth、EZproxy、WebVPN、资源聚合门户等识别规则）随之全部不适用——本地版根本不进入这些登录阶段。

**出版商 API 凭据（仅用户主动提供时配置，不索取）**：用户主动发来 key 时，才用 `python3 scripts/configure_credentials.py set elsevier|springer_nature|ieee` 保存（IEEE 需 `--fulltext-endpoint` 模板；IEEE 普通 Metadata API ≠ 收费全文权限）。官方注册链接（用户如需自行申请）：Elsevier `https://dev.elsevier.com/`、Springer Nature `https://dev.springernature.com/docs/quick-start/api-access/`、IEEE `https://developer.ieee.org/member/register`。Elsevier 机构 token 可传 `--insttoken`/`--authtoken`。Unpaywall 需 `configure_credentials.py contact-email`（存非秘密的 `~/.config/lit-dl/settings.json`）。秘密存 `~/.config/lit-dl/credentials.json`（权限 0600，展示只显末四位）。**用户未主动提供 key 时，绝不询问、绝不引导注册，OA 路线照常运行。**

### 3. 前置条件（Preconditions，本地版）

本地版只有一个分支，前置条件 4 条：① 目标标识符/明确文献清单；② 用户已批准的本地输出目录；③ Node.js 22+（PATH 没有时试 `%LOCALAPPDATA%\OpenAI\Codex\bin\node.exe`）；④ PDF 验证需 Python 3（`pip install -r requirements.txt`）。**删除原"付费图书馆分支 8 条"**（登录态浏览器、学校配置、CDP profile、web-access CDP 代理等）——本地版不要求、不检查、不引导这些条件；`src/wizard.py` 学校配置向导不运行。

### 4. Fast Batch Path（2 篇以上默认）

`scripts/batch_download.mjs` 为编排入口；本地版只使用其中的 OA 与出版商 API 路径（本就不需 CDP）；CNKI 登录态/Web Access/SI 经认证浏览器抓取的惰性附加逻辑不触发。用法示例：

```bash
node scripts/batch_download.mjs --dois "10.1007/s00122-021-03957-1,10.1111/pbi.14066" --no-si --out "<project>"
node scripts/batch_download.mjs --title "Attention Is All You Need" --open-access --no-si --out "<project>"
node scripts/batch_download.mjs --pdf-url "https://arxiv.org/pdf/1706.03762" --title "..." --no-si --out "<project>"
```

（本地版删除原示例中的 WoS `--topic` 检索与 CNKI 机构下载两条命令。）

输出 `{ summary, manifest, results }`；写 `<project>/manifest.json`（路由、OA 证据、访问模式、格式、MIME、字节、SHA-256、SI 选择、typed failures；秘密字段递归移除）。目录分工：PDF 入 `PDFs/`、原生 HTML/XML 入 `FullText/`、补充材料入 `SupportingInformation/`（原 CAJ 入 `CNKI/` 的分支随 CNKI 登录态路由一并停用）。

**Token 纪律（全路径强制）**：绝不把整页 DOM、检索结果、PDF/SI 字节 `eval` 回 agent 上下文；大数据留在 Node 脚本内，只上浮紧凑状态。本地版无交互式 `/eval`、无 `cdp_open_url.mjs` 诊断路径。

### 5. Web Access 机构浏览器工作流（本地版整体停用，以下为原流程存档，不执行）

> **本地版不执行本节任何步骤**：不打开 WoS、不做机构认证/handoff、不跑反机器人自动验证、不经浏览器上下文抓 PDF；下列脚本（`browser_pdf_downloader.mjs`、`cdp_open_url.mjs`、`lib/anti-bot.mjs` 等）一律不运行。原文保留仅为说明原 skill 的能力边界。OA/API 都拿不到的文献，按第 1 节直接报告 `no_authorized_pdf_found` 与合法替代路线。

1. 经图书馆聚合/机构入口打开 WoS 并一次性认证（WoS host：`webofscience.clarivate.cn`、`www.webofscience.com`、`*.webofknowledge.com`、`*.clarivate.com`；WoS 记录渲染在 shadow DOM + 虚拟列表中，手工抓取须穿透 shadow root 并滚动加载——批脚本已处理）。
2. 确认已认证检索页（可见机构名+检索框）。
3. 优先按 DOI 检索，否则精确题名；用 `/eval` 读结果页，按 标题+年份+作者 选记录。
4. 打开记录，按偏好顺序点全文路由：`Free Full Text`/`Open Access` → 图书馆 resolver（`Find it at`、`SFX`、`OpenURL`、`Full Text Links`、`查看全文`、Ovid 等）→ 出版商全文链接（`View Full Text`/`View PDF`）。全文链接继承机构会话；出现二次机构 handoff 就完成一次。
5. 出版商页找 PDF 链接（`PDF`/`pdfft`/`/doi/pdf/`），用 `scripts/browser_pdf_downloader.mjs` 保存。
6. resolver 只给可读 HTML 全文时：保存 HTML/文本，标 `full_text_html_available`，明确告诉用户"已获取 HTML 全文，但当前授权路径没有可下载 PDF"；**不得**把 HTML 误标为 PDF。
7. resolver 明确说机构无权限：标 `library_no_permission`，直告用户，不得藏在 `failed_after_retry` 后面，不得当作临时网络问题重试直连。
8. 默认不下载 SI（用户明确要求才取）。
9. manifest 记录路由（OA 来源 或 WoS → SFX/OpenURL/provider → 出版商/数据库）。

**出版商验证（ScienceDirect 等）**：反自动化挑战的预防——优先聚合/CARSI 入口而非 doi.org 直连；敏感出版商一次一篇；不并行开多出版商标签；等页面稳定再找 PDF；用户完成验证后复用同标签；默认一次尝试、同标签至多两次。挑战出现时先用内置反机器人模块 `scripts/lib/anti-bot.mjs`（简单点击挑战、ScienceDirect robot check、Cloudflare Turnstile、滑块含 CNKI Geetest、reCAPTCHA 复选框）；成功则继续并记 `verification_auto_passed`；失败则停该标签、记 `verification_auto_failed`（完全无法自动交互时用 `sciencedirect_robot_check`）、告知用户哪篇哪个标签；用户完成后同标签续试一次；挑战立即复现则 `do_not_auto_retry`。中断批量时维护 `publisher_verification.tsv`（表头：id/project/title/doi/year/venue/publisher/status/source_url/current_url/next_action/notes；next_action 建议值：user_complete_publisher_verification / retry_same_tab_after_user_confirms / try_aggregation_entry_route / try_authorized_oa_route / mark_do_not_auto_retry）。

**机构认证 handoff**：到达机构登录/联邦 WAYF/数据库登录/IP 登录页时：停自动操作 → 记 `carsi_retry.tsv` 状态 `carsi_waiting_user` → 精确告知用户哪个标签需要处理 → 不读/不存/不要密码、QR、OTP、cookie、storage → 用户明确授权且浏览器已填凭据时**只点一次**可见登录/确认按钮（不输入字段、不查隐藏值）→ 完成后同标签刷新继续 → 重新探测页面类型 → 成功则下载验证并更新 `downloaded`/`downloaded_with_si` → 登录后循环回原登录页则记 `failed_after_retry` 附观察原因。**Safe Institutional Auto-Confirm** 五条件全满足才可点：预期机构域名、当次会话明确授权、按钮确为 登录/确认/Continue/Proceed/Sign in 类、无 QR/SMS/OTP/推送确认/密码重置/新数据共享同意/账号安全警告、不读不存不输入凭据。CARSI 重试批一次只处理一个或少数标签。

**浏览器上下文 PDF 下载**：shell 直连返回 403/401/Cloudflare HTML/登录页而 Chrome 能开时，用 `browser_pdf_downloader.mjs`（在受控 Chrome 页面内 `fetch(location.href, {credentials:"include"})`，分块经本地 CDP 代理传出，写盘，默认校验 `%PDF` 签名；选项 `--url/--target/--out/--proxy/--close/--allow-non-pdf`）。

### 6. SI 抓取与交付验证（references/delivery-verification-and-failures.md；本地版仅 OA/API 范围）

本地版 SI 只在 OA 页/出版商 API（有 key 时）可达时抓取，方法与验证纪律不变；其中"WoS + `--si` 批路由"的目录纪律、shell 403 改走 `browser_pdf_downloader.mjs`、会话过期请用户重登等浏览器相关处置均不适用。

**SI**：仅在用户明确选择时抓。方法：打开文章 landing page（非仅 PDF 页）；抽取匹配 `Supporting Information`/`Supplementary`/`Supplemental`/`/doi/suppl/`/`/suppl_file/`/`_si_`/`mmc1`,`mmc2`（Elsevier 模式）的链接；下载所有明显合法的 PDF/DOCX/XLSX/视频/数据附件。WoS + `--si` 批路由要求：每篇独立可读题名文件夹；只放已验证主 PDF 与明确标注的 SI 文件；保留附件原名；补充落地页最多下钻一层；排除 GitHub/Zenodo/Figshare/Dryad/OSF 外部仓库链接；无 SI 时保留主 PDF 并报 `si.status = not_found`；部分 SI 失败报 `partial` 且不算主 PDF 失败。该整洁目录行为**不适用**于 CNKI、`--open-access`、裸 `--pdf-url`、裸 `--dois` 路由。ACS 回退 URL 模式（须先验证 DOI 与文章页）：`https://pubs.acs.org/doi/suppl/<DOI>/suppl_file/<journal-code>_si_001.pdf`；**不得**把猜出的补充 URL 当事实，404 则记 not found 并检查文章页。

**验证**：每个文件都验。PDF 用 `scripts/extract_pdf_text.py --pdf <path> --pages 3`（报页数与抽取文本，内部 UTF-8 重配以防 Windows GBK 故障；抽取失败但 PDF 有效时可试 PyMuPDF/OCR/本地 pdf skill）。最小清单：文件存在且大小合理；PDF 首字节为 `%PDF`；页数非零；抽取文本含标题/摘要/SI 标题；HTML 全文须含标题或 DOI 且在回复中声明无有效 PDF；多篇时保存含 DOI/标题/源 URL/下载日期/补充状态的小 manifest。Zotero 导入不替代本地 PDF 验证。

**命名**：`FirstAuthor_Year_Journal_short-title.pdf` / `..._SI.pdf`；项目目录 `文献自动下载/{manifest.tsv, PDFs/, SupportingInformation/, extracted_text/}`。

**典型故障处置**：shell 403 而 Chrome 能开 → `browser_pdf_downloader.mjs`（机构访问常态）；聚合入口无全文链接 → 试出版商 Institutional login/CARSI 路由、已建机构会话后试 DOI、只查合法 OA 副本，记 `no_authorized_pdf_found` 而非找未授权镜像；页面开成 `about:blank` → 先按 URL 片段/编码问题处理（含 `#`/`#!` 时用 `scripts/cdp_open_url.mjs --url "<full URL>" --wait`，不把含片段 URL 未加引号粘进 shell）；curl 不可用 → PowerShell `Invoke-WebRequest` 或优先 Node helper（`URLSearchParams` 正确保留嵌套片段）；会话过期 → 请用户在同一浏览器重新认证。

**首次运行需与用户确认**：地址栏实际出现的机构登录/联邦/代理/WebVPN/数据库 host；实际使用的聚合/数据库入口 base URL 与链接模式；是否出现 federation/WAYF/机构选择、IP 登录、数据库登录步骤及用户是否授权选择无歧义项。

### 7. 状态码（SKILL.md Status Categories，manifest 持久记录）

> 本地版说明：状态码清单是原 manifest 契约的存档；其中与浏览器/机构会话相关的状态（`carsi_waiting_user`、`carsi_resolved_retry_needed`、`publisher_verification_waiting_user`、`sciencedirect_robot_check`、`retry_after_user_verification`、`verification_auto_passed/failed`、`do_not_auto_retry`、`publisher_blocked_waiting_user` 等）在本地版**永不产生**；OA/API 拿不到时直接落 `oa_not_found`/`no_authorized_pdf_found`/`api_*` 类状态。

完整清单：`downloaded`、`downloaded_with_si`、`open_access_downloaded`、`full_text_html_available`、`available_not_downloaded`、`native_fulltext_downloaded`（CNKI CAJ、Springer JATS/XML 等）、`si_confirmation_required`、`credentials_missing`、`credentials_invalid`、`api_not_entitled`、`api_fulltext_unavailable`、`api_fallback_confirmation_required`、`oa_not_found`、`oa_resolution_inconclusive`、`metadata_ambiguous`、`carsi_waiting_user`、`carsi_resolved_retry_needed`、`publisher_verification_waiting_user`、`sciencedirect_robot_check`、`retry_after_user_verification`、`verification_auto_passed`、`verification_auto_failed`、`do_not_auto_retry`、`url_needs_repair`、`library_no_permission`、`no_full_text_link`、`publisher_blocked_waiting_user`、`no_authorized_pdf_found`、`failed_after_retry`。

## 四、输出契约与质量检查

- 必产出：按格式分目录的全文文件（`PDFs/`、`FullText/`、`SupportingInformation/`；本地版无 `CNKI/` CAJ 分支）+ `<project>/manifest.json`（规范 DOI、语言、出版商、路由、OA 证据、访问模式、格式、MIME、字节、SHA-256、SI 选择、typed failures；递归移除 API key/token 等秘密字段）。
- 命名规范见上（`FirstAuthor_Year_Journal_short-title[ _SI].pdf`）。
- 自检清单：最小验证清单（存在性/大小/`%PDF`/页数/文本含标题）；批量维护 `manifest.tsv`；原 `carsi_retry.tsv` / `publisher_verification.tsv`（规定表头）随浏览器路由停用，本地版不产生。
- 验证命令（README）：`python3 -m unittest discover -s tests/python`、`node --test tests/unit/*.test.mjs`、`node --check scripts/batch_download.mjs`（本地版不跑 `node --check scripts/browser_pdf_downloader.mjs` 等停用脚本）。

## 五、关键规则与红线

- **零第三方软件连接（本地版首要红线）**：不启动、不附加、不控制任何浏览器（不开 Chrome 远程调试/CDP 端口）；不读取或复用 cookie、localStorage、浏览器 profile、登录会话；不运行 `configure_school.py`/`wizard.py`、`browser_pdf_downloader.mjs`、`cdp_open_url.mjs`、`anti-bot.mjs` 等机构/浏览器脚本；不做 CNKI/WoS 机构登录态下载；不配置学校入口。下载只走匿名公开 OA 拉取与用户主动给 key 的出版商 API；拿不到就如实报告，绝不转向镜像或登录态通道。
- 只用合法路径；不绕过付费墙、DRM、双因子认证；不使用镜像站。
- ~~**Verification-first**（滑块/复选框/机器人校验自动尝试）~~：**本地版停用**——不与任何验证控件交互；遇到即停止该篇并报告。
- ~~**User handoff**（挑战失败后请用户在受控标签完成认证）~~：**本地版停用**——不持有浏览器标签，无 handoff；需要机构登录的文献直接报告由用户自行处理。
- 不索取/不接受机构密码、数据库密码、OTP、恢复码、会话令牌（chat 或终端均不可）。**出版商 API key（本地版唯一允许的凭据）**：仅在用户**主动发送**时视为授权保存——不询问、不引导注册、不要求重生成、不回显；经 `configure_credentials.py set <provider> --stdin` 保存，不进命令行参数/日志/回复/manifest，只报掩码确认与验证状态；没有 key 就全程走 OA，不影响使用。
- ~~已保存登录页例外（SSO/CAS/CARSI 页面代点一次登录按钮）~~：**本地版停用**，不存在代点登录场景。
- 不检查/不导出/不复用 cookie、密码、localStorage、浏览器 profile、会话文件；不打开任何"已认证页面上下文"。
- ~~**浏览器状态原则**（CDP profile 失效的判读）~~：**本地版停用**——无 CDP、无浏览器控制路径。
- **格式原则**：PDF、HTML 全文是不同交付物；用户只要 PDF 时必须拿到真 PDF 链接或 `%PDF` 响应，否则报 `no_authorized_pdf_found`/`pdf_fetch_failed`；不得把 HTML/登录页/验证页当 PDF 保存（原 CAJ 分支随 CNKI 路由停用）。
- 中文文献本地版**只找公开免费合法副本，不走 CNKI 登录态**；不导出 cookie、不收集机构密码。
- 不无限量下载：只处理用户确认的明确文献清单，按 provider 友好节奏，留下完整审计轨迹（何物、何来、SI 是否找到）。
- 中文 SI 触发词示例："连补充材料一起下"、"把补充材料也下了"、"include SI"。

## 六、配套资源

**references/**：
- `institutional-browser-workflow.md` — **本地版停用、不加载**。原文内容：Web Access 推荐流程（WoS 认证、检索、全文路由优先级）、出版商验证与 ScienceDirect 反爬、机构认证 handoff 与安全 auto-confirm 五条件、浏览器上下文 PDF 传输。
- `delivery-verification-and-failures.md` — SI 抓取方法与逐文件验证清单、命名规范、OA 故障处置（本地版适用部分）；其中 WoS+SI 目录纪律、Zotero 注意事项、浏览器相关故障处置与首次机构运行确认项不适用。

**scripts/**：
- `batch_download.mjs` — 编排入口（本地版只用 DOIs/title --open-access/pdf-url 与出版商 API 分支；不用 topic/WoS 与 CNKI 分支）。
- ~~`browser_pdf_downloader.mjs`~~ — **停用**：经 CDP 在已认证 Chrome 页面抓 PDF，本地版不运行。
- ~~`cdp_open_url.mjs`~~ — **停用**：CDP 代理开页。
- ~~`configure_school.py`~~ — **停用**：学校/图书馆入口配置。
- `configure_credentials.py` — 出版商 API 凭据与联系邮箱配置（本地版仅在用户主动给 key 时用 set；0600 存储）。
- `extract_pdf_text.py` — PDF 文本抽取验证（页数 + 前 N 页文本），本地保留。
- `retry_elsevier_api.mjs` — Elsevier API 重试（有 key 时保留）。
- `lib/`：**本地版使用** `credentials.mjs`、`direct-download.mjs`、`manifest.mjs`、`metadata.mjs`、`open-access.mjs`/`open-access-provider.mjs`（PMC/Unpaywall/arXiv 等 OA 解析）、`pdf-utils.mjs`、`provider-registry.mjs`/`provider-utils.mjs`/`publisher-providers.mjs`（出版商 API 路由与状态）、`routing.mjs`、`settings.mjs`、`status-codes.mjs`；**停用** `anti-bot.mjs`、`cdp-utils.mjs`、`cnki.mjs`、`school-config.mjs`、`wos-supporting-information.mjs`。

**src/**：`config.py`、`validators.py` 随 OA/API 路径保留；~~`wizard.py`（学校配置向导）、`health_check.py`、`schools_loader.py`~~ 本地版不运行。

**data/**：`publishers.json`（出版商数据）保留；`schools.yaml` + `school.schema.json`（学校配置）本地版不使用。

**examples/**：`manifest-template.tsv` 保留；`carsi-retry-template.tsv`、`publisher-verification-template.tsv` 随浏览器路由停用。

**tests/**：本地版只跑 OA、凭据、manifest、metadata、routing、status-codes 相关单测；CNKI、WoS SI 等浏览器路由单测不适用。

**docs/**：调研与合并计划文档（非运行时功能）。**assets/**：banner.jpg。无 evals.json。
