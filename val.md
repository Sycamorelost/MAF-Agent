# MAF Agent 全方位评估报告（安全 · 功能 · 性能）

> 版本：1.10.3 · 评估日期：2026-10-26 · 构建：debug（`src-tauri/target/debug/app.exe`）+ Vite dev server
> 实测环境：Windows / 运行中实例 CDP 9223 / 审批策略 `full`（测试受限模式时临时切换后已恢复）
> 评估方法：源码逐层审读 + 3 组自动化探针（CDP 直调 185 个 Tauri 命令中的 117 个）+ 依赖审计（`cargo audit`）+ 构建/进程/数据库统计 + 10 条路由 UI 冒烟
> 覆盖声明：**本报告完全覆盖 `val.md` 原有内容**；上一轮「命令执行命中率诊断 + Step 1/2/3 落地」结论合并到 §7，开源发布政策保留在 §10。
> 权限边界声明：按用户要求，**MCP 权限边界问题不计入缺陷**——完全信任 shell、GUI 自动化、能力令牌、任意 MCP 工具调用均为当前预期能力，本报告只评估其实现质量、可观测性与安全基线，不评判“权限过大”。

---

## §0 结论摘要

### 0.1 总评

| 维度 | 评级 | 一句话结论 |
|---|---|---|
| 安全 | **B+** | 桌面威胁模型下的基线扎实（CSP、capability 最小集、DPAPI、路径沙箱、SSRF/zip-slip 防护齐全）；扣分项集中在依赖漏洞、分发签名、审计完整性与数据库明文。 |
| 功能 | **B+** | 159 条实测用例首轮通过 149 条（93.7%）；复核后**真实缺陷 4 项**（1 项功能不可用、1 项校验缺失、2 项一致性/接口问题），核心链路（会话、记忆、附件、Provider、研究、学习、文件、MCP、shell、UI）全部可用。 |
| 性能 | **B** | IPC 极快（常规 2–6 ms，消息分页 2–3 ms），但 `list_images` 403 ms、首屏产物 8.42 MB、WebView2 开发态 23 进程 1487 MB、SQLite 单连接无 WAL 是明确短板。 |
| **综合** | **B+（约 8.1/10）** | 工程完成度高、可继续迭代；优先修 4 个真实缺陷与 3 个结构性风险（SQLite 并发、依赖漏洞、审计完整性）。 |

> 评分为工程判断，用于排序改进项，不等同于任何标准认证。

### 0.2 关键数字

| 指标 | 实测值 |
|---|---|
| 已注册 Tauri 命令 | **185** |
| 直接调用覆盖 | **117 / 185（约 63%）**，其余经代码审读 + 间接路径确认 |
| 本轮实测用例 | **159**（eval3 70 + eval4 67 + shell 12 + UI 10） |
| 首轮判定 | PASS 149 / FAIL 10；复核后真实缺陷 4 项（涉及 5 条用例），5 条为用例口径/入参问题 |
| 自动化回归（最近一次全量） | Rust `cargo test --lib` **327 通过**；前端 `vitest run` **104 通过**；`tsc -b`、`vite build`、`clippy -D warnings` 均通过 |
| 前端产物 | **8.42 MB / 134 文件**；最大 chunk：elk 1.44 MB、mermaid 658 KB、cynefin 638 KB、markdown 597 KB、index 593 KB |
| 数据库 | **1.14 MB / 28 表**；sessions 36、messages 98、usage_record 130、study_logs 26、quiz_attempts 15、memory_versions 56、knowledge_nodes 38 |
| 审计日志 | `data/logs/mcp-audit.jsonl` **77,270 B / 541 行** |
| 进程内存 | `app.exe` 63 MB（debug）；`msedgewebview2` 23 进程合计 **1487 MB**（dev + CDP，非发布态） |
| 依赖审计 | `cargo audit` 报告 **7 条告警 / 2 个包**：zip 2.4.2 与 8.6.0（RUSTSEC-2026-0367/0368/0369）、webpki-root-certs 1.0.9（RUSTSEC-2026-0383） |

### 0.3 Top 风险（按修复优先级）

| 优先级 | 问题 | 证据 | 影响 |
|---|---|---|---|
| P1 | 翻译历史**无法删除** | 列表 JSON 无 `file_name` 字段，前端 `if (!rec.file_name) return` 直接返回 | 功能不可用、历史无法清理 |
| P2 | SQLite 单连接 + 无 WAL/busy_timeout | `db/mod.rs` 单 `Mutex<Connection>`；schema 仅 `foreign_keys=ON` | 长迁移/FTS 回填期间所有 IPC 阻塞；大库启动变慢 |
| P2 | 空/纯空白 API Key 可保存 | `save_api_key` 无 trim/空值校验；实测保存 `"   "` 后 `has_api_key=true` | 界面显示“已配置”，实际请求必失败，误导排障 |
| P2 | 依赖漏洞（zip / webpki-root-certs） | `cargo audit` 7 条 | 本项目解压未用危险 API，实际利用面低，但需升级并持续审计 |
| P2 | 审计日志无轮转/无完整性、写入失败被忽略 | `mcp/mod.rs:109-130` 使用 `let _ =` 追加写 | 审计可被同用户删除/篡改；磁盘满时静默丢失 |
| P2 | 无代码签名/更新签名 | `tauri.conf.json` bundle 无 updater/签名配置 | 分发供应链风险 |
| P3 | `notes_get` 比 `notes_save` 多一个尾随换行 | 实测 body `"评估闭环测试"` → `"评估闭环测试\n"` | 严格字符串比较失败；一般 UI 无感 |
| P3 | `snapshot_memory` 返回路径，版本 API 收数字 id | 实测返回 `.md` 路径；回滚需 `list_memory_versions[].id` | 调用方易误用；接口语义不一致 |

---

## §1 评估范围与方法

### 1.1 系统边界

- 技术栈：Tauri 2 + React 19 + Rust（edition 2024）+ SQLite（rusqlite bundled）+ rmcp 3。
- 进程边界：前端 WebView（CSP 限制）↔ IPC（185 命令）↔ Rust 内核（fs/shell/MCP/DB/Provider）。
- 数据边界：`data/metaagent.db`、`data/secrets/`（DPAPI 密文）、`data/workspace/`（文件沙箱）、`data/conversations/`、`data/attachments/`、`data/logs/`。
- 页面边界：10 条路由（`/`、`/chat/:id`、`/research`、`/plan`、`/draw`、`/translate`、`/memory`、`/notes`、`/attachments`、`/settings`）。

### 1.2 评估对象

- **功能**：185 个已注册命令（`commands/mod.rs` 的 `generate_handler!`）+ 10 个页面 + 内置 MCP 工具 + shell 双通道。
- **安全**：CSP、capability、密钥存储、路径沙箱、zip 解压、MCP URL/Header 校验、GUI 白名单、能力令牌、审计。
- **性能**：IPC 延迟、批量写入、大对象列表、构建体积、进程内存、数据库结构与迁移开销。

### 1.3 方法

1. **运行时实测**：通过 CDP（9223）在页面上下文调用 `window.__TAURI_INTERNALS__.invoke(cmd, args)`，绕开 UI 但走真实 IPC、真实数据库、真实文件系统。
2. **用例分组**：
   - `eval3`：只读矩阵、shell、fs 沙箱、MCP 校验、能力令牌、会话/消息/分支、记忆、附件、格式、列表、harness、性能（70 条）。
   - `eval4`：会话设置、消息/分支 CRUD、memory.md 全生命周期、附件、Provider/Key/用量、翻译、研究、学习、zip、MCP/GUI、性能（67 条）。
   - `shell`：full/受限双通道 12 条（含管道、重定向、退出码、cwd、GBK）。
   - `ui`：10 条路由 + 关于页文案核对。
3. **代码审计**：逐文件核对安全控制点与实现细节；统计生产代码 panic 面。
4. **依赖审计**：`cargo audit --json`；前端使用 `pnpm-lock.yaml`（120 KB），当前环境未安装 pnpm，故未跑 `pnpm audit`。
5. **数据统计**：`node:sqlite` 只读读取库表行数；PowerShell 统计构建体积、进程内存、审计日志。

### 1.4 明确不计入缺陷的项

- **MCP 权限边界**：完全信任模式可执行任意命令、GUI 可操作任意白名单进程、能力令牌可预授权、MCP 可调用任意外部工具——均为当前设计目标。
- **模型质量/网络质量**：流式回答内容、生成图片质量、搜索时效性依赖外部服务，不在本次评分内。
- **安装器/签名/商店分发**：仅作为发布建议列出（SEC-6），不计入运行时缺陷。
- **UI 视觉设计**：仅做可用性与异常冒烟，不做像素级评审。

### 1.5 环境与数据基线

- 审批策略：`full`（开始与结束均为 `full`；受限模式测试期间临时切到 `manual` 并已恢复）。
- 数据库：36 个会话、98 条消息、130 条用量记录；`usage_record` 成本已由 `usage_recalc` 重算（delta_nano=601，约 6.01×10⁻⁷ 元，无业务影响）。
- 测试数据：全部临时会话、消息、笔记、附件、翻译记录、Agent 模板、工作区文件、临时 Key 已删除；`data/secrets` 仅保留原有 5 个 provider 密钥文件，均为 `dpapi:` 前缀。
- 侧效说明：`mcp_clear_session_allowed` 清空的是本次审批会话的临时放行表；`glossary_save([])` 前后术语表均为空。

---

## §2 功能实测矩阵

### 2.1 域覆盖总览

