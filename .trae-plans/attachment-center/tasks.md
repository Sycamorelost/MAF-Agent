# 任务清单（Attachment Center）

> 依据 `spec.md` 执行。全部自动化质量门已通过。

## T1 后端：跨会话复制解析块
- [x] `src-tauri/src/doc/store.rs` 新增 `copy_document(from_session, to_session, doc_id) -> Result<usize, String>`
- [x] 事务内清目标会话同 doc_id/同 doc_name 的块与 FTS，再逐块复制并重建 FTS
- [x] `src-tauri/src/doc/mod.rs` 导出 `copy_document`
- [x] 单测：复制成功/FTS 可检索、目标同名校验、源缺失报错

## T2 后端：会话级文档清单与导入命令
- [x] `commands/attachment/center.rs` 新增 `SessionDoc` + `list_session_docs_on` + `list_session_docs`
- [x] 新增 `find_meta_on`（消息 → 源会话 + AttachmentMeta）与 `reuse_parsed_on`（复用/回退判定）
- [x] 新增 `import_attachment_from_center`（校验 → 配额 → 同会话短路 → 复用 copy_document → 回退重解析）
- [x] `commands/attachment/mod.rs` 重导出；`commands/mod.rs` 注册两条命令
- [x] 单测：解析信息聚合、复用复制、未解析回退、消息缺失

## T3 后端：doc_list 工具
- [x] `mcp/tool_spec/readonly.rs` 增加 `doc_list` 定义
- [x] `mcp/builtin/docs.rs` 增加 handles/exec/`doc_list_exec`

## T4 前端：IPC 与文档清单注入
- [x] `src/lib/ipc/attachment.ts` 增加 `SessionDoc`、`listSessionDocs`、`importAttachmentFromCenter`
- [x] `src/pages/chat/attachments.ts` 增加 `DOC_MANIFEST_MAX`、`formatDocManifest`
- [x] `src/pages/chat/useChatSend.ts` 拉取清单并追加到 system 末尾（依赖 `sessionId/messages.length/pendingAttachments.length`）

## T5 前端：Composer 双来源入口
- [x] `composer/ToolBar.tsx` 增加 `onOpenAttachmentCenter` 与来源浮层（电脑上传 / 附件中心）
- [x] `composer/Composer.tsx` 透传 prop
- [x] `ChatPage.tsx` 挂载选择器，导入后处理图片 data URL
- [x] 样式：`styles/tools/attachment.css` 增加 `.attach-src-*`（见 spec §5.7 偏差说明）

## T6 前端：附件中心选择器（确认后导入）
- [x] 新建 `src/components/composer/AttachmentCenterPicker.tsx`（列表/搜索/多选/块数与摘要/未解析禁用）
- [x] 点击「导入选中」→ `uiConfirm` 确认 → 顺序导入；`busy` 防重复
- [x] 样式：`styles/tools/attachment.css` 增加 `.att-pick-*`

## T7 前端：附件中心插入确认与解析信息
- [x] `AttachmentsPage.tsx` `onInsert` 改为 `uiConfirm` 后执行，新增 `inserting` 防重复
- [x] `AttachmentCard.tsx` 显示 `N 块`、摘要 tooltip、插入按钮 disabled
- [x] 现有 `attachments.ui.test.tsx` 12 项断言全部通过（新 prop 可选，无需改动）

## T8 质量门
- [x] `npx tsc -b`（通过）
- [x] `npx vitest run`（32 文件 / 253 用例通过）
- [x] `cd src-tauri; cargo test --no-default-features --lib`（483 通过 / 0 失败）

## 人工回归建议（需真实上传/图形界面，不属于自动化门）
- 上传 PDF 后继续追问早期内容：system 末尾应出现【本会话文档 · 长期可用】。
- 附件中心卡片显示「N 块」；点插入先弹确认，取消不执行。
- Composer「+」→「从附件中心导入」→ 勾选 → 导入 → 二次确认后才进入待发区，重复点击不重复导入。
