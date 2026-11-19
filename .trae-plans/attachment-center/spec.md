# 会话级文档长期可用 + 附件中心改造（Spec）

## 1. 背景

现状问题：

* 每轮请求只取最近 `max_context_messages`（默认 24）条消息，附件注入只在窗口内生效；滑窗滚过后模型对早期文档“失忆”。
* 附件中心只能看到“消息里的附件”，没有会话级的文档清单与解析信息（块数/摘要/doc\_id）。
* 附件中心的「插入当前会话」会重新读盘解析（`extract_and_index`），已解析过的内容被重复计算。
* 附件中心「插入」是单击即执行，不符合“未确认不得执行”的要求。

## 2. 目标

1. **常驻文档清单**：每轮 system 末尾注入本会话已解析文档清单（doc manifest），不随滑窗丢失。
2. **会话级查询**：提供会话级文档/附件查询命令，返回名称、类型、块数、字符数、摘要、doc\_id、时间。
3. **导入复用解析**：从附件中心导入已解析附件时，复制源会话 `doc_chunks`（含 FTS），不重新提取原文件；仅当源无解析结果时回退重解析。
4. **双来源入口**：Composer 附件按钮提供「从电脑上传」「从附件中心导入」。
5. **先确认后执行**：所有导入/插入动作必须经应用内确认框（`uiConfirm`）确认；按钮有 busy 态防重复提交。
6. **UI 一致**：复用现有 `.dialog-mask/.dialog`、`.tool-chip`、`.att-card` 等样式与 token，不新增色值。

## 3. 范围

做：

* Rust：`doc::store::copy_document`、`list_session_docs`、`import_attachment_from_center`、`doc_list` 工具。
* 前端：IPC 封装、doc manifest 注入、Composer 双来源菜单、附件中心选择器弹窗、附件中心插入确认、卡片解析信息展示。

不做：

* 不做 P1 三档注入优先级重构（pinned 全文预算）与 P2 多 query 预检索增强。
* 不修改内存记忆（memory.md）链路。
* 不新增数据库迁移（复用现有 `doc_chunks` / `doc_chunks_fts`）。

## 4. 后端设计

### 4.1 `src-tauri/src/doc/store.rs`

新增：

```rust
/// 跨会话复制已解析文档块（附件中心导入复用解析，不重读原文件）。返回复制块数。
pub fn copy_document(
    conn: &Connection,
    from_session: &str,
    to_session: &str,
    doc_id: &str,
) -> Result<usize, String>
```

行为：

1. 取源会话该 doc 的 `doc_name`、`kind`（`SELECT doc_name, kind FROM doc_chunks WHERE session_id=?1 AND doc_id=?2 LIMIT 1`），无行 → `Err("源会话缺少该文档的解析块")`。
2. 事务内先清目标会话同名/同 doc\_id 旧块及其 FTS（与 `index_document` 同口径）：
   * `DELETE FROM doc_chunks_fts WHERE rowid IN (SELECT id FROM doc_chunks WHERE session_id=?1 AND (doc_id=?2 OR doc_name=?3))`
   * `DELETE FROM doc_chunks WHERE session_id=?1 AND (doc_id=?2 OR doc_name=?3)`
3. `SELECT page, section, ord, content, created FROM doc_chunks WHERE session_id=?1 AND doc_id=?2 ORDER BY ord`，逐块以 `to_session` 写入，`last_insert_rowid()` 同步写 `doc_chunks_fts(section || ' ' || content)`。
4. 提交，返回块数。

单元测试（`store.rs` 内 `#[cfg(test)]`，复用现有 `setup` 风格）：

* `copy_document_copies_chunks_and_fts`：源会话 2 块 → 目标会话 2 块，目标 `search` 可命中。
* `copy_document_replaces_same_name_in_target`：目标已有同名文档 → 复制后只剩新块。
* `copy_document_missing_source_errors`。

### 4.2 `src-tauri/src/doc/mod.rs`

`pub use store::{copy_document, DocInfo, index_document, list_docs, outline};`

### 4.3 `src-tauri/src/commands/attachment/center.rs`

新增类型与命令：

```rust
#[derive(Debug, Clone, Serialize)]
pub struct SessionDoc {
    pub doc_id: String,
    pub doc_name: String,
    pub kind: String,
    pub chunks: i64,
    pub chars: i64,
    pub summary: Option<String>,
    pub created: String,
}

pub(crate) fn list_session_docs_on(conn: &Connection, session_id: &str) -> Result<Vec<SessionDoc>, String>;

#[tauri::command]
pub async fn list_session_docs(session_id: String) -> Result<Vec<SessionDoc>, String>;

#[tauri::command]
pub async fn import_attachment_from_center(
    target_session_id: String,
    source_message_id: i64,
    path: String,
) -> Result<AttachmentMeta, String>;
```

`list_session_docs_on`：