| 功能域 | 覆盖内容 | 结果 | 关键发现 |
|---|---|---|---|
| 会话/消息/分支 | 创建、改名、模型、系统提示、上下文窗口、联网开关、记忆策略、Agent 绑定、消息增删改、分页、分支 fork/切换/改名/删除、清空 | 全部通过 | 120 条写入 577 ms；分页 100+20 条 3 ms/2 ms |
| memory.md | 读、写、追加、快照、版本列表、版本读取、回滚、预算、是否需要总结、watch/unwatch | 通过 | `snapshot_memory` 返回路径而非版本 id（P3） |
| 附件 | 保存、读 data URL、二次保存、目录逃逸、删除 | 全部通过 | 20 MB 上限、目录白名单生效 |
| Provider/Key/用量 | 已存 provider、Key 掩码、槽位、轮换配置、模板、模型配置、思考能力、用量条目/健康/小时/重算 | 全部通过 | 空 Key 可保存（P2） |
| 翻译 | 术语表读写、历史保存/列表/删除 | 术语表通过；历史删除失败 | `file_name` 未序列化（P1） |
| 深度研究 | 搜索配置、会话读取、参考资料、模板、蓝图、预设、技能 | 全部通过 | 导出/启动需模型或路径令牌，未实测 |
| 学习规划 | 计划读取、节点、今日、统计、复习队列、记忆摘要 | 全部通过 | 导出需路径令牌，未完成 |
| 笔记/知识库 | 新建、读取、列表、删除、FTS 重建 | 通过 | `notes_get` 多尾随换行（P3） |
| Agent/教程/搜索 | 模板列表/保存/删除、会话绑定、应用提示词、教程卡、全局搜索 | 全部通过 | 24+ 内置模板可用 |
| 工作区文件 | 列表、读写、备份、回滚、docx/xlsx 导出与提取、zip 列表/解包、越界防护 | 全部通过 | zip 18 条目解出 14 个文件（目录/链接跳过） |
| MCP/能力/GUI | 配置读取、URL/Header/命令校验、令牌签发/列表/撤销/清空、GUI 策略/窗口/元素、审计、工具收集 | 全部通过 | 49 个工具定义 24 KB；GUI 默认拒绝未白名单进程 |
| shell | full 整行/管道/重定向/退出码/cwd/GBK；受限白名单/越界/解释器 | 全部通过 | 受限模式未回退，full 可恢复 |
| UI | 10 条路由、关于页文案 | 全部通过 | settings 1038 DOM 节点；旧文案已删除 |

### 2.2 会话 / 消息 / 分支

| 用例 | 调用与输入 | 预期 | 实测 | 结论 |
|---|---|---|---|---|
| E1 会话改名 | `rename_session(id, "eval4-改名")` | 标题更新 | `eval4-改名` | PASS |
| E2 切换模型 | `set_session_model(id, "deepseek-flash")` | 模型写入 | `deepseek-flash` | PASS |
| E3 会话设置 | `update_session_settings(systemPrompt="你是评测助手", max=12)` | 两字段生效 | `sp=你是评测助手, max=12` | PASS |
| E4 联网开关 | `set_session_web_search(true)` + `get_session` | 开关打开 | 字段为布尔 `true`（用例按 0/1 判定属口径错误） | PASS（行为正确） |
| E5 记忆策略 | `set_session_memory_strategy("summary")` | 策略更新 | `summary` | PASS |
| E6 绑定 Agent | `set_session_agent(id, "product-assistant")` | 绑定并渲染提示词 | `agent=product-assistant`，提示词已写入 | PASS |
| E7 应用提示词 | `apply_agent_prompt(id, "product-assistant")` | 系统提示词写入 | 451 字符 | PASS |
| E8 Agent 模板 CRUD | `save_agent_template` → list → `delete_agent_template` | 保存可见、删除消失 | 三步均成功 | PASS |
| E9 追加 4 条消息 | `append_message` ×4 | id 连续 | `595,596,597,598` | PASS |
| E10 修改消息 | `update_message(596, "...-updated")` | 内容更新 | 列表命中新内容 | PASS |
| E11 分支 CRUD | `fork_branch` → list → rename → set_active → delete | 2 分支→1 分支 | fork/list=2/rename/set/del 全成功 | PASS |
| E12 删除单条 | `delete_message(598)` | 消息消失 | 剩 3 条 | PASS |
| E13 从某条起删除 | `delete_messages_from(597)` | 删除其后全部 | 删除 1 条，剩 2 条 | PASS |
| E14 清空消息 | `clear_messages` | 0 条 | 0 条 | PASS |
| F1 创建 + 120 条写入 | `create_session` + `append_message` ×120 | 全成功 | 577 ms（约 4.8 ms/条） | PASS |
| F2/F3 分页 | `list_messages_page(limit=100)` / `beforeId` | 100 + 20，`has_more` 正确 | 3 ms / 2 ms | PASS |
| F4 分支列表 | `list_branches` | ≥1 | 1 | PASS |
| H1/H4 todos | `session_get_todos` 已有/空会话 | 3 条已完成 / 空数组 | 3 done=3 / `[]` | PASS |
| P4 全量 120 条 | `list_messages` | 完整返回 | 5 ms | PASS |

### 2.3 memory.md

| 用例 | 调用与输入 | 预期 | 实测 | 结论 |
|---|---|---|---|---|
| M1 写入/读取 | `write_memory("# 记忆\n初始内容")` | 正文可读 | 命中 | PASS |
| M2 追加 | `append_memory("追加一条", section=null)` | 追加到资料节 | 正文含“追加一条” | PASS |
| M3 快照/版本 | `snapshot_memory` → `list_memory_versions` | 生成版本 + 可列出 | 返回 `.md` 路径；列表 4 版本，`id=78` | PASS（接口不一致，见 FUNC-3） |
| M3b 读版本 | `read_memory_version(versionId=78)` | 返回快照正文 | 命中 | PASS |
| M4 回滚 | `rollback_memory(versionId=78)` | 回到快照 | 正文恢复 `# T\nv1` | PASS（复核用例） |
| M5 预算/总结判定 | `needs_summarize` + `memory_budget_status` | 布尔 + 预算 | `need=false`，29/2000 → 15/2000 | PASS |
| M6 监听 | `watch_memory` / `unwatch_memory` | 注册/注销 | 均成功 | PASS |
| F5/F6 记忆读取与预算 | `read_memory` + `memory_budget_status` | 返回正文/预算 | 57 字符 / 29 tokens | PASS |

### 2.4 附件

| 用例 | 调用与输入 | 预期 | 实测 | 结论 |
|---|---|---|---|---|
| A2 保存 | `save_attachment(session, "eval4.txt", base64("eval4 附件正文 hello"))` | 落盘 + 提取文本 | 24 B，`extracted_text` 命中 | PASS |
| A3 读 data URL | `read_attachment_data(path)` | `data:` 前缀 | `data:application/octet-stream;base64,...` | PASS |
| A4 二次保存 | 保存 `eval4-b.txt` | 独立文件 | 成功 | PASS |
| A5 目录逃逸 | `read_attachment_data("C:\\Windows\\win.ini")` | 拒绝 | “路径不在 attachments 目录内” | PASS |
| A6 删除 | `append_message(attachments=[meta])` → `delete_attachment(msg, path)` | 条目移除 + 文件清理 | 成功，列表剩 1 条 | PASS |

### 2.5 Provider / Key / 轮换 / 用量 / 模型

| 用例 | 调用与输入 | 预期 | 实测 | 结论 |
|---|---|---|---|---|
| P1 已存 Provider | `list_stored_provider_ids` | ≥1 | `["deepseek","deepseek-search","zhipu"]`，2 ms | PASS |
| P2 Key 是否存在 | `has_api_key("deepseek")` | 布尔 | `true`，3 ms | PASS |
| P3 槽位掩码 | `list_provider_keys("deepseek")` | 不含明文 | `sk-4****bf0a`，slot 0/1/2 | PASS |
| P4 轮换配置 | `get_rotation_configs` | 对象 | zhipu/deepseek 配置返回 | PASS |
| P5 模板 | `list_provider_templates` | 数组 | 2 个（DeepSeek/智谱；用例阈值 ≥5 属口径错误） | PASS（行为正确） |
| P6 模型配置 | `get_provider_model_config` | 对象 | `{}`（未自定义） | PASS |
| P7/P8 思考能力 | `get_reasoning_capability` / `get_reasoning_override` | 能力对象/覆盖值 | `{enabled:true, can_disable:true, levels:true}` / `null` | PASS |
| P9 用量条目 | `usage_entries(limit=20, days=30)` | 数组 | 130 条中最近 20 条 | PASS |
| P10 Key 健康 | `usage_key_health("deepseek",30)` | 分组统计 | 槽位 0：129 请求 / 998,477 tokens / 0 失败 | PASS |
| P11 小时分布 | `usage_hourly(7)` | 数组 | 按 dow/hour 聚合 | PASS |
| P12 成本重算 | `usage_recalc(30)` | 报告 | `delta_nano=601`，3 ms | PASS |
| P13 空 Key | `save_api_key("eval4","   ")` | 应拒绝 | **接受**，`has_api_key=true` | **FAIL（FUNC-2）** |

### 2.6 翻译

| 用例 | 调用与输入 | 预期 | 实测 | 结论 |
|---|---|---|---|---|
| T1 术语表 | `glossary_save([{src,dst,note}])` → list → 清空 | 写入可见、清空生效 | 1 条 → 0 条 | PASS |
| T2 历史 CRUD | `save_translation(rec)` → `list_translations` → `delete_translation(file)` | 列表含该记录、可删除 | 保存返回 `20261026-005015527.json`；列表 13 条但**无 `file_name` 字段**，删除按钮拿不到文件名 | **FAIL（FUNC-1）** |
| 字段核对 | 直接读取列表首条 JSON | 含 `file_name` | keys=`created,provider_id,model,source_lang,target_lang,source_text,result_text` | **FAIL（FUNC-1 证据）** |

### 2.7 深度研究

