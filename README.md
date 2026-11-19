# myAgent

> 本地优先的 Windows 桌面智能体工作台。模型能力由你自备的 API 提供，会话、记忆与文件都留在本机。

myAgent 是一个基于 **Tauri 2 + Rust + React 19** 的桌面应用。它不做模型托管，也不做数据中转：你填入自己的 OpenAI 兼容端点与 API Key，应用只与该端点直连；其余数据（会话、分支、记忆、笔记、附件、产物、用量）写入本地 SQLite 与纯文本文件。

**当前版本**：v2.8.0
**许可证**：Apache-2.0（第三方衍生声明见 [NOTICE](NOTICE)）
**平台**：Windows 10 / 11（64 位）

---

## 目录

- [概述](#概述)
- [功能](#功能)
- [技术架构](#技术架构)
- [隐私与安全](#隐私与安全)
- [系统要求](#系统要求)
- [开发与构建](#开发与构建)
- [配置模型服务商](#配置模型服务商)
- [项目结构](#项目结构)
- [数据存储](#数据存储)
- [快捷键](#快捷键)
- [已知限制](#已知限制)
- [第三方致谢](#第三方致谢)
- [许可证](#许可证)

---

## 概述

| 维度 | 说明 |
|---|---|
| 形态 | 无边框主窗口 + 托盘常驻；双击托盘图标弹出贴任务栏的窄版小窗 |
| 前端 | React 19 + TypeScript 5.8 + Vite 6 + Zustand 5 + React Router 7 |
| 后端 | Rust（edition 2024，rust-version 1.90），180 个 `#[tauri::command]` IPC 入口 |
| 数据库 | SQLite（rusqlite bundled），迁移版本 `SCHEMA_VERSION = 38` |
| 模型接入 | OpenAI 兼容协议，内置 OpenAI / DeepSeek / Kimi / 通义千问 / 智谱 GLM 模板，可自定义端点 |
| 工具 | 内置 38 个（19 只读 / 13 写 / 6 GUI 自动化）+ 外部 MCP Server |
| 渲染 | react-markdown + KaTeX + Mermaid + ECharts + highlight.js |

安装包约 17 MB（NSIS），前端构建产物约 8.7 MB。体积主要来自复用系统 WebView2，而不是捆绑 Chromium。

---

## 功能

### 对话

- SSE 流式输出，推理过程单独折叠展示，不混入正文。
- 多会话管理；会话可归入项目文件夹，项目与会话均可改名与删除。
- 消息级分支树：任一消息可另起分支，分支间独立继续对话，可查看分支首条摘要与消息数。
- 会话内可中途切换模型与服务商，历史消息随之重放。
- Markdown、KaTeX 公式、Mermaid 图表、ECharts 图表四类渲染，均在前端完成。
- 公式容错：除 `$...$`、`$$...$$`、`\(...\)`、`\[...\]` 外，以 `latex`、`tex`、`math` 作为语言标记的代码围栏也会被归一化为块级公式，避免模型用代码块输出公式。
- 选中消息文字弹出复制 / 追问浮条。追问与引用是同一能力：引用片段以卡片形式显示在提问气泡上方，复用 Markdown 与 KaTeX 渲染；超过 5 行自动折叠，可展开。
- 消息中的产物路径自动整理为列表视图（文件名、目录、类型、大小、打开/定位操作），超过 5 条折叠；正文只写文件名时，会结合上下文目录补全识别。
- 上下文超出预算时自动压缩：先截断旧的工具结果，再把最早轮次交给模型摘要，尾部保留约 55% 预算；压缩只作用于本次请求，数据库中的历史消息不变。

### 项目

- 项目把一组会话绑定到本地目录，可设置模型覆盖与项目级共享上下文，会话继承项目设置。
- 工作区根下的 `AGENTS.md`（兼容 `CLAUDE.md`）作为「项目约定」注入每轮请求，上限 32 KB，按路径、修改时间与大小缓存，文件改动后下一轮生效；只在组装请求时注入，不写入数据库历史。

### 检查器与任务清单

- 会话右侧检查器展示工具流水：工具名、参数摘要、耗时与关联任务，点击可跳到来源消息；参数与输出均脱敏。面板宽度可拖拽（240–520 px），收起后由会话头按钮重新展开。
- 目标级任务清单：模型通过 `todo_write` / `todo_read` 维护；若回合结束时清单仍未同步，后端会追加一轮提醒模型更新，达到工具轮次上限则跳过。面板支持手动刷新。

### 产物与导出

- 模型调用 `present` 工具显式声明交付文件，前端渲染交付列表；未声明但正文出现的路径也会被识别。
- 文件写入采用分块协议（`fs_write_begin` / `fs_write_chunk` / `fs_write_finish`），大文件不一次性占用上下文。
- 会话可导出 Markdown；PDF 通过 WebView 调起系统打印对话框，在打印机中选择「另存为 PDF」。导出前的链接做协议白名单与转义校验，非法链接降级为纯文本。
- 表格类产物可导出 DOCX / XLSX（`fs_export_docx`、`fs_export_xlsx`）。

### 媒体生成与识别

- `image_generate` 调用设置中配置的图像服务商（OpenAI 兼容 `/images/generations`），结果保存到 `outputs/images/` 并自动登记为图片交付卡；服务商与模型在「设置 → 能力 → 媒体能力」中选择。
- `slides_export` 把结构化大纲导出为 16:9 的 `.pptx`（每页标题、要点、可选配图）；生成失败时回退导出同名 Markdown 大纲，不静默丢内容。
- `ocr_extract` 用 Windows 系统 OCR 读取图片文字，支持中英混排，结果按视觉顺序返回；依赖系统已安装的 OCR 语言包。

### 工具与审批

内置工具按用途分三类：

| 类别 | 数量 | 代表工具 |
|---|---|---|
| 只读 | 19 | `fs_read` `fs_list` `fs_grep` `file_extract` `zip_list` `doc_search` `doc_read` `doc_list` `web_fetch` `sys_overview` `sys_processes` `sys_disks` `sys_networks` `ocr_extract` `get_now` `present` `todo_read` / `todo_write` `delegate` |
| 写入 / 执行 | 13 | `shell_exec` `fs_write` `fs_edit` `fs_write_begin` `fs_write_chunk` `fs_write_finish` `fs_delete` `fs_export_docx` `fs_export_xlsx` `image_generate` `slides_export` `fs_restore` `zip_extract` |
| GUI 自动化 | 6 | `ui_list_windows` `ui_find_elements` `ui_invoke` `ui_set_value` `ui_type_text` `ui_focus_window` |

- `todo_write` 与 `present` 只写会话内的任务清单和产物索引，不改动磁盘文件，因此计入只读类。
- 审批三档：**逐次审批**（默认）、**工作区**（只读与工作区内写入自动放行，shell / 删除 / 越界写入仍询问）、**完全信任**（全部直接执行，并解锁 GUI 自动化）。审批结果与调用参数写入本地审计日志，日志对 `sk-`、`ghp_`、`Bearer` 等密钥形态做脱敏。
- 工作区档支持附加自动放行名单，条目可写 `server::tool` 或只写工具名。
- shell 默认只放行只读命令白名单；解释器与写操作被拒绝。格式化、删根、改注册表等高危命令在逐次审批与工作区档下强制人工确认，模型写入的 `confirm` 参数不能替代确认；完全信任档下不再询问，只记审计。
- 文件沙箱：相对路径限制在当前会话工作区内，拒绝 `..` 越界；绝对路径可显式操作其他位置。边界比较按路径组件进行，`D:\proj` 不会误判包含 `D:\proj2`。
- 工具规格按需下发，只影响每轮注入量，不改变可用能力：只读工具全量注入；写工具中 8 个高频项（`shell_exec`、`fs_write`、`fs_edit`、分块写入三件套与 `image_generate`、`slides_export`）常驻，5 个低频项（删除、DOCX / XLSX 导出、备份恢复、解压）仅在会话历史调用过时注入；GUI 工具只在「完全信任」档注入。
- 外部 MCP 工具按 server 粒度裁剪：该 server 在会话历史上被调用过则全量下发；未调用过时，工具数不超过 3 个的 server 全量下发，超过 3 个的只下发名称含 `read` / `list` / `search` / `get` / `find` / `think` / `fetch` / `extract` 的核心子集，无命中则取前 3 个。

### MCP

- 基于 rmcp 官方 SDK，支持 stdio（子进程）与 Streamable HTTP 两种传输。
- 预置四个服务，均为运行时经 `npx` 启动（应用启动时拉起并预热工具列表），不随安装包分发：
  1. 项目文件 — `@modelcontextprotocol/server-filesystem`
  2. 长期记忆 — `@modelcontextprotocol/server-memory`
  3. 顺序思考 — `@modelcontextprotocol/server-sequential-thinking`
  4. 联网搜索 — `agent-search-mcp`
- 设置页可增删服务、编辑 JSON 配置、测试连通性、查看工具清单与诊断信息；支持自定义请求头，请求头中的凭据在落盘前经 DPAPI / AES-GCM 加密。
- 外部工具默认按中等风险走确认；工作区档可逐个放行。

### 记忆

- 每个会话对应一份 `memory.md`（YAML frontmatter + 结构化 Markdown），可直接用外部编辑器修改，应用通过文件监听实时同步。
- 记忆版本快照与行级 diff，保留最近 20 版，可回滚到任一历史版本。
- 跨会话记忆注入与草稿交接；注入按策略取用（摘要 / 混合 / 全文），受 token 预算约束。
- 长期记忆（MCP memory 服务）与本机记忆分开管理，互不覆盖。

### 知识库与文档检索

- 内置 Markdown 笔记，支持标签、预览、全文检索，`Ctrl+S` 保存。
- 11 条预置笔记在首次启动时落盘，随后即为普通笔记，可编辑、可删除，删除后不复活；「导入预置」按章节与表格首行逐条合并，已有内容不被覆盖。
- 会话内文档库：解析后的文档切块写入 `doc_chunks`，检索走 FTS5 `trigram` 索引。两字中文查询会展开为 3-gram 的 OR 表达式，再按覆盖率打分；与关键词路结果做 RRF 融合。

### 附件

- 本地解析，不经云端：PDF、DOCX、XLSX（兼容 xls / ods）、PPTX、EPUB、HTML。
- 附件中心统一管理大小配额、拒绝记录与清理；解析结果以块数与摘要形式注入上下文，不重复注入全文。
- 从附件中心插入会话时可预览：左侧列表、右侧展示文件名、来源会话、解析状态与摘要，窄窗口自动收起预览栏。
- 拖拽入会话或从附件中心插入，两者走同一解析链路。

### 全局搜索

- `Ctrl+K` 或标题栏放大镜唤起居中搜索面板，`Esc` 或点击遮罩关闭。
- 检索范围：会话标题、消息全文（FTS5）、`memory.md`、知识库笔记、附件解析内容（文件名 / 摘要 / 提取正文）。结果按来源分组展示并支持跳转定位。

### 智能体

- 内置多套模板，按对话、文档、开发、创作、运维、学习、工作、通用八类组织，可绑定会话，或仅作为提示词应用一次。
- 内置模板提示词可编辑：同 id 的自定义副本覆盖内置，删除副本即恢复。
- 支持基于基座模板创建变体（追加后缀）与完全自定义模板。
- 存储可选**耦合**（单文件 `agents.json`）或**解耦**（目录多文件 `config.json` / `prompt.md` / `tools.json`）。解耦模式下可关联外部 `prompt.md`、`tools.json`，便于用外部编辑器维护。
- 导入 / 导出走原生另存为对话框。
- 委派：`delegate` 工具把独立子任务交给指定智能体，在隔离上下文中单轮执行并返回结果；子智能体没有工具权限，也不能再委派。
- 设置页「角色设置」可编辑全局 base prompt，留空恢复默认。

### 运行时预设

预设是一份可合并的配置清单，用来决定工具集、上下文窗口、记忆策略、审批建议与检索参数。它不改变界面布局，也不替代全局权限档位。

| 档位 | 定位 | 工具 | 上下文 | 检索 |
|---|---|---|---|---|
| 经济 | 快问快答、弱模型 | 7 个：`get_now` `fs_read` `fs_list` `doc_search` `doc_read` `present` `shell_exec` | 20 条 / 24k 字符 | 3-gram，Top-3 |
| 标准 | 日常默认 | 15 个：常用读写、文档解析与媒体生成，无子代理、删除、GUI | 40 条 / 48k 字符 | 3-gram + 向量，Top-5 |
| 深度 | 长文档、多步调研 | 全量 | 30 条 / 48k 字符 | 3-gram + 向量 + 重排，Top-5 |
| 代码 | 改码与重构 | 全量 | 40 条 / 64k 字符 | 3-gram，Top-5 |

合并优先级：会话钉住 > Agent 模板默认 > 全局默认。会话创建时钉住预设清单版本，老会话不随内置预设升级漂移，切换需显式确认。可在数据目录放置 `presets.json` 覆盖同名内置档或追加新档。同一设置页还可设定单次生成的总时长上限（1–1440 分钟），到点只结束本次生成，已输出内容保留。

### 用量与预算

- Token 级用量记录，按日、按会话、按服务商聚合；提供时段热力图、成本估算、按当前计价表复算历史成本，以及 Key 健康度视图。
- 请求预算三层校准：
  1. 模型名匹配官方 BPE 词表（deepseek_v3/v4、qwen2、kimi_k2/k3、glm4/glm5、minimax_m2、o200k_base、cl100k_base 等），逐 token 计数；
  2. 取本会话最近 10 轮「真实 `input_tokens` / 估算值」的中位数作校准系数；
  3. 与上一轮消息比较最长公共前缀，按服务商的缓存命中率预估分段缓存节省量。
- 每条请求在 `session_events` 中留档，`payload` 含 `est_tokens` 与 `breakdown`（`messages`、`tools`、`l1_raw`、`calibration`、`l2_calibrated`、`cache_saved`），可与 `usage_record` 的真实值逐轮对账。

### 设置

- 分组：模型（服务商 / 智能体管理 / 记忆管理）、能力（联网搜索 / 媒体能力）、界面（外观与首页 / 导航菜单 / 运行时预设）、扩展（MCP 工具）、数据（用量统计 / 存储位置 / 关于）。
- 外观可调整主题、字号与首页提示语；导航菜单可拖拽排序，并可隐藏记忆库、知识库、附件入口。
- 存储位置支持整体迁移数据目录，迁移时同步改写数据库中附件记录的绝对路径。

---

## 技术架构

| 层 | 技术 |
|---|---|
| 桌面框架 | Tauri 2.12（Rust + 系统 WebView2） |
| 前端 | React 19 / TypeScript 5.8 / Vite 6 / Zustand 5 / React Router 7 |
| 后端 | Rust 2024，tokio 1，reqwest 0.13（rustls） |
| 数据库 | SQLite（rusqlite 0.32 bundled），`PRAGMA user_version` 迁移 |
| 密钥 | Windows DPAPI + AES-GCM 双路径（`provider/keychain.rs`） |
| 文档解析 | pdf-extract / docx-rs / calamine / zip / rust_xlsxwriter |
| MCP | rmcp 3（stdio + Streamable HTTP） |
| Token 估算 | tiktoken 4（纯 Rust，内置多厂商 BPE） |
| GUI 自动化 | windows 0.62（UI Automation / SendInput / 窗口枚举），仅 Windows 目标 |
| 媒体 | 图片生成（OpenAI 兼容 HTTP）、演示文稿（ppt-rs）、OCR（Windows.Media.Ocr） |

架构要点：

- 附件解析、检索排序、流式转发、密钥链、工具调度都在 Rust 侧完成，前端只负责渲染与交互。
- CSP 为白名单制，原始配置：

```text
default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';
img-src 'self' data: file: blob:; font-src 'self' data:;
connect-src 'self' ipc: http://ipc.localhost;
object-src 'none'; base-uri 'none'; frame-ancestors 'none'
```

- 窗口无系统装饰，标题栏由前端绘制；关闭主窗口隐藏到托盘，而非退出进程。
- 工具调用日志按会话滚动保留最近 500 条；数据库升级前用 `VACUUM INTO` 生成快照，保留最近 2 份。

单轮请求的处理顺序：

1. 前端提交消息、附件元数据、会话预设与工具定义到 `chat_stream`。
2. Rust 侧读取会话历史与 `memory.md`，按预设裁剪上下文，完成 token 估算并写入 `session_events`。
3. 请求发往用户配置的服务商端点，SSE 增量回传，前端边接收边渲染。
4. 模型返回工具调用时，由后端 `dispatcher` 做风险评估与审批判定，执行后把结果作为 tool 消息续接，循环至模型不再调用工具。
5. 回合结束后落库 `messages`、`usage_record`、`tool_call_log`，并同步任务清单与产物登记。

---

## 隐私与安全

| 维度 | 说明 |
|---|---|
| 数据位置 | 全部本地：SQLite + Markdown / JSON，数据目录可由用户指定并迁移 |
| 出网范围 | 仅连接用户配置的模型端点、用户启用的联网搜索后端与 MCP 服务；无遥测 |
| API Key | DPAPI（Windows）加密，辅以 AES-GCM 密钥链，不落明文；`secrets/` 目录权限收紧为当前系统用户可读 |
| 文件操作 | 默认工作区沙箱；绝对路径显式放行；路径组件级边界比较 |
| 工具执行 | 三档审批；高危 shell 在非完全信任档强制确认；全部调用写审计日志并脱敏密钥 |
| 数据库升级 | 迁移前自动快照（保留最近 2 份），异常可手工回滚 |
| 导出 | 链接做协议白名单与转义校验 |
| CSP | 严格白名单，禁止远程脚本与对象嵌入 |
| 开源 | Apache-2.0，代码可审计 |

「完全信任」档会解除绝大多数执行前确认，并解锁 GUI 自动化。该档只应在你同时信任所用模型与工具输出内容时开启。

---

## 系统要求

- Windows 10 / 11（64 位）
- WebView2 运行时（Windows 11 自带；Windows 10 一般已预装，缺失时安装程序会引导）
- 至少一个 OpenAI 兼容服务商的 API Key
- 联网搜索若使用 Tavily / 博查 / 智谱等后端，需要各自的 Key；SearXNG 需要自建实例地址

---

## 开发与构建

环境：

| 依赖 | 版本 |
|---|---|
| Node.js | 18+ |
| pnpm | 11（`corepack enable`） |
| Rust | 1.90+ |
| 系统 | WebView2 + MSVC 生成工具 |

脚本（`package.json`）：

| 命令 | 作用 |
|---|---|
| `pnpm dev` | 仅启动 Vite（端口 1420） |
| `pnpm build` | `tsc -b` + `vite build` |
| `pnpm preview` | 预览前端构建产物 |
| `pnpm test` | `vitest run` |
| `pnpm audit` | `pnpm audit --audit-level=high` |
| `pnpm tauri dev` | 开发模式（Vite 热更新 + Tauri 窗口） |
| `pnpm tauri build` | 前端构建、Rust 编译、NSIS 打包 |

打包产物位于 `src-tauri/target/release/bundle/`（NSIS 安装包与可执行文件）。

测试：

```bash
cd src-tauri
cargo test                 # Rust 单元与集成测试，500+ 项
cargo check                # 静态检查
cd ..
pnpm test                  # 前端 35 个测试文件
npx tsc -b                 # 类型检查
```

---

## 配置模型服务商

1. 打开 **设置 → 模型服务商**，选择内置模板（OpenAI / DeepSeek / Kimi / 通义千问 / 智谱 GLM）或新增 OpenAI 兼容服务商。
2. 填入 API Key，Key 立即经 DPAPI 加密写入本地密钥链。
3. 拉取模型列表并勾选启用。
4. 同一服务商可配置多个 Key，按轮换与故障转移策略使用。

联网搜索在 **设置 → 联网搜索** 中配置，支持博查、智谱 BigModel、DeepSeek 搜索、SearXNG、Tavily 五种后端。

---

## 项目结构

```text
myAgent/
├── index.html
├── package.json
├── vite.config.ts
├── tutorials/                    # 应用内教程文本
├── src/                          # 前端
│   ├── components/               # 标题栏、侧栏、消息气泡、检查器、搜索面板
│   ├── lib/                      # IPC 封装、导出、路径与格式工具
│   ├── pages/                    # chat / memory / notes / attachments / settings
│   ├── stores/                   # Zustand 状态
│   └── styles/                   # 按域拆分的 CSS
└── src-tauri/
    ├── tauri.conf.json
    └── src/
        ├── commands/             # IPC 命令（provider / mcp / memory / attachment / ...）
        ├── db/                   # SQLite schema、迁移、查询
        ├── provider/             # 服务商模板、Key 轮换、DPAPI、模型卡、计价
        ├── mcp/                  # MCP 客户端、内置工具、审批与安全策略
        ├── agents/               # 内置智能体模板与存储
        ├── memory/               # memory.md 读写、版本、监听
        ├── websearch/            # 搜索引擎、抓取、正文提取
        ├── retrieval.rs          # 3-gram 展开、覆盖率打分、RRF
        ├── presets/              # 内置四档预设与合并
        ├── gui/                  # UI Automation 工具
        ├── doc/                  # 文档库切块与索引
        └── settings.rs
```

---

## 数据存储

默认数据目录在应用数据文件夹内，可在设置中迁移：

```text
data/
├── metaagent.db                # 会话、消息、分支、用量、索引、产物
├── settings.json
├── provider_templates.json
├── mcp-approval.json           # 审批档与自动放行名单
├── agents.json                 # 耦合模式下的自定义智能体
├── presets.json                # 可选：覆盖内置预设
├── secrets/                    # DPAPI / AES-GCM 加密的凭据
├── conversations/<id>/
│   ├── memory.md
│   └── memory.versions/
├── notes/                      # 知识库 Markdown
└── attachments/                # 会话附件
```

主要数据表：

| 表 | 用途 |
|---|---|
| `sessions` / `messages` / `branches` | 会话、消息、分支树 |
| `projects` / `tasks` / `todos` | 项目、任务、待办 |
| `usage_record` | 每请求真实 token 与费用 |
| `session_events` | 请求/工具/交付等事件留档 |
| `tool_call_log` | 工具调用记录（按会话滚动 500 条） |
| `deliverables` | 交付产物登记 |
| `memory_versions` | 记忆版本快照 |
| `notes` | 知识库笔记 |
| `doc_chunks` / `doc_chunks_fts` | 文档块与 FTS5 trigram 索引 |

迁移全自动且幂等：磁盘 `user_version` 落后时按版本顺序补迁移，存量数据回填；索引表缺失或分词器过旧会重建。深度研究、学习中心、翻译、绘画页面已从界面移除，其中研究相关的 `research_sessions` 表在 V23 迁移中删除；`data/research/**`、`data/images/**` 等历史文件仍保留在数据目录，可自行删除。

记忆与笔记均为纯文本，可用任意编辑器修改。备份整个数据目录即可迁移全部数据。

---

## 快捷键

| 快捷键 | 作用 |
|---|---|
| `Ctrl + K` | 打开全局搜索面板 |
| `Ctrl + S`（笔记页） | 保存笔记 |
| `Esc` | 关闭搜索面板 / 取消重命名 |

鼠标操作：双击会话标题重命名；双击托盘图标打开小窗；点击托盘图标恢复主窗口。

---

## 已知限制

- 仅提供 Windows 构建；GUI 自动化依赖 Windows UI Automation。
- 附件解析不做 OCR：扫描件 PDF（纯图片）只能提取内嵌文本层；需要读取图片文字时，把图片单独拖入会话后用 `ocr_extract` 识别，识别语言取决于系统已安装的 OCR 语言包。
- 会话导出 PDF 依赖系统 WebView2 打印，分页与字体由系统渲染决定，不同机器可能略有差异。
- 用量成本估算基于内置计价表，自定义端点需手动维护单价，否则按 0 成本统计。
- 工具调用日志按会话保留 500 条，超出后最早的记录被覆盖。

---

## 第三方致谢

- **[Cherry Studio](https://github.com/CherryHQ/cherry-studio)**（AGPL-3.0）—— 仅参考服务商管理与桌面助手架构思路，未复制代码或资源
- **[MCP 官方参考服务器](https://github.com/modelcontextprotocol/servers)**（MIT）—— 内置项目文件 / 长期记忆 / 顺序思考服务，运行时经 npx 启动
- **[agent-search-mcp](https://github.com/lennney/agent-search-mcp)**（Apache-2.0）—— 内置联网搜索服务，运行时经 npx 启动
- **[ppt-rs](https://crates.io/crates/ppt-rs)**（Apache-2.0）—— 生成 `.pptx` 演示文稿
- 其余 Rust 与前端依赖的许可证见 `NOTICE` 与各 crate 元数据

---

## 许可证

Copyright 2026 梧桐吾桐及其团队

本项目基于 [Apache License 2.0](LICENSE) 开源，衍生与引用关系见 [NOTICE](NOTICE)。