* `SELECT doc_id, doc_name, kind, COUNT(*), COALESCE(SUM(length(content)),0), MIN(created) FROM doc_chunks WHERE session_id=?1 GROUP BY doc_id, doc_name, kind ORDER BY MIN(id)`。
* 摘要：一次扫描该会话 `messages.attachments`（`doc_id → summary`，取首个非空），按 doc\_id 回填。
* 测试：`list_session_docs_returns_parse_info`（块数/字符数/摘要）、`list_session_docs_empty`。

`import_attachment_from_center` 步骤：

1. `crate::memory::validate_id(&target_session_id)`；`ensure_in_attachments_dir(path)`；`std::fs::metadata` 存在且 ≤ `MAX_ATTACHMENT_BYTES`。
2. 读源消息并定位 meta：新增 helper

```rust
pub(crate) fn find_meta_on(
    conn: &Connection, message_id: i64, path: &str,
) -> Result<(String /* source_session */, AttachmentMeta), String>
```

消息不存在 / 该消息无此 path → `Err`。
3\. 配额：`attachment_quota::check_quota_on(&conn, &target_session_id, Some(&path_str), size)`。
4. `target_session_id == source_session` → 直接返回源 meta（不复制）。
5. 若 `meta.doc_id` 存在且源会话有对应块：

* `let n = crate::doc::copy_document(&conn, &source_session, &target_session_id, doc_id)?`
* 返回 `AttachmentMeta { path, name: meta.name, kind: meta.kind, size, extracted_text: meta.extracted_text, chunk_count: Some(n), summary: meta.summary, doc_id: meta.doc_id, extract_note: None }`
* **不读盘、不重解析**。

6. 否则回退：`extract_and_index(&target_session_id, &meta.name, attachment::classify(&meta.name), &path_str)` 组装 meta（保持旧行为）。

测试：`import_reuses_chunks_without_reparse`（断言目标块数 > 0、返回 `chunk_count == n`、源 `extracted_text` 被复用）、`import_falls_back_when_unparsed`、`import_rejects_missing_message`、`import_respects_quota`。

### 4.4 命令注册

`src-tauri/src/commands/attachment/mod.rs`：

```rust
pub use center::{
    AttachmentEntry, delete_attachment, import_attachment_from_center,
    insert_attachment_to_session, list_all_attachments, list_session_docs, SessionDoc,
};
```

`src-tauri/src/commands/mod.rs` 的 `all_commands!` 增加：

* `crate::commands::attachment::center::list_session_docs`
* `crate::commands::attachment::center::import_attachment_from_center`

### 4.5 `doc_list` 内置工具

* `src-tauri/src/mcp/tool_spec/readonly.rs`：`doc_read` 之后新增

```rust
tool("doc_list", "列出本会话已解析的文档（名称 / doc_id / 块数 / 字符数）；细节用 doc_search 检索、doc_read 回看。", json!({ "type": "object", "properties": {} })),
```

* `src-tauri/src/mcp/builtin/docs.rs`：`handles` 增加 `"doc_list"`；`exec` 分发到 `doc_list_exec`。
* `doc_list_exec`：锁库 → `crate::doc::list_docs(&conn, sid)` → 逐行 `- 《name》(kind) · N 块 · 约 X 字符 · doc_id=…`；空 → `"本会话还没有已解析的文档"`；≤50 份。

## 5. 前端设计

### 5.1 IPC（`src/lib/ipc/attachment.ts`，已由 `index.ts` 全量导出）

```ts
export interface SessionDoc {
  doc_id: string; doc_name: string; kind: string;
  chunks: number; chars: number; summary: string | null; created: string;
}
export const listSessionDocs = (sessionId: string) =>
  invoke<SessionDoc[]>('list_session_docs', { sessionId });
export const importAttachmentFromCenter = (
  targetSessionId: string, sourceMessageId: number, path: string,
) => invoke<AttachmentMeta>('import_attachment_from_center', {
  targetSessionId, sourceMessageId, path,
});
```

### 5.2 文档清单注入

`src/pages/chat/attachments.ts` 新增：

```ts
export const DOC_MANIFEST_MAX = 8;
export function formatDocManifest(docs: SessionDoc[]): string
```

格式（空数组返回 `''`）：

```
【本会话文档 · 长期可用】以下文档已解析并驻留，细节请用 doc_search 检索、doc_read 回看；未检索到的内容不要臆测。
- 《name》 doc_id=xxx · 12 块 · 摘要：……
…另有 3 份未列出，可用 doc_list 查看完整清单。
```

摘要按 160 字符截断。

`src/pages/chat/useChatSend.ts`：

* 新增 `const [docManifest, setDocManifest] = useState('')`。
* `useEffect` 依赖 `[sessionId, messages.length, pendingAttachments.length]`：`ipc.listSessionDocs(sessionId).then(d => setDocManifest(formatDocManifest(d)))`，失败置 `''`，卸载后不再 setState。
* `buildSystemWith` 返回值改为 `docManifest ? `${sys}\n\n${docManifest}` : sys`，并把 `docManifest` 加入依赖。

### 5.3 Composer 双来源

`src/components/composer/ToolBar.tsx`：