| 用例 | 调用与输入 | 预期 | 实测 | 结论 |
|---|---|---|---|---|
| R1 搜索配置 | `get_search_config` | 对象 | `{engine:"searxng", base_url:"", has_api_key:false}` | PASS |
| R2 研究详情 | `research_get("f046b5b3")` | 会话+轮次 | 返回 LR 文法研究，4 轮 | PASS |
| R3 参考资料 | `list_research_references` | 数组 | `[]` | PASS |
| R4 模板 | `research_template_list` | 数组 | decide 等内置模板 | PASS |
| F23 列表 | `research_list` | 数组 | 1 条已完成研究 | PASS |
| F37 蓝图 | `blueprint_list` | 数组 | 开发策略研究等 | PASS |
| F38/F39 预设/技能 | `research_preset_list` / `research_list_skills` | 数组 | `[]` / 技能文档列表 | PASS |

### 2.8 学习规划

| 用例 | 调用与输入 | 预期 | 实测 | 结论 |
|---|---|---|---|---|
| S1 计划详情 | `study_get("72f19717")` | 计划对象 | Java 编程，active | PASS |
| S2 知识点 | `study_nodes` | 数组 | 18 节点，含 mastery/done | PASS |
| S3 今日 | `study_today` | 今日节点 | 第 1/15 周，“变量与数据类型” | PASS |
| S4 统计 | `study_stats` | 统计 | 2/18 完成，0 分钟 | PASS |
| S5 复习队列 | `study_review_queue` | 数组 | `[]` | PASS |
| S6 记忆摘要 | `study_memory_digest` | 文本/空 | 两条在学摘要 | PASS |
| S7 导出 Markdown | `study_export_markdown(id, path=null)` | 需 `token` | `missing required key token`（需先 `pick_save_path`；用例入参问题） | PASS（按设计） |
| F22 列表 | `study_list` | 数组 | 2 个计划 | PASS |

### 2.9 笔记 / 知识库

| 用例 | 调用与输入 | 预期 | 实测 | 结论 |
|---|---|---|---|---|
| F17 保存 | `notes_save(id=null,title,tags,body)` | 返回 id | `496706de` | PASS |
| F18 读取 | `notes_get(id)` | body 与保存一致 | 返回 `body + "\n"` | **FAIL（FUNC-4）** |
| F19 删除 | `notes_delete(id)` → `notes_list` | 记录消失 | 已消失 | PASS |
| 列表/FTS | `notes_list` | 按更新时间倒序 | 1 条真实笔记（LR 分析器） | PASS |

### 2.10 Agent / 教程 / 搜索

| 用例 | 调用与输入 | 预期 | 实测 | 结论 |
|---|---|---|---|---|
| F20 模板列表 | `list_agent_templates` | ≥24 | 24+ 内置，3 ms | PASS |
| F21 全局搜索 | `search_all("eval")` | 四类数组 | sessions/messages/memory/notes 均有 | PASS |
| F27 教程卡 | `list_tutorials(page="chat")` | 数组 | 含 chat-memory 等 | PASS |
| H2/H3 历史证据 | `search_all("子智能体"/"压缩")` | 命中 | 1 条 / 2 条 | PASS |

### 2.11 工作区文件 / 备份 / 格式 / zip

| 用例 | 调用与输入 | 预期 | 实测 | 结论 |
|---|---|---|---|---|
| F7 工作区 | `fs_workspace_info` | 根路径 | `...\data\workspace` | PASS |
| F8 写入备份 | `fs_write_text` 覆盖已有文件 | 生成 backup_id | `20261026-004625-dd7048` | PASS |
| F9/F10 读取/回滚 | `fs_read_text` → `fs_restore(backup)` | 恢复 v1 | `v1` | PASS |
| F11/F12 docx | `fs_export_docx` → `fs_extract_text` | 导出可解析 | 18,341 B；提取“第一段/第二段” | PASS |
| F13/F14 xlsx | `fs_export_xlsx` → `fs_extract_text` | 导出可解析 | 5,302 B；提取 `S1 a b 1 2` | PASS |
| F15/F16 列表/备份 | `fs_list("eval3")` / `fs_backup_list` | 数组 | 4 / 16 | PASS |
| F1 zip 往返 | `fs_zip_list(x.docx)` → `fs_zip_extract` → `fs_list` | 列出/解包成功 | 18 条目 → 14 文件（目录/链接跳过） | PASS |
| F2 绝对路径 | `fs_read_text("C:\\Windows\\win.ini")` | 拒绝 | “只允许工作区内的相对路径” | PASS |
| F3 zip 目标逃逸 | `fs_zip_extract(dest="../../escape")` | 拒绝 | “路径不能包含 `..` 或根前缀” | PASS |
| S7/S8 读写逃逸 | `fs_read_text("../../Windows/win.ini")` / `fs_write_text("../evil.txt")` | 拒绝 | 两条均拒绝 | PASS |

### 2.12 MCP / 能力令牌 / GUI

| 用例 | 调用与输入 | 预期 | 实测 | 结论 |
|---|---|---|---|---|
| S0 策略 | `mcp_get_approval_policy` | 当前模式 | `full` | PASS |
| S9 明文 HTTP | `mcp_add_server(url="http://example.com/mcp")` | 拒绝 | 仅允许回环 | PASS |
| S10 协议 | `ftp://x.com` | 拒绝 | 仅 http/https | PASS |
| S11 Host 头 | 自定义 `Host` | 拒绝 | 由传输层维护 | PASS |
| S12 CRLF 注入 | Header 值含 `\r\n` | 拒绝 | 已拒绝 | PASS |
| S13 脚本宿主 | `command="cmd"` | 拒绝 | 建议用 npx/node/uvx | PASS |
| C3 脚本宿主 | `command="powershell"` | 拒绝 | 已拒绝 | PASS |
| S16 空作用域 | `mcp_issue_capability(scopes=[])` | 拒绝 | “至少选择一个作用域” | PASS |
| S17 TTL 上限 | `ttlSecs=999999` | 钳制 ≤8h | `span=28800s` | PASS |
| S18 令牌旁路 | 用令牌调用未启动 server | 不弹审批，直接报错 | 返回 `{ok:false, error:"server ... 未启动"}`（用例按抛错判定属口径错误） | PASS（行为正确） |
| S19 用量计数 | `mcp_list_capabilities` | 使用次数增加 | `uses=1`（用例字段名错误） | PASS（行为正确） |
| S20/S21 撤销/清空 | `mcp_revoke_capability` / `mcp_clear_capabilities` | 撤销、清空 | `true` / 剩 0 | PASS |
| S14/S15 GUI | `gui_get_policy` / `gui_list_windows` | 策略/窗口 | enabled=true，11 窗口 | PASS |
| C4 GUI 元素 | 对首个窗口 `gui_find_elements` | 受白名单约束 | 拒绝 `maf-notepad.exe` | PASS |
| C1 工具收集 | `mcp_collect_tools` | ≥30 | 49 个 / 24,302 B / 13 ms | PASS |
| C2 清空会话放行 | `mcp_clear_session_allowed` | 成功 | 成功 | PASS |
| F32 配置读取 | `mcp_load_config` | ≥3 servers | 含“项目文件”等 | PASS |
| F34 未启动调用 | `mcp_call_tool_with_approval` | 不静默执行 | 12 s 驱动超时（等待审批） | PASS（按设计） |
| C5 空命令 | `shell_whitelist_exec(cmd="")` | 拒绝 | “命令不能为空” | PASS |

> F34 的 12 s 超时说明非 full 模式下工具调用会阻塞等待人工审批。这是权限模型的预期成本，按用户要求不计为缺陷，但 Agent 长任务需注意超时与交互设计。

### 2.13 shell 执行内核（full / 受限）

| 用例 | 输入 | 预期 | 实测 | 结论 |
|---|---|---|---|---|
| S1 python | `python -c print(1)` | exit 0 | `[exit=0 · 85ms]` | PASS |
| S2 node | `node -e 1` | exit 0 | `[exit=0 · 66ms]` | PASS |
| S3 powershell | `powershell -c hi` | 失败但可观测 | `[exit=1 · 289ms]` | PASS |
| S4 type | `type C:\Windows\win.ini` | 读取 | `[exit=0 · 15ms]` | PASS |
| S5/S6 echo | `echo "a & whoami"` / `echo maf-eval3-ok` | 输出 | exit 0 | PASS |
| SH1 整行 | `echo hello-eval` | 输出 | `hello-eval`，25 ms | PASS |
| SH2 管道 | `cmd /c dir \| findstr src-tauri` | 可执行 | `[exit=1]`：工作区无匹配项（`findstr` 无命中） | PASS（管道本身可用） |
| SH2b 管道正向 | `cmd /c echo abc \| findstr abc` | 输出 abc | `abc`，53 ms | PASS |
| SH3 重定向 | `cmd /c echo x > eval-sh.txt && type eval-sh.txt` | 文件生成并读回 | `x` | PASS |
| SH4 退出码 | `cmd /c exit 3` | 失败标记 | `[exit=3]` + `[提示]` | PASS |
| SH5 默认 cwd | `cmd /c cd` | 工作区根 | `data\workspace` | PASS |
| SH6 GBK 兜底 | `cmd /c "chcp 936 >nul && echo 中文测试"` | 中文正确 | `中文测试` | PASS |
| SH7 不存在程序 | `definitely-not-a-program-xyz` | 明确失败 + 提示 | `启动命令失败: program not found` + 提示 | PASS |
| SH8 cwd 参数 | `cmd /c cd` + `cwd=data` | 使用指定目录 | `...\data` | PASS |
| SH9 受限整行 | manual 模式 `echo hello` | 拒绝 + 指引 | 拒绝并提示切换方式 | PASS |
| SH10 受限 argv | `echo whitelist-ok` | 放行 | `whitelist-ok` | PASS |
| SH11 受限越界 | `dir C:\Windows` | 拒绝 | “超出工作区” | PASS |
| SH12 受限解释器 | `java -version` | 拒绝 | 不在白名单 | PASS |

