# 执行计划 v2（按优先级 × 难度合并）

> **基准**：HEAD `84adee5` / clean 起点 / tsc ✅ / vitest 253 / cargo test 482
> **排序原则**：**P0（安全/数据 bug）→ P1（明显体验 bug）→ P2（清理/小功能）→ P3（大改动新功能）→ P4（调研/架构）**
> **合并逻辑**：同一组件/模块上的多个改动打包做，避免来回改同一个文件

---

## Phase 1 · 安全 & 数据 Bug（最高优先级，不改不行）

**全部是纯 bug 修复，无功能变更，改完立即跑全量测试。**

| 子项 | 影响 | 难度（改几文件） | 改动位置 |
|---|---|---|---|
| 1a 准备阶段取消 → 重复消息 | 用户重发导致 DB 重复行 | 2 | `messages.ts` + `useChatSend.ts` |
| 1b 停止不等后端 + 两条流并发 + cancel 竞态 | 资源泄露 / 缓冲错乱 / 后端冲突 | 5 | `messages.ts` + `provider.ts` + `useChatStream.ts` + `stream.rs` + `useChatSend.ts` |
| 1c 流式期间可重生成/换模型 | cancelRef 被覆盖，旧请求失控 | 3 | `useMessageActions.ts` + `MessageList.tsx` + `ChatPage.tsx` |
| 1d 弹窗自动放行审查 | 右键删除会话已有 ConfirmDialog；MCP Full/Workspace 模式自动放行——你确认要收紧哪些 | 0-3 | 确认后再动 |

**质量门**：`npx tsc -b` / `npx vitest run` / `cargo test --no-default-features --lib`

---

## Phase 2 · 体验 Bug & 布局修复（高优先级，都是看得到的问题）

**全部是 CSS + 组件 props 层面小改，改完立即能看到效果。** 按改动组件归类打包。

### 包 A：Composer/ToolBar 组件（2 个文件）
| 子项 | 现状 | 改动 |
|---|---|---|
| 2a 思考强度浮层被任务条截断 | 浮层 overflow 被容器裁剪 + 位置被 Inspector 遮挡 | 改为抽屉式滑出（transform 过渡）+ z-index 提升到 80 + 全屏后重算锚点 |
| 2b 发送按钮改圆形 + 箭头居中 | `.send` 是圆角矩形 + `IconSend` 偏右上 | CSS `border-radius: 50%` + SVG centering 调整 |
| 2c 主页发送按钮箭头修正 | 同上 | 同 ToolBar.tsx 的 `.send` 类，改完自动覆盖两处 |

### 包 B：侧边栏（1 个目录 3-5 文件）
| 子项 | 改动 |
|---|---|
| 2d 去进度条 | CSS 删除 `.sidebar-progress` 或组件渲染 |
| 2e 整体滚动去固定 | 去掉 header/footer 的 `position: sticky/fixed` |
| 2f 左右边界缩 0.5 倍 | `padding/margin` CSS var 改 |
| 2g 记忆库/知识库独立切换 | 当前 tab 合并 → 拆成左侧独立菜单项 |
| 2h 会话右键菜单地址截断 | padding / overflow 修复 |
| 2i 会话加折叠按钮 + 项目文件夹高度对齐会话 | Menus.tsx + SessionsList 组件 |
| 2j 去"昨天/今天"分组 + 去统计会话数据 | SessionsList 的 groupByTime 逻辑 |
| 2k 会话/项目名改黑体 + 灰色 + 去灰色横线 | CSS |

### 包 C：其他小修（3 文件）
| 子项 | 改动 |
|---|---|
| 2l 新对话按钮主题未适应 | CSS var 补齐 |
| 2m 关闭应用内调试右键菜单 | 加 `import.meta.env.PROD` 守卫 |
| 2n 侧边栏去多余 CSS 冗余 | 同 Phase 3 清理联动 |

**质量门**：`npx tsc -b`（纯 CSS/组件改动，cargo 不需要跑）

---

## Phase 3 · 清理 & 删除 & 小功能（中优先级，改不改不影响核心流程）

**按"前端清理 → 后端清理 → 功能增强"顺序做。**

### 包 A：主题适配 & emoji 清理（纯样式/注释，20 分钟）
| 子项 | 改动 |
|---|---|
| 3a 所有弹窗主题适配（ConfirmDialog / McpApprovalDialog / injected-view / toast / ModelSwitchDialog / AgentPickerDialog） | CSS var 补齐，每个弹窗检查 1-2 个类 |
| 3b emoji 清理（注释 + 测试数据） | 前端 3 处 + 后端 8 处 → 批量 grep 替换 |

### 包 B：删除冗余模块 & 关联（你确认边界后做）
| 子项 | 改动 | 状态 |
|---|---|---|
| 3c 学习中心 | HEAD 已删，清理 dangling blob | ✅ 无工作量 |
| 3d 翻译 | 删除 `builtin.rs` 翻译专家注册 + 对应 prompt md + `humanVoice.ts` 白名单 | 3 文件 |
| 3e AI味检测 | 删除 `builtin.rs` 两个 ai-flavor 注册 + 两个 prompt md + `AgentPickerDialog.tsx` 引用 | 4 文件 |
| 3f 冗余代码/无效文件 | 先按"没被 import / 没在 mod.rs 注册"清单给你看，再删 | 待你确认标准 |