* 新 prop `onOpenAttachmentCenter?: () => void`；新增 `srcMenu` 状态。
* 「+」按钮：未提供 `onOpenAttachmentCenter` 时保持原行为（直接打开文件选择）；提供时改为打开浮层 `.attach-src-menu`，含两项：
  * 「从电脑上传」→ 触发隐藏 `input[type=file]`
  * 「从附件中心导入」→ `onOpenAttachmentCenter()` 并关闭浮层
* 浮层用 `.attach-src-backdrop` 点击关闭，Esc 关闭。

`src/components/composer/Composer.tsx`：新增 `onOpenAttachmentCenter` prop 并透传。

`src/pages/chat/ChatPage.tsx`：

* `const [centerPickerOpen, setCenterPickerOpen] = useState(false)`。
* `<Composer onOpenAttachmentCenter={() => setCenterPickerOpen(true)} />`
* 渲染 `AttachmentCenterPicker`，`onImported` 中：追加 metas；对 `kind === 'image'` 的项 `ipc.readAttachmentData(path)` 补 `pendingImages`。

### 5.4 新增 `src/components/composer/AttachmentCenterPicker.tsx`

Props：

```ts
{ sessionId: string; onClose: () => void;
  onImported: (metas: AttachmentMeta[]) => void;
  onError: (msg: string) => void }
```

* 打开时 `listAllAttachments()`；按 `meta.path` 去重（保留最新 message\_id），过滤掉 `kind === 'other'`。
* 列表项展示：名称、来源会话标题、类型、大小、`N 块`（`chunk_count`）、摘要首行；`extract_note` 非空时显示「未解析」并禁用勾选。
* 支持关键词搜索；勾选多选；底部「取消」「导入选中(N)」。
* 「导入选中」点击后：若 `busy` 直接 return → `await uiConfirm({ title:'确认导入附件', message:'将把以下 N 个附件导入当前会话：\n…\n\n已解析文档会复用现有解析结果，不重新解析。', confirmText:'导入' })` → 为 true 才逐个调用 `importAttachmentFromCenter`（顺序执行，`busy` 期间按钮禁用）。
* 失败项收集原因，通过 `onError` 提示；全部成功 → `onImported(metas)` + `onClose()`。

### 5.5 附件中心插入确认

`src/pages/AttachmentsPage.tsx` 的 `onInsert`：

* 新增 `inserting: string | null` 状态。
* 先 `uiConfirm({ title:'插入附件', message:'将把《name》插入会话「title」，复用已解析内容（不重新解析）。是否继续？', confirmText:'插入' })`；取消直接 return。
* 确认后 `setInserting(key)`，调用 `insertAttachmentToSession`，`finally` 清空；期间同一卡片按钮禁用。

### 5.6 附件卡片解析信息

`src/pages/attachments/AttachmentCard.tsx`：

* meta 行追加 `· {chunk_count} 块`（`chunk_count > 0` 时）。
* 名称 `title` 追加摘要（有 `summary` 时）。
* 新增 `inserting?: boolean` prop，插入按钮 `disabled={inserting}`。

### 5.7 样式（`src/styles/tools/attachment.css`）

> 实现偏差：`composer.css` 已接近 700 行守卫，`.attach-src-*` 与 `.att-pick-*` 一并落在 `styles/tools/attachment.css`（`tools.css` 在 `chat.css` 之后加载，可正常覆盖 `.pin-menu` 定位）。

* `.attach-src-backdrop` / `.attach-src-menu` / `.attach-src-item`：复用 `.pin-menu` / `.pin-backdrop` 的 token（`--bg-2`、`--line`、`--radius-m`、`--shadow-2`、`--text-1`）。
* `.att-pick-*`：选择器列表、勾选态、摘要弱化色（`--text-2`）。
* 不新增色值；间距用 `var(--space-*)`。

## 6. 确认与防重复（硬约束）

* 任何涉及“导入/插入/覆盖”的操作，必须先 `await uiConfirm(...)`，返回 `true` 才执行。
* 执行中的按钮必须 `disabled`，并用 busy 标志在入口 `if (busy) return` 防重复提交。
* 不使用 `window.confirm`。

## 7. 验证

质量门：

1. `npx tsc -b`
2. `npx vitest run`
3. `cd src-tauri; cargo test --no-default-features --lib`

手工验证：

1. 会话 A 上传 PDF → 新开会话 A 追问早期内容 → 系统提示含【本会话文档】清单且回答能引用。
2. 附件中心按会话分组可见「N 块」；点「插入当前会话」先弹确认，取消则不执行。
3. Composer「+」→「从附件中心导入」→ 勾选 → 「导入选中」→ 弹确认 → 确认后附件出现在待发区；重复点击不产生重复导入。
4. 导入已解析文档后，目标会话 `doc_search` 能命中；未触发重新读盘解析（日志无提取动作）。
5. 附件中心删除仍保持原有二次点击确认逻辑不变。

## 8. 风险与回退

* 复制块时目标会话已有同名文档会被替换（与重新上传口径一致），确认文案已提示“复用已解析内容”。
* 文件仍共享同一磁盘路径；删除逻辑按全库引用计数，不会误删其他会话引用。
* 若后端命令异常，前端仅提示错误，不改变现有附件区内容。