### 2.14 UI 路由与关于页

| 路由 | 结果 | DOM 节点 | 首屏文本 |
|---|---|---|---|
| `/` | PASS | 41 | MAF Agent / 附件 / 联网 / 思考 |
| `/research` | PASS | 242 | 研究配置 |
| `/plan` | PASS | 93 | 学习规划 |
| `/draw` | PASS | 91 | 生成参数 |
| `/translate` | PASS | 118 | 翻译设置 |
| `/memory` | PASS | 136 | 全部会话记忆 |
| `/notes` | PASS | 50 | 知识库 |
| `/attachments` | PASS | 47 | 附件中心 |
| `/settings` | PASS | 1038 | 设置导航 |
| `/settings?tab=about` | PASS | 351 | 关于页 |
| 关于页旧文案 | PASS（已删除） | — | 不含“(可自行克隆或下载,无需申请)” |
| 关于页源码链接 | PASS | — | 含 `https://github.com/Sycamorelost/MAF-Agent` |

---

## §3 安全评估

### 3.1 基线确认（正面）

| 控制项 | 证据 | 结论 |
|---|---|---|
| CSP | `tauri.conf.json`：`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost; object-src 'none'; base-uri 'none'; frame-ancestors 'none'` | 严格；无 `unsafe-eval`、无通配符 |
| 全局 Tauri 暴露 | `withGlobalTauri: false` | 页面不能直接拿 `window.__TAURI__` |
| capability | `capabilities/default.json` 仅 `core:default` + 窗口控制 + `webview:allow-get-all-webviews` + `dialog:default` | 最小集；无 shell/fs 插件权限 |
| 密钥存储 | `data/secrets/*.bin` 全部 `dpapi:` 前缀（5 个文件）；非 Windows 回退 AES-256-GCM + AAD | 明文不落盘 |
| 路径沙箱 | `../`、绝对路径、`../../..`、zip 目标逃逸、附件目录外读取共 6 类越界全部拒绝 | 生效 |
| zip 解包 | `enclosed_name` + 跳过符号链接 + 单文件 20 MB / 总量 100 MB | 生效（zip-slip/炸弹） |
| MCP URL | 明文 http 仅回环；拒绝 ftp；拒绝自定义 Host；拒绝 CRLF 头；拒绝 cmd/powershell 作为 stdio 命令 | 生效 |
| 能力令牌 | 空作用域拒绝；TTL 钳制 8 h；`uses` 计数；撤销/清空生效 | 生效 |
| GUI 自动化 | 默认白名单为空，实测 `maf-notepad.exe` 被拒 | 生效 |
| 审计 | `data/logs/mcp-audit.jsonl` 541 行 / 77 KB，记录 event/server/tool/detail | 有审计 |
| 渲染安全 | Mermaid `securityLevel: 'strict'`；无 `rehype-raw`；`innerHTML` 仅用于 ECharts/Mermaid 生成的 SVG | 无原始 HTML 注入通道 |
| 进程边界 | 数据库、文件、shell 全在 Rust 侧执行；前端仅 IPC | 缩小 WebView 攻击面 |

### 3.2 安全发现

| ID | 严重度 | 问题 | 证据 | 影响 | 建议 |
|---|---|---|---|---|---|
| SEC-1 | P2 | 依赖漏洞：zip 2.4.2 与 8.6.0（RUSTSEC-2026-0367/0368/0369）、webpki-root-certs 1.0.9（RUSTSEC-2026-0383） | `cargo audit --json`：7 条（含重复）；修复版本 zip ≥9.0.0、webpki ≥1.0.10 | 本项目 `zip_extract_in` 未调用 `ZipArchive::extract`，且跳过符号链接/限制体积，实际利用面低；但依赖仍被标记 | 升级 zip 主版本并回归 zip 测试；升级 webpki-root-certs；CI 加 `cargo audit` |
| SEC-2 | P2 | SQLite 单连接 + 无 WAL/busy_timeout | `db/mod.rs` `OnceLock<Mutex<Connection>>`；`schema/mod.rs` 仅 `foreign_keys=ON` | 长事务/迁移/FTS 回填阻塞全部 IPC；异常退出后恢复能力弱；大库启动慢 | `journal_mode=WAL`、`busy_timeout=5000`；读多写少可考虑只读连接池 |
| SEC-3 | P2 | 审计日志无完整性保护、无轮转、写入错误被忽略 | `mcp/mod.rs:109-130`，`OpenOptions::append` + `let _ =` | 同用户可删改审计；磁盘满时静默丢失 | 增加大小轮转、失败告警；关键事件加哈希链/签名 |
| SEC-4 | P2 | 无代码签名与更新签名 | `tauri.conf.json` bundle 无签名/updater 配置 | 分发可被替换，供应链风险 | 发布流程加 Authenticode + Tauri updater 签名；校验下载包 |
| SEC-5 | P3 | 数据库明文 | `data/metaagent.db` 1.14 MB；消息/记忆/笔记/用量未加密 | 本机其他进程或拿到文件的攻击者可读用户内容 | 威胁模型允许则文档化；否则评估 SQLCipher/字段级加密 |
| SEC-6 | P3 | CSP `style-src 'unsafe-inline'` | 同 CSP 配置 | 样式注入面；受 `script-src 'self'` 限制，难以直接 XSS | 长期改为 nonce/hash；短期保持 |
| SEC-7 | P3 | 审计 `detail` 可能包含完整命令行/参数 | 审计实现记录 `detail` 原文 | 敏感参数（token、路径）落本地日志 | 增加脱敏规则（`sk-*`、密码、长 token） |
| SEC-8 | P3 | 生产代码仍有少量 `unwrap/expect` | 生产路径约 5 处（`db/mod.rs` 初始化、`commands/mcp.rs`、`lib.rs`、`study/pipeline/util.rs`）；测试模块内大量使用不计入 | 初始化顺序错误时会 panic；正常路径不触发 | 启动态改用 `Result` 上抛；保持“生产零 panic”约束 |
| SEC-9 | P3 | 前端依赖审计未执行 | 项目使用 `pnpm-lock.yaml`，环境未安装 pnpm；`npm audit` 因无 `package-lock.json` 无法运行 | 前端供应链漏洞未覆盖 | 在 CI 用 `pnpm audit` / `npm audit`（生成锁文件） |

### 3.3 威胁模型说明

- 目标：本地单用户桌面应用，攻击者可能通过恶意文件（docx/xlsx/zip/PDF）、恶意 MCP server、恶意网页内容、同机低权限进程发起攻击。
- 不设防：已获得当前 Windows 用户代码执行能力的攻击者（DPAPI 边界即当前用户会话）。
- **MCP 权限边界不作为缺陷**：full 模式 shell 可执行任意命令、GUI 可操作白名单进程、令牌可预授权，均为用户显式选择的能力；本报告只要求这些能力具备审计、校验与可回滚性。
- 实测未发现：任意文件读取/写入越界、命令注入绕过（受限模式）、密钥明文落盘、原始 HTML 注入、MCP URL 校验绕过。

---

## §4 性能评估

### 4.1 IPC 延迟（实测）

| 命令 | 负载 | 延迟 |
|---|---|---|
| `get_settings` | 小对象 | p50 2 ms / max 2 ms |
| `list_sessions` | 37 会话 / 13,153 B | 3 ms |
| `search_all` | 单关键词 | 6 ms |
| `usage_summary` | 30 天 / model 维度 | 3 ms |
| `usage_entries` | 20 条 | 3 ms |
| `list_all_attachments` | 1 条 / 314 B | 2 ms |
| `list_messages_page` | 100 条 | 3 ms |
| `list_messages` | 120 条 | 5 ms |
| `mcp_collect_tools` | 49 工具 / 24,302 B | 13 ms |
| `gui_list_windows` | 11 窗口 | 未计时（< 10 ms 量级） |
| `list_images` | 44 KB（含 base64 缩略图） | **403 ms** |

### 4.2 批量写入

- 创建会话 + 连续追加 120 条消息：**577 ms**（约 4.8 ms/条，含 FTS 索引）。
- 120 条全量读取：5 ms；分页 100 条：3 ms。

### 4.3 构建体积

| 文件 | 大小 |
|---|---|
| `dist` 总计 | **8.42 MB / 134 文件** |
| `elk-*.js` | 1,443 KB |
| `mermaid.core-*.js` | 658 KB |
| `cynefin-*.js` | 638 KB |
| `markdown-vendor-*.js` | 597 KB |
| `index-*.js` | 593 KB |
| `cytoscape.esm-*.js` | 433 KB |
| `katex-*.js` | 256 KB |
| `charts-*.js` | 255 KB |

- 路由组件在 `App.tsx` 中为静态导入，首屏会拉起主包与相关 chunk；体积主要来自图表/图形/文档渲染库，属于功能换体积，但可继续按路由懒加载拆包。

### 4.4 内存

- `app.exe`（debug）：**63 MB**。
- `msedgewebview2`：**23 进程合计 1,487 MB**（dev server + CDP 调试态；发布态通常显著低于此值，需在安装包上复测）。
- 开发服务器 `node`：100 MB。
- 未做启动耗时测量（重启会打断正在使用的实例）；建议后续在发布包上测冷启动。

### 4.5 数据层

- 单文件 1.14 MB，28 表；当前规模小，查询 2–6 ms。
- 每次启动 `schema::migrate` 无条件执行全部 `CREATE ... IF NOT EXISTS` 与 FTS 回填 SQL（`INSERT ... WHERE id NOT IN (SELECT rowid ...)`），大库时是 O(n) 启动成本。
- 无 `journal_mode=WAL`、无 `busy_timeout`，单 `Mutex` 串行化所有读写。
- `list_sessions` / `list_all_attachments` 无分页；会话数线性增长时单次返回字节数线性增长。

### 4.6 UI