### 包 C：小功能增强（4 文件）
| 子项 | 改动 |
|---|---|
| 3g 会话内文件改列表视图 | OutputCards → 列表 |
| 3h 去掉转 PDF 按钮 + 后端 | OutputCards button + `commands/export` 里的 PDF 导出 |
| 3i 划词选中后弹复制/追问（= 引用统一功能） | MessageBubble 加 `onMouseUp` selection 检测 → popover |
| 3j 引用超阈值折叠 | QuoteTray 加长度判断 + "展开" |
| 3k 文案替换 + 输入栏空 placeholder | 待你确认具体位置后做 |

**质量门**：`npx tsc -b` / `cargo test --lib`（如果删了后端模块）

---

## Phase 4 · 小窗 Popout（独立功能，最大改动，最后做）

**从零重写，无代码可恢复。** 跨 Tauri 后端 + 前端 + 托盘 + 存储。

| 子项 | 依赖 | 改动位置 |
|---|---|---|
| 4a Tauri WebviewWindow popout-{sessionId} 创建逻辑 | 无 | `lib.rs` 新 command / 现有 tray 扩展 |
| 4b 默认位置：桌面右下角贴任务栏上方 | 4a | Tauri 后端 `app_handle.shell().display()` |
| 4c 托盘双击 → 触发 popout create/focus | 4a | 现有 tray event handler 扩展 |
| 4d 小窗路由 & 组件共用 | 无 | 前端路由 query param + 同一套 ChatPage |
| 4e 小窗 UI 美化 | 4d | CSS：白色板块透明 + 间距缩 1/3 + 整体样式对齐 |
| 4f 小窗支持：模型切换 + 思考强度默认 standard + 全量 markdown | 4d | 同一套 Composer + Markdown，参数覆盖 |
| 4g 右键菜单加项目地址 + 自动省略 + hover 全名 + 去重 | 同 Phase 2h | Menus.tsx + CSS |
| 4h 附件按钮去掉 + 直接拖/复制粘贴（主+小窗共用一套） | 无 | Composer 加 onDrop/onPaste |
| 4i 小窗渲染解析同主窗口 + 公用后端 | 4d | 自动，因为共用组件 |
| 4j 小窗会话自动同步到主窗口 | 后端 session 共享 + 前端 store persist | 天然，无需额外逻辑 |
| 4k 修：插入文件后小窗无响应 | collectSessionTools 主窗口 env 问题 | 后端统一工具收集 |

**质量门**：`cargo check`（编译过） + 手动走查（Tauri 窗口难 vitest）

---

## Phase 5 · 调研 & Token 预算节约（最晚做，不阻塞前面所有）

| 子项 | 动作 | 产出 |
|---|---|---|
| 5a GitHub 调研 | WebSearch："LLM system prompt tool injection lazy loading"、"MCP tool schema on demand"、"prompt caching system prompt slicing" | 调研报告 |
| 5b 当前预算诊断 | 抓一次 chat_stream 请求的 system_prompt + tool_definitions + history 各占多少 tokens | 预算快照 |
| 5c 系统 prompt 与 MCP prompt 解耦设计 | 核心层 + 可选层 + tool 层分层；大模型按需调用 | 架构图 |
| 5d 实施 | failover.rs 主循环改造（tool_use detect → 按需注入 tool_definitions → 重发） | 代码 |

**质量门**：跑一轮完整对话验证不丢 tool_call 能力 + token 实测减少

---

## 执行顺序图（简化版）

```
Phase 1 [安全bug]  ← 立即可做，无确认点
  │
  ▼
Phase 2 [体验bug+布局]  ← 可立即开始
  │
  ▼
Phase 3 [清理+删除+小功能]  ← 3d/3e 需你确认删除边界
  │
  ▼
Phase 4 [小窗popout]  ← 从零重写，时间最长
  │
  ▼
Phase 5 [预算节约调研]  ← 最晚做，不阻塞
```

## 估算改动规模

| Phase | 前端文件数 | 后端文件数 | 预估代码行 |
|---|---|---|---|
| 1 | 5 | 1 | ~250 |
| 2 | 10 | 0 | ~300 |
| 3 | 12 | 4 | ~400 |
| 4 | 8 | 3 | ~800 |
| 5 | 3 | 2 | ~300 + 报告 |

**总计 ~2050 行改动，不改现有架构，全部最小补丁。**

---

## 本轮我停下来不动代码等你拍板的点（只剩 2 个）

1. **Phase 3d/3e 删除边界**：AI味检测 + 翻译的 **内置 agent 注册 + prompt md 文件** 也删？还是只删独立路由页面？
2. **Phase 5 启动时机**：是 Phase 1-4 全做完再开始调研？还是和 Phase 2/3 **并行**（调研不碰代码，可以后台做）？

你回这两个点，我立刻 Phase 1 动手。