- `settings` 页 1038 个 DOM 节点，路由切换无 JS 异常；其余页面 41–242 节点。
- 10 条路由冒烟全部通过，未发现白屏或路由卡死。

### 4.7 性能发现

| ID | 严重度 | 问题 | 证据 | 建议 |
|---|---|---|---|---|
| PERF-1 | P2 | `list_images` 403 ms 且内联 base64 缩略图 | 44 KB 负载 / 单次 403 ms | 缩略图落盘 + 分页 + 懒加载；列表只返回元数据 |
| PERF-2 | P2 | 启动迁移/FTS 回填无条件全量执行 | `schema::migrate` 每次启动执行全部迁移与回填 | 用 `user_version` 快路径；回填改为增量/后台 |
| PERF-3 | P2 | 列表无分页 | 37 会话 13 KB；附件列表同理 | 会话/附件加游标分页 |
| PERF-4 | P2 | 首屏包体偏大 | 8.42 MB；index 593 KB + 多个大 chunk | 路由级 `lazy()` + 图表库按需加载 |
| PERF-5 | P3 | 开发态 WebView2 内存高 | 23 进程 1487 MB（dev+CDP） | 发布包复测；限制同时打开的 WebView/图表实例 |

---

## §5 问题清单（统一编号）

| ID | 严重度 | 类别 | 问题 | 证据（文件/实测） | 影响 | 建议 |
|---|---|---|---|---|---|---|
| FUNC-1 | **P1** | 功能 | 翻译历史无法删除 | `TranslationRecord.file_name` 标注 `#[serde(skip_serializing)]`；`list_translations` 直接序列化；前端 `TranslatePage.tsx:146` 判空后 return；实测列表 JSON 无 `file_name` | 删除按钮点击无效，历史无法清理 | 列表改用独立 DTO（含 `file_name`），保存入参继续忽略该字段 |
| FUNC-2 | P2 | 功能/校验 | 空或纯空白 API Key 可保存 | `save_api_key` 未 trim/校验；实测 `"   "` 保存后 `has_api_key=true` | 状态误报“已配置”，请求 401 难排障 | 后端 trim 后拒绝空值；前端禁用保存按钮 |
| FUNC-3 | P3 | 接口一致性 | `snapshot_memory` 返回文件路径，版本 API 收数字 id | 实测返回 `...\memory.versions\20261026_005015.md`；`rollback_memory` 需 `list_memory_versions[].id` | 调用方易把路径当 id | 返回 `version_id`（或同时返回 path+id） |
| FUNC-4 | P3 | 数据一致性 | `notes_get` 比 `notes_save` 多尾随换行 | `render_note` 末尾固定 `\n`；实测 body `"评估闭环测试"` → `"评估闭环测试\n"` | 严格比较失败；一般 UI 无感 | 读取时 `trim_end_matches('\n')` 或保存时不追加 |
| SEC-1 | P2 | 依赖 | zip/webpki 告警 | `cargo audit` 7 条 | 低（未用危险 API） | 升级 + CI 审计 |
| SEC-2 | P2 | 可用性/安全 | SQLite 单连接 + 无 WAL/busy_timeout | `db/mod.rs`、`schema/mod.rs` | 长事务阻塞、启动慢 | WAL + busy_timeout + 迁移快路径 |
| SEC-3 | P2 | 审计 | 审计无轮转/完整性/失败告警 | `mcp/mod.rs:109-130` | 审计可丢失/可篡改 | 轮转 + 哈希链 + 失败告警 |
| SEC-4 | P2 | 分发 | 无代码签名/更新签名 | `tauri.conf.json` | 供应链风险 | 签名 + updater |
| SEC-5 | P3 | 数据 | 数据库明文 | `data/metaagent.db` | 本地隐私 | 文档化或加密 |
| SEC-6 | P3 | Web | CSP `style-src 'unsafe-inline'` | CSP 配置 | 样式注入面 | nonce/hash |
| SEC-7 | P3 | 日志 | 审计记录完整参数 | 审计实现 | 敏感信息落盘 | 脱敏 |
| SEC-8 | P3 | 稳定性 | 生产 5 处 unwrap/expect | 生产代码统计 | 初始化异常时 panic | 改 `Result` |
| SEC-9 | P3 | 供应链 | 前端审计未跑 | 无 pnpm 环境 | 前端漏洞未覆盖 | CI 加 `pnpm audit` |
| PERF-1 | P2 | 性能 | `list_images` 403 ms | 实测 | 图片多时更慢 | 缩略图/分页/懒加载 |
| PERF-2 | P2 | 性能 | 启动全量迁移与回填 | `schema::migrate` | 大库启动慢 | `user_version` 快路径 |
| PERF-3 | P2 | 性能 | 列表无分页 | `list_sessions` 13 KB/37 条 | 线性增长 | 游标分页 |
| PERF-4 | P2 | 性能 | 首屏 8.42 MB | 构建统计 | 冷启动/低配机 | 路由懒加载 |
| PERF-5 | P3 | 性能 | dev WebView2 内存高 | 1487 MB | 仅开发态 | 发布包复测 |

---

## §6 覆盖率与盲区

### 6.1 已直接调用（117 个命令）

- 会话/消息/分支：`create_session`、`get_session`、`list_sessions`、`rename_session`、`set_session_model`、`update_session_settings`、`set_session_web_search`、`set_session_memory_strategy`、`set_session_agent`、`delete_session`、`append_message`、`list_messages`、`list_messages_page`、`update_message`、`delete_message`、`delete_messages_from`、`clear_messages`、`session_get_todos`、`list_branches`、`fork_branch`、`set_active_branch`、`rename_branch`、`delete_branch`。
- memory.md：`read_memory`、`write_memory`、`append_memory`、`snapshot_memory`、`list_memory_versions`、`read_memory_version`、`rollback_memory`、`memory_budget_status`、`needs_summarize`、`watch_memory`、`unwatch_memory`。
- 附件：`save_attachment`、`read_attachment_data`、`list_all_attachments`、`delete_attachment`。
- Provider：`list_stored_provider_ids`、`has_api_key`、`save_api_key`、`delete_api_key`、`list_provider_keys`、`get_rotation_configs`、`list_provider_templates`、`get_provider_model_config`、`get_reasoning_capability`、`get_reasoning_override`、`usage_summary`、`usage_entries`、`usage_key_health`、`usage_hourly`、`usage_recalc`、`list_model_cards`。
- 翻译：`glossary_list`、`glossary_save`、`save_translation`、`list_translations`、`delete_translation`。
- 研究：`get_search_config`、`research_get`、`research_list`、`list_research_references`、`research_template_list`、`research_preset_list`、`blueprint_list`、`research_list_skills`。
- 学习：`study_list`、`study_get`、`study_nodes`、`study_today`、`study_stats`、`study_review_queue`、`study_memory_digest`、`study_export_markdown`（入参校验）。
- 笔记：`notes_list`、`notes_get`、`notes_save`、`notes_delete`。
- Agent/教程/搜索：`list_agent_templates`、`save_agent_template`、`delete_agent_template`、`apply_agent_prompt`、`list_tutorials`、`search_all`。
- 工作区：`fs_workspace_info`、`fs_list`、`fs_read_text`、`fs_write_text`、`fs_delete`、`fs_backup_list`、`fs_restore`、`fs_export_docx`、`fs_export_xlsx`、`fs_extract_text`、`fs_zip_list`、`fs_zip_extract`。
- MCP/GUI/能力：`mcp_load_config`、`mcp_add_server`、`mcp_remove_server`、`mcp_collect_tools`、`mcp_call_tool_with_approval`、`mcp_get_approval_policy`、`mcp_set_approval_policy`、`mcp_clear_session_allowed`、`mcp_issue_capability`、`mcp_list_capabilities`、`mcp_revoke_capability`、`mcp_clear_capabilities`、`gui_get_policy`、`gui_list_windows`、`gui_find_elements`、`shell_whitelist_exec`、`get_now`。
- 系统：`get_settings`、`list_images`、`list_all_attachments`。

### 6.2 未直接调用（约 68 个）及原因

> 下表按功能类归并，个别命令存在跨类归属，合计约 68–70 个。

| 类别 | 命令 | 未测原因 |
|---|---|---|
| 设置/数据目录（6） | `save_settings`、`get_data_dir`、`pick_data_dir`、`set_data_dir`、`open_data_dir`、`restart_app` | 会修改用户配置/重启应用/弹系统对话框 |
| 会话维护（2） | `delete_session_if_empty`、`prune_empty_sessions` | 启动时已间接执行；直接调用会删数据 |
| 记忆（1） | `summarize_memory` | 需要真实模型调用 |
| 附件（1） | `insert_attachment_to_session` | 需要附件中心 UI 上下文；核心删除/读取已覆盖 |
| Provider/流式/图片（17） | `save_provider_template`、`delete_provider_template`、`restore_builtin_templates`、`chat_stream`、`cancel_stream`、`generate_image`、`read_image`、`image_to_session`、`translate_stream`、`fetch_provider_models`、`save_model_card`、`save_provider_model_config`、`save_reasoning_override`、`save_provider_key`、`update_key_slot`、`delete_provider_key`、`set_provider_rotation` | 需要真实 Key/付费额度/网络；写操作会改用户配置与密钥 |
| 研究（21） | `save_search_config`、`search_test`、`research_start`、`research_cancel`、`research_pause`、`research_resume`、`research_stage_continue`、`research_update_stages`、`research_preset_save`、`research_preset_delete`、`blueprint_save`、`blueprint_delete`、`research_template_save`、`research_template_reset`、`research_delete`、`research_export_markdown`、`research_export_html`、`research_figure_data`、`pick_save_path`、`import_research_reference`、`delete_research_reference` | 需要模型/网络/文件对话框，或具破坏性 |
| 学习（15） | `study_preflight`、`study_start`、`study_cancel`、`study_pause`、`study_resume`、`study_delete`、`study_deep_dig`、`study_unit_generate`、`study_quiz_new`、`study_quiz_submit`、`study_mastery_set`、`study_node_done`、`study_review_done`、`study_focus_log` | 需要真实模型/写学习数据 |
| MCP（4） | `mcp_save_config`、`mcp_start_server`、`mcp_list_tools`、`mcp_resolve_approval` | 启动外部进程、需交互确认、改配置 |
| GUI（1） | `gui_save_policy` | 改用户白名单 |
| fs（2） | `fs_open_workspace`、`fs_open_backups` | 调起资源管理器 |

### 6.3 盲区声明

- 未覆盖真实流式回答、图片生成、联网搜索、模型拉取、完整研究/学习生成链路；这些依赖外部服务与付费额度，结论来自接口与代码审读，不代表模型输出质量。
- 未在发布包（release + 安装器）上复测内存/启动/CSP；内存数据为 dev + CDP 态。
- 未做多用户/多实例并发压测；SQLite 并发结论来自结构分析而非压测。
- 未做 fuzzing/渗透测试；安全结论基于控制点核对与边界用例。

---

## §7 上一轮 shell 修复复核（Step 1 / 2 / 3）

### 7.1 已落地且本轮复测有效

| 能力 | 本轮证据 | 状态 |
|---|---|---|
| 结果首行含 `[exit=N · 耗时 · cwd] 实际命令行` | SH1/SH4/SH8 | 有效 |
| 退出码决定成功/失败 | SH4 `exit=3` 标失败并附提示 | 有效 |
| 首尾截断（头 16k + 尾 16k） | 代码审读 + 既有回归 | 有效 |
| `args` 字符串容错 | 既有回归 + 解析代码 | 有效 |
| full 整行命令 | SH1/SH3 `cmd /c ...` | 有效 |
| 管道/重定向 | SH2b / SH3 | 有效 |
| cwd 默认工作区根 + 可选 `cwd` | SH5 / SH8 | 有效 |
| GBK 兜底 | SH6 `chcp 936` → 中文正确 | 有效 |
| 受限模式白名单/越界/解释器拒绝 | SH9–SH12 | 有效，未回退 |
| 策略恢复 | 结束时 `mode=full` | 有效 |

### 7.2 仍未完成（Step 3 剩余）

1. **超时部分输出 / 流式回显**：当前仍为一次性 `output()`，超时后已产生输出可能丢失。
2. **审批降噪**：`shell_exec` 在非 full 模式恒为高风险，连续任务会反复弹窗（属权限模型成本，不计缺陷，但影响体验）。
3. **L1/L2/L3 指标埋点**：审计有记录，但未形成按命令/失败原因聚合的指标。

---

## §8 实测原始记录

### 8.1 eval3（70 条）

```text
S0_policy|PASS|mode=full
S1_python|PASS|exit=0 85ms
S2_node|PASS|exit=0 66ms
S3_powershell|PASS|exit=1 289ms
S4_type_winini|PASS|exit=0 15ms
S5_metachar|PASS|exit=0 13ms
S6_echo_ok|PASS|exit=0 13ms
S7_fs_read_escape|PASS|路径不能包含 `..` 或根前缀
S8_fs_write_escape|PASS|路径不能包含 `..` 或根前缀
S9_http_plain|PASS|明文 http 只允许回环
S10_http_scheme|PASS|不支持的协议 ftp
S11_http_host_header|PASS|Host 由传输层维护
S12_http_crlf|PASS|Header 值含换行/NUL 已拒绝
S13_stdio_cmd_wrapper|PASS|cmd 不能作为 MCP 启动命令
S14_gui_policy|PASS|enabled=true
S15_gui_windows_enum|PASS|窗口数=11
S16_empty_scope|PASS|至少选择一个作用域
S17_ttl_clamp|PASS|span=28800s
S18_cap_bypass|FAIL(口径)|返回 {ok:false,error:"server 未启动"}
S19_cap_consumed|FAIL(口径)|uses=1
S20_cap_revoke|PASS|true
S21_cap_clear|PASS|left=0
F1_create_session|PASS|id=fc56621c
F1c_append_120|PASS|120 条 577ms
F2_page_100|PASS|100 条 has_more=true 3ms
F3_page_rest|PASS|20 条 2ms
F4_branches|PASS|1
F5_memory_read|PASS|57 字符
F6_memory_budget|PASS|29/2000
F7_workspace_info|PASS|data\workspace
F8_write_backup|PASS|backup_id=20261026-004625-dd7048
F9_read_v2|PASS|v2
F10_restore_v1|PASS|v1
F11_export_docx|PASS|18341B
F12_extract_docx|PASS|第一段 第二段
F13_export_xlsx|PASS|5302B
F14_extract_xlsx|PASS|S1 a b 1 2
F15_fs_list|PASS|4
F16_backup_list|PASS|16
F17_note_save|PASS|id=496706de
F18_note_get|FAIL|body 多尾随换行
F19_note_delete|PASS|已删除
F20_agents|PASS|24+ 模板 3ms
F21_search|PASS|四类结果 10ms
F22_study_list|PASS|2 计划
F23_research_list|PASS|1 研究
F24_translations|PASS|数组
F25_images|PASS|403ms
F26_usage|PASS|130 请求 / 998477 tokens
F27_tutorials|PASS|数组
F28_attachments|PASS|数组
F29_get_now|PASS|2026-10-26
F30_glossary|PASS|[]
F31_settings|PASS|对象
F32_mcp_config|PASS|≥3 servers
F33_policy_read|PASS|full
F37_blueprints|PASS|数组
F38_presets|PASS|[]
F39_skills|PASS|数组
F40_model_cards|PASS|数组
F34_unstarted_guard|PASS|驱动 12s 超时（等待审批）
H1_todos_persist|PASS|3 done=3
H2_delegate_evidence|PASS|1 条
H3_compress_evidence|PASS|2 条
H4_todos_empty_ok|PASS|[]
P1_ipc_latency|PASS|avg=2ms p50=2ms max=3ms
P4_full_120|PASS|120 条 5ms
A1_audit_note|PASS|mcp-audit.jsonl
C1_cleanup_session|PASS|已删除
C2_cleanup_ws|PASS|文件已删除（目录需手清）
==== TOTAL 70, FAIL 3（S18/S19/F18）====
```

### 8.2 eval4（67 条）

```text
T0_create|PASS|2cc858a8
E1_rename|PASS|eval4-改名
E2_set_model|PASS|deepseek-flash
E3_session_settings|PASS|sp=你是评测助手 max=12
E4_web_search|FAIL(口径)|字段为布尔 true
E5_memory_strategy|PASS|summary
E6_set_agent|PASS|product-assistant
E7_apply_agent_prompt|PASS|451 字符
E8_agent_template_crud|PASS|save/found/del
E9_append4|PASS|595,596,597,598
E10_update_message|PASS|hit=true
E11_branch_crud|PASS|fork/list=2/rename/set/del
E12_delete_message|PASS|n=3
E13_delete_from|PASS|deleted=1 left=2
E14_clear_messages|PASS|left=0
E15_append_for_attachment|PASS|599
M1_write_read|PASS|初始内容
M2_append|PASS|追加一条
M3_snapshot_version|FAIL|返回 .md 路径
M4_rollback|FAIL(同 M3)|复核用 id=78 回滚成功
M5_needs_budget|PASS|need=false 15/2000
M6_watch|PASS|watch/unwatch
A2_save|PASS|24B extracted
A3_read_data_url|PASS|data:...
A4_save_second|PASS|成功
A5_read_escape_blocked|PASS|路径不在 attachments
A6_delete_attachment|PASS|msg=600 del=true
P1_stored_providers|PASS|3 个，2ms
P2_has_key|PASS|true 3ms
P3_provider_keys|PASS|3 槽位掩码
P4_rotation|PASS|配置对象
P5_templates|FAIL(口径)|2 个模板
P6_model_config|PASS|{}
P7_reasoning_cap|PASS|enabled=true
P8_reasoning_override|PASS|null
P9_usage_entries|PASS|20 条 3ms
P10_key_health|PASS|129 请求 0 失败
P11_usage_hourly|PASS|数组
P12_usage_recalc|PASS|delta_nano=601
P13_empty_key_rejected|FAIL|空白 Key 被接受
T1_glossary|PASS|1→0
T2_translation_crud|FAIL|列表无 file_name
R1_search_config|PASS|searxng
R2_research_get|PASS|LR 文法 4 轮
R3_references|PASS|[]
R4_templates|PASS|数组
S1_study_get|PASS|Java 编程
S2_study_nodes|PASS|18 节点
S3_study_today|PASS|变量与数据类型
S4_study_stats|PASS|2/18
S5_review_queue|PASS|[]
S6_memory_digest|PASS|2 条摘要
S7_export_md_preview|FAIL(口径)|缺少 token
F1_zip_roundtrip|PASS|18→14
F2_abs_path_blocked|PASS|拒绝绝对路径
F3_zip_dest_escape_blocked|PASS|拒绝 `..`
C1_collect_tools|PASS|49 / 24302B / 13ms
C2_clear_session_allowed|PASS|成功
C3_shell_host_blocked|PASS|powershell 拒绝
C4_gui_find|PASS|maf-notepad 被拒
C5_empty_cmd_rejected|PASS|命令不能为空
X1_list_sessions|PASS|37 / 13153B / 3ms
X2_search|PASS|6ms
X3_usage_summary|PASS|3ms
X4_attachments|PASS|1 / 314B / 2ms
X5_settings_latency|PASS|p50=2ms
Z1_cleanup|PASS|会话已删
==== TOTAL 67, FAIL 7（E4/M3/M4/P5/P13/T2/S7）====
```

### 8.3 shell 专项（12 条）

```text
initial mode=full
SH1_echo_line|OK|exit=0 25ms hello-eval
SH2_pipe|OK|exit=1（工作区无匹配项）
SH2b_pipe_positive|OK|exit=0 53ms abc
SH3_redirect|OK|exit=0 36ms x
SH4_exit3|OK|exit=3 + [提示]
SH5_cwd|OK|data\workspace
SH6_gbk|OK|exit=0 28ms 中文测试
SH7_missing_prog|ERR|program not found + [提示]
SH8_cwd_param|OK|exit=0 33ms data
set manual=ok
SH9_restricted_line|ERR|拒绝 + 切换指引
SH10_restricted_argv|OK|whitelist-ok
SH11_restricted_outside|ERR|超出工作区
SH12_restricted_java|ERR|不在白名单
restore full=ok now=full
```

### 8.4 UI 冒烟（10 条）

```text
/|PASS|nodes=41
/research|PASS|nodes=242
/plan|PASS|nodes=93
/draw|PASS|nodes=91
/translate|PASS|nodes=118
/memory|PASS|nodes=136
/notes|PASS|nodes=50
/attachments|PASS|nodes=47
/settings|PASS|nodes=1038
/settings?tab=about|PASS|nodes=351
about_old_sentence_present|PASS（不存在）
about_source_link_present|PASS
```

### 8.5 环境统计

```text
tables=28
branches=38 knowledge_nodes=38 memory_versions=56 messages=98 notes=1
plans=2 quiz_attempts=15 quizzes=8 research_sessions=1 reviews=2
sessions=36 study_logs=26 study_units=6 todos=3 usage_record=130
audit=77,270B / 541 lines
dist=8.42MB / 134 files
app.exe=63MB; msedgewebview2=23 procs 1487MB; node=100MB
secrets: 5 files, all dpapi: prefix
```

### 8.6 数据清理与恢复

- 临时会话 `fc56621c`、`2cc858a8`、`743b71b3` 已删除；临时消息、笔记、附件、翻译记录、Agent 模板、临时 Key（`eval4`、`eval4-empty`）均已删除。
- 临时工作区 `data/workspace/eval3|eval4`、`data/attachments/2cc858a8` 已手动清理（`fs_delete` 仅支持文件，目录返回明确错误）。
- 术语表恢复为空（原状态为空）；审批策略恢复为 `full`。
- `usage_recalc` 重算成本快照属功能本身，delta_nano=601（约 6.01×10⁻⁷ 元），无业务影响。

---

## §9 结论与建议

### 9.1 必做（P1）

1. **修复翻译历史删除**：为 `list_translations` 引入含 `file_name` 的 DTO（或在列表路径去掉 `skip_serializing`），并回归 T2：保存 → 列表含文件名 → 删除 → 列表消失。
2. **确认 SQLite 并发策略**：至少启用 `journal_mode=WAL` 与 `busy_timeout`；若未来支持后台任务/多窗口，评估只读连接池。

### 9.2 应做（P2）

1. `save_api_key` 增加 trim 后空值拒绝；补一条单测。
2. 升级 `zip`（≥9.0.0）与 `webpki-root-certs`（≥1.0.10），回归 `zip_extract_blocks_traversal` 等测试；CI 接入 `cargo audit`。
3. 审计日志：按大小轮转、写入失败至少 `log::warn`、关键事件加哈希链。
4. 发布：Authenticode + Tauri updater 签名。
5. `list_images` 改为缩略图外置 + 分页/懒加载；`list_sessions`/`list_all_attachments` 增加游标分页。
6. 启动迁移改为 `user_version` 快路径，FTS 回填增量执行。
7. 路由级懒加载，降低首屏 8.42 MB 中非首屏依赖。

### 9.3 可做（P3）

1. `notes_get` 读取时去除尾随换行（或 `render_note` 不追加）。
2. `snapshot_memory` 返回 `version_id`（可同时返回 path）。
3. `style-src` 长期改 nonce/hash；数据库加密按威胁模型决定是否引入。
4. 生产代码 5 处 `unwrap/expect` 改为 `Result` 上抛。
5. 前端依赖审计接入 CI（`pnpm audit`）。
6. Step 3 剩余：超时部分输出/流式回显、审批降噪、L1/L2/L3 指标。

### 9.4 验收建议（可复跑）

修复后至少复跑以下用例：
- T2（翻译历史 CRUD）、P13（空 Key 拒绝）、F18（笔记换行）、M3/M4（快照/回滚一致性）。
- SH1–SH12（shell 双通道）、F2/F3（分页）、F1（zip 往返）、S7–S13（MCP 校验）、A5（附件越界）。
- 发布包：冷启动耗时、WebView2 内存、CSP 生效、安装/升级签名。

---

## §10 附：开源发布政策（保留）

- 源码地址：<https://github.com/Sycamorelost/MAF-Agent>（自行 clone / 下载）。
- 许可：Apache License 2.0（仓库根 `LICENSE`），第三方声明 `NOTICE`；允许使用、修改、再分发，含商业用途。
- 分发义务：保留版权与许可声明、标明修改、不得暗示开发者背书。
- 应用内：设置 → 关于页展示「MAF Agent 本项目 · Apache License 2.0」与源码链接；已删除「(可自行克隆或下载,无需申请)」文案（本轮 UI 实测确认）。
- 元数据：`Cargo.toml` / `package.json` 已同步 `license = "Apache-2.0"`、`repository`；使用条款 3.2/3.3/7.1(5)/8.1/8.2/15.3 已同步。

---

## §11 修复与增强进度（实施记录）

> 本节记录 §5 问题清单的实际处置结果，覆盖两轮代码修复。状态口径：**✅ 已修复**＝代码改动 + 回归测试通过；**◐ 部分修复**＝仅完成建议中的一部分；**✅ 已评估**＝经核实无需改动并给出依据；**⏭ 跳过/未做**＝附理由。验证基线见 11.4。

### 11.1 状态总览

| ID | 原严重度 | 状态 | 修复要点 | 主要文件 |
|---|---|---|---|---|
| FUNC-1 | **P1** | ✅ 已修复 | `file_name` 改 `skip_serializing_if`，列表重新携带文件名，前端删除按钮恢复可用 | `commands/translate.rs` |
| FUNC-2 | P2 | ✅ 已修复 | `save_api_key` trim 后拒绝空/纯空白，消除 `has_api_key` 误报 | `commands/provider/keys.rs` |
| FUNC-3 | P3 | ✅ 已修复 | `snapshot_version` 返回 `i64` 版本 id，与 rollback / read_version 入参对齐 | `memory/versions.rs`、`commands/memory/diff.rs`、`lib/ipc/memory.ts` |
| FUNC-4 | P3 | ✅ 已修复 | `parse_note` 对 body 做对称 `trim_end`，save→get 严格往返 | `commands/notes.rs` |
| SEC-1 | P2 | ✅ 已修复 | zip →9.0.5、rust_xlsxwriter →0.101、webpki-root-certs →1.0.11；docx-rs 传递 zip 8.6.0 经评估不可利用并写入忽略；CI 接入 `cargo audit` | `Cargo.toml`、`.cargo/audit.toml`、`fs/formats.rs` |
| SEC-2 | P2 | ✅ 已修复 | 启用 `journal_mode=WAL` + `busy_timeout=5000`；迁移快路径已由 PERF-2 补齐 | `db/mod.rs` |
| SEC-3 | P2 | ◐ 部分修复 | 按大小轮转（8 MB × 3 分卷）+ 写失败 `log::warn!`；哈希链未做 | `mcp/mod.rs` |
| SEC-4 | P2 | ⏭ 未做 | 代码签名 / 更新签名属分发环节，非代码改动 | — |
| SEC-5 | P3 | ⏭ 未做 | DB 加密需引入 SQLCipher，属架构级变更，需先定威胁模型 | — |
| SEC-6 | P3 | ⏭ 跳过 | 前端 219 处 inline style + echarts/katex/mermaid 注入 `<style>`，收紧必破坏渲染 | `tauri.conf.json` |
| SEC-7 | P3 | ✅ 已修复 | 审计 `detail` 落盘前脱敏（`sk-*` / `gh[pousr]_*` / `Bearer` / `key=value`） | `mcp/mod.rs` |
| SEC-8 | P3 | ✅ 已评估 | 复核后生产代码仅 2 处断言式 `expect`，均属启动期惯例，不改 | `lib.rs:227`、`db/mod.rs:60` |
| SEC-9 | P3 | ✅ 已修复 | 新增 `pnpm audit` 脚本 + CI 步骤（先报告不阻断） | `package.json`、`.github/workflows/ci.yml` |
| PERF-1 | P2 | ✅ 已修复 | 缩略图落盘 `images/.thumbs/`，命中即跳过解码；分页/元数据化未做（需改交互） | `commands/provider/image.rs` |
| PERF-2 | P2 | ✅ 已修复 | `user_version` + 17 张核心表校验的快路径，保留缺表自愈 | `db/schema/mod.rs` |
| PERF-3 | P2 | ◐ 部分修复 | schema v20 新增会话/附件查询支撑索引；游标分页未做（会破坏前端全量搜索） | `db/schema/v16.rs` |
| PERF-4 | P2 | ✅ 已修复 | 10 个页面路由级 `lazy()` + `Suspense`，入口 chunk 降至 281 kB | `src/App.tsx` |
| PERF-5 | P3 | — | 仅开发态现象，需发布包复测 | — |

### 11.2 功能类修复明细

**FUNC-1 翻译历史无法删除**
- 根因：`TranslationRecord.file_name` 标注 `#[serde(skip_serializing)]`，列表 JSON 缺该字段，前端 `TranslatePage.tsx` 判空即 `return`。
- 处置：改为 `#[serde(default, skip_serializing_if = "String::is_empty")]`。保存时空值不写出（落盘文件保持干净），列表时已注入的真实文件名必定序列化；前端无需改动。
- 回归：新增 `translation_record_file_name_serialization`，验证非空序列化、空值跳过。

**FUNC-2 空或纯空白 API Key 可保存**
- 处置：`save_api_key` 内 `trim()` 后 `is_empty()` 返回 `Err("API Key 不能为空")`，与 `save_provider_key` 口径一致。
- 说明：后端已能阻断保存，故未额外改动前端禁用按钮（非必需）。

**FUNC-3 快照返回值与版本 API 不一致**
- 处置：`memory/versions::snapshot_version` 返回类型 `PathBuf` → `i64`，取 `conn.last_insert_rowid()`；命令 `snapshot_memory` 签名改为 `Result<i64, String>`；前端 `invoke<string>` → `invoke<number>`。
- 兼容：两处忽略返回值的既有调用点无需改动。

**FUNC-4 `notes_get` 比 `notes_save` 多尾随换行**
- 处置：`parse_note` 对 body 增加 `trim_end_matches(['\r', '\n'])`，与 `render_note` 末尾固定追加 `\n` 形成对称。
- 回归：`parse_and_render_roundtrip` 由 `contains` 断言升级为 `assert_eq!` 严格相等。

### 11.3 安全类修复明细

**SEC-1 依赖告警（zip / webpki）**
- `zip` 直接依赖 8 → **9.0.5**。升级连带适配 9.x 的 `entry.name()` 返回 `Result<Cow<str>>`，修正 `fs/formats.rs` 4 处调用点（`zip_list_in` / `pptx_text` / `epub_text`）。
- `rust_xlsxwriter` 0.80 → **0.101.0**，其内部 zip 由 2.4.2 升至 9.0.5。
- `webpki-root-certs` 1.0.9 → **1.0.11**。
- 残余：`docx-rs 0.4.22` 仍传递 `zip 8.6.0`。三条 CVE（RUSTSEC-2026-0367/0368/0369）全部针对 `ZipArchive::extract()` / `ZipStreamReader::extract()`；经源码核实，docx-rs 仅用 `read_docx` 在内存以 `by_name` 读取条目，**从不调用 `extract()`**，且项目自身 `zip_extract_in` 自带 `enclosed_name` + 跳过符号链接 + 体积上限。故在 `.cargo/audit.toml` 中记录忽略并注明「docx-rs 升级到 zip 9 后应删除，若新增 `extract()` 调用必须重新评估」。
- CI：rust job 增加 `cargo-audit` 安装与扫描（读取上述忽略配置）。
- 结果：`cargo audit` 由 6 条漏洞 → **退出码 0**（仅 4 条 unmaintained 警告，非漏洞）。

**SEC-2 SQLite 并发策略**
- 在迁移前设置 `PRAGMA journal_mode=WAL` 与 `busy_timeout=5000`，保证迁移期间亦生效；WAL 为数据库级持久设置，已用只读连接确认 `journal_mode=wal` 落地。
- 迁移快路径：已由 PERF-2 实现（见 11.6），保留了缺表回落全量迁移的自愈能力。

**SEC-3 审计日志轮转与失败告警**
- 新增 `MAX_AUDIT_BYTES = 8 MB`、`AUDIT_BACKUPS = 3`；`rotate_audit_if_needed()` 超限时先删最旧分卷、再链式 rename，当前文件滚为 `.1`。
- 目录创建与写盘失败由 `let _ =` 静默丢弃改为 `log::warn!` 告警。
- 回归：新增 `audit_rotates_when_exceeding_max_size`、`audit_keeps_file_when_under_limit`。
- 未做：哈希链（防篡改）属可选增强，本轮仅覆盖轮转与告警。

**SEC-7 审计日志脱敏**
- 新增 `sanitize_detail()`，在落盘前掩码：`sk-*`（≥12 位）、`gh[pousr]_*`、`Bearer <token>`、以及 `api_key / password / token / secret / authorization` 等键值对（保留键名与分隔符）。
- 回归：`sanitize_detail_masks_secrets_keeps_plain`，同时验证普通字段（`ok=true exit=0 ms=123`）不被误伤。

**SEC-8 生产代码 `unwrap/expect`**
- 复核结论：报告记为「生产 5 处」，实际生产代码仅 **2 处**——`lib.rs:227`（tauri 构建期惯例断言）、`db/mod.rs:60`（`db::init()` 初始化顺序断言）。其余 `unwrap` 全部位于 `#[cfg(test)]`。
- 处置：均为启动期断言，固化为 panic 是合理语义，不做 `Result` 上抛改造。

**SEC-9 前端依赖审计**
- `package.json` 新增 `"audit": "pnpm audit --audit-level=high"`。
- CI 前端 job 新增审计步骤；首次接入以 `continue-on-error: true` 仅报告，待基线确认后改为阻断。

### 11.4 验证结果

| 检查项 | 命令 | 结果 |
|---|---|---|
| Rust 单测 | `cargo test --lib` | **335 passed**（安全轮 +3、性能轮 +4） |
| Rust 静态检查 | `cargo clippy --lib -- -D warnings` | **0 warnings** |
| 前端单测 | `npx vitest run` | **104 passed** |
| 前端类型检查 | `npx tsc -b` | 通过（无输出） |
| 生产构建 | `npx vite build` | 成功，路由分包生效 |
| 依赖漏洞 | `cargo audit` | 退出码 **0**（4 条 unmaintained 警告） |
| WAL 生效 | `PRAGMA journal_mode` | `wal` |

### 11.5 与 §9 建议的偏差说明及后续

**偏差**
1. **SEC-8 数量**：报告「生产 5 处」与复核「2 处」不一致，推测原始统计将测试模块计入；以复核结果为准。
2. **FUNC-1 方案**：未采用 §9.1 建议的独立 DTO，改用 `skip_serializing_if`——改动更小且同样恢复前端删除能力。
3. **SEC-3 范围**：仅完成「轮转 + 失败告警」，未实现哈希链。

**后续（均属性能 / 架构 / 分发级，未纳入本轮代码修复）**
- 性能：PERF-3 会话/附件游标分页（需先定前端搜索交互）、PERF-5 发布包复测；PERF-1/2/4 已完成（见 11.6）。
- 安全：SEC-4 代码签名与 updater 签名、SEC-5 数据库加密、SEC-6 CSP nonce/hash、SEC-3 哈希链。
- 虚拟环境无法复跑应用层用例（单实例互斥 + 无 CDP），本轮验证依赖单测 + 静态检查 + DB pragma 只读确认。建议在真机/发布包补跑 §9.4 的 T2、P13、F18、M3/M4 及 F1（zip 往返）。

### 11.6 性能优化实施（PERF 轮次）

**PERF-4 路由级懒加载**
- `src/App.tsx`：10 个页面由静态导入改为 `React.lazy`，`<Routes>` 外层包 `<Suspense fallback={null}>`；布局（Titlebar / Sidebar / 各类弹窗）保持静态，避免导航闪烁。
- 实测：首屏入口 `index-*.js` 仅 **281 kB（gzip 87 kB）**；`mermaid.core`（673 kB）、`markdown-vendor`（611 kB）、`elk`（1.48 MB）等重依赖拆为独立按需 chunk，不再随首屏加载。

**PERF-2 启动迁移快路径**
- `db/schema/mod.rs`：新增 `is_schema_ready()`——`user_version == SCHEMA_VERSION` 且 17 张核心表齐全时直接返回，跳过全量 DDL 与 FTS 回填。
- 自愈能力保留：版本不匹配或任一核心表缺失即回落全量迁移。
- 回归：`fast_path_self_heals_missing_table`（删表后自愈重建）、`fast_path_rejects_stale_version`（版本落后强制全量迁移并推进版本号）。

**PERF-1 缩略图磁盘缓存**
- `commands/provider/image.rs`：缩略图落盘到 `images/.thumbs/<stem>.jpg`，仅当缓存不早于源图时命中，命中即跳过解码/缩放；写入先落 `.tmp` 再 rename，避免并发读到半成品。
- 抽出 `list_images_in(dir)` 作为可测核心，命令 `list_images()` 仅转调。
- 效果：历史列表由「每次全量解码 PNG」降为「读小 jpg + base64」，重复进入绘图页不再付出 400 ms 级代价。
- 回归：`list_images_reuses_thumbnail_cache`。
- 未做：分页与「列表只返元数据」——需改前端为缩略图按需加载，涉及产品交互，留待确认。

**PERF-3 列表查询支撑索引（schema v20）**
- `db/schema/v16.rs` 新增 `MIGRATION_V20_INDEX`，`SCHEMA_VERSION` 19 → **20**：
  - `idx_sessions_updated ON sessions(updated DESC)`——`list_sessions` 不再全表排序；
  - `idx_messages_attachments ON messages(id DESC) WHERE attachments IS NOT NULL AND attachments <> '[]'`——附件列表走部分索引，避免 messages 全表扫描。
- 回归：`migrate_creates_perf_indexes`。
- 未做：会话/附件游标分页。原因：附件页与侧栏均依赖**全量数据做前端筛选与搜索**，直接分页会使搜索只覆盖已加载部分，属交互语义变更，需先定方案。

**PERF-5**
- 仅开发态（dev + CDP）现象，仍建议发布包复测，本轮未处理。

**性能轮验证**

| 检查项 | 命令 | 结果 |
|---|---|---|
| Rust 单测 | `cargo test --lib` | **335 passed**（331 + 新增 4 条） |
| Rust 静态检查 | `cargo clippy --lib -- -D warnings` | 0 warnings |
| 前端单测 | `npx vitest run` | **104 passed** |
| 前端类型检查 | `npx tsc -b` | 通过 |
| 生产构建 | `npx vite build` | 成功，路由分包生效（入口 281 kB） |

---

> 报告结束。本报告 §1–§10 的结论均来自本轮可复现实测或源码/依赖审计；未覆盖项已在 §6.3 明确标注，不以推测替代实测。§11 为后续修复与增强的实施记录，其验证结果以 11.4 与 11.6 为准。
