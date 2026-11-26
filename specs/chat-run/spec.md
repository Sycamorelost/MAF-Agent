# 方案 C：对话生成下沉为后端 run 任务 + 全局事件 + 序号重放

## 1. 目标与验收

问题：切换会话/切到其它页面后，正在进行的生成被中断；无法跨页面继续查看/停止。

目标：生成生命周期与页面组件解耦。

验收标准：

1. 生成中切换会话、切到设置页/附件页，再回到原会话：回复继续增量出现，正文不丢、不重复。
2. 在任意页面可停止该会话正在进行的生成（Composer 停止、状态栏停止）。
3. 刷新页面（同一 webview）后，仍在进行的生成可恢复显示并继续；生成结束时照常落库。
4. 生成中离开会话不再触发「空会话删除」，不产生幽灵消息/丢消息。
5. 三套测试与文件体积守卫通过。

## 2. 现状（已核实）

- 后端 `chat_stream` 接收 `Channel<StreamChunk>`，`channel` 贯穿 `failover/mod.rs`、`runner.rs`、`stream.rs`、`mcp/dispatcher.rs`（`channel.send` 共约 30 处），无 UI 生命周期绑定；取消走 `cancel_stream`（`StreamCancelState`，oneshot + pending 兜底，注册 id 由前端生成）。
- 前端 `ipc.chatStream`（`src/lib/ipc/provider.ts` L422-465）自建 `Channel`，返回 `{done, cancel}`；流式与落库全部在 `useChatStream`/`useChatSend` 组件内完成。
- `useChatSessionEffects` 卸载时 `removeIfEmpty(sessionId)`（准备阶段 0 消息会删掉会话 = 丢消息隐患）。
- `StreamChunk` 定义于 `src-tauri/src/provider/mod.rs` L176-231（`Clone + Serialize`，tag=`type`）；`ChatStreamInput` 在 `chat/mod.rs`；`uuid` v4 已依赖。
- 文件体积守卫：`src/lib/fileSize.test.ts`，非基线文件 ≤400 行/24KB；`src/lib/ipc/provider.ts` 基线 477 行（只降不升）；`mcp/dispatcher.rs` 基线 402 行（只降不升）。

## 3. 后端设计

### 3.1 新增 `src-tauri/src/commands/provider/chat/run.rs`（约 170 行）

```rust
pub const EVENT_CHUNK: &str = "chat:run";
pub const EVENT_END: &str = "chat:run_end";
const MAX_BUFFERED: usize = 4096;                       // 环形缓冲上限
const KEEP_FINISHED: Duration = 15 分钟;                 // 终态保留供重放

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus { Running, Done, Error, Cancelled }

#[derive(Clone, Serialize)]
pub struct SeqChunk { seq: u64, chunk: StreamChunk }     // seq 从 1 单调递增

pub struct RunEntry { id, session_id, seq: AtomicU64, chunks: Mutex<VecDeque<SeqChunk>>,
                      dropped_through: AtomicU64, status: Mutex<RunStatus>,
                      finished_at: Mutex<Option<Instant>> }
```

- `RunEntry::push(chunk) -> u64`：入队 + 超过 4096 丢弃最旧并更新 `dropped_through`；Mutex 中毒用 `into_inner` 兜底，不 panic。
- `finish_once(status) -> bool`：仅 `Running → 终态` 生效一次，同时写 `finished_at`（幂等，防重复 `chat:run_end`）。

`RunSink { app: AppHandle, entry: Arc<RunEntry> }`（`Clone`）：

- `send(chunk) -> Result<(), String>`：`push` → `app.emit("chat:run", { run_id, seq, chunk })` → 若为 `Done`/`Error` 则 `finish(Done|Error)`；恒返回 `Ok`（保持既有 `is_err()` 判断不改逻辑）。
- `finish(status)`：`finish_once` 成功才 `emit("chat:run_end", { run_id, status })`；`finish_if_running(Done)` 供正常返回兜底。
- `run_id()` / `session_id()` 访问器。

`ChatRunRegistry`（`#[derive(Default)]`，Tauri State，`Mutex<HashMap<String, Arc<RunEntry>>>`）：

- `start(app, run_id, session_id) -> Result<RunSink, String>`：先惰性清理（终态超过 15 分钟移除）；`run_id` 重复 / 同会话已有 `Running` → `Err("该会话已有进行中的生成，请先停止当前生成")`。
- `get(run_id) -> Option<Arc<RunEntry>>`（含惰性清理）。

命令：

```rust
#[tauri::command]
pub fn chat_run_state(state: State<ChatRunRegistry>, run_id: String, after_seq: Option<u64>)
  -> Result<RunStatePayload, String>
// RunStatePayload { run_id, status, chunks: Vec<SeqChunk>, truncated: bool }
// truncated = after_seq < dropped_through（有被丢弃的更早 chunk）
```

### 3.2 改造 `chat/command.rs`

`chat_stream` → `chat_start_run`：

- 入参增加 `runs: State<ChatRunRegistry>`，去掉 `Channel<StreamChunk>`；返回 `Result<String, String>`（run_id）。
- 保留原有全部组装逻辑（项目模型覆盖 / resolve_spec / Key 校验 / reasoning 合并 / AGENTS.md 注入 / 项目 guard / 压缩与滑窗 / event_log / 定价归因）。
- 末尾改为：

```rust
let sink = runs.start(app.clone(), input.run_id.clone(), input.session_id.clone())?;
let _ = sink.send(StreamChunk::Phase { phase: "preparing".into() });
let (_, cancel_rx) = cancel_state.register(&input.run_id);
spawn_chat_with_failover(spec, req, sink, state.inner().clone(), cfg, attr,
                         cancel_state.inner().clone(), input.run_id.clone(), cancel_rx,
                         app, mcp_runtime.inner().clone(), approval_store.inner().clone());
Ok(input.run_id)
```

### 3.3 `ChatStreamInput`（`chat/mod.rs`）

`cancel_id: String` → `run_id: String`（由前端生成并提前注册）；其余不变。

### 3.4 failover 链路换类型

- `failover/mod.rs`：参数 `channel: Channel<StreamChunk>` → `sink: RunSink`；`panic_channel` → `panic_sink = sink.clone()`；所有 `channel.send` → `sink.send`；`consume_stream(..., &sink, ...)`、`dispatch_tool_calls(..., &sink, ...)`；`runner::launch(work, panic_sink, ...)`。
- `failover/stream.rs`：`channel: &Channel<StreamChunk>` → `sink: &RunSink`；`if sink.send(chunk).is_err() { break; }` 保留。
- `failover/runner.rs`：

```rust
match AssertUnwindSafe(run_cancellable(...)).catch_unwind().await {
    Ok(None)        => sink.finish(RunStatus::Cancelled),   // 用户停止，不伪造 Error chunk
    Ok(Some(()))    => sink.finish_if_running(RunStatus::Done),
    Err(_)          => { log::error!(...); let _ = sink.send(StreamChunk::Error { message: "对话流异常中断(内部错误),请重试".into() }); }
}
```

- `mcp/dispatcher.rs`：`channel: &Channel<StreamChunk>` → `sink: &RunSink`，6 处 `channel.send` → `sink.send`，去掉 `use tauri::ipc::Channel;`（换为 `use crate::commands::provider::chat::run::RunSink;`，行数不增）。

### 3.5 注册

- `chat/mod.rs`：`pub(crate) mod run;`、`pub use command::chat_start_run;`；`commands/provider/mod.rs` 同步导出。
- `commands/mod.rs`：`chat_stream` → `chat_start_run`，新增 `run::chat_run_state`；`cancel_stream` 保留（取消 id 即 run_id）。
- `lib.rs`：`.manage(commands::provider::chat::run::ChatRunRegistry::default())`。

## 4. 前端设计

### 4.1 `src/lib/ipc/provider.ts`

删除 `chatStream`；新增：

```ts
export type RunStatus = 'running' | 'done' | 'error' | 'cancelled';
export interface ChatRunState { run_id: string; status: RunStatus;
  chunks: { seq: number; chunk: StreamChunk }[]; truncated: boolean }
export const startChatRun = (input: ChatStreamInput) => invoke<string>('chat_start_run', { input });
export const chatRunState = (runId: string, afterSeq = 0) =>
  invoke<ChatRunState>('chat_run_state', { runId, afterSeq });
export const onChatRun = (cb) => listen<{run_id:string;seq:number;chunk:StreamChunk}>('chat:run', e => cb(e.payload));
export const onChatRunEnd = (cb) => listen<{run_id:string;status:RunStatus}>('chat:run_end', e => cb(e.payload));
```

`ChatStreamInput.cancel_id` → `run_id: string`；`cancelStream` 保留。

### 4.2 新增 `src/stores/chatRuns.ts`

```ts
interface SessionRun { runId: string | null; preparing: boolean; error: string | null }
bySession: Record<string, SessionRun | undefined>
prepare(sessionId) / begin(sessionId, runId) / finish(sessionId, error?) /
setError(sessionId, msg|null) / isBusy(sessionId)
```

`busy = preparing || runId !== null`；`finish` 不传 error 时保留原 error。

### 4.3 新增 `src/pages/chat/chatRuntime.ts`（模块级，约 200 行）

模块级状态：`runs: Map<runId, ActiveRun>`、`bySession: Map<sessionId, runId>`、`pendingCancels: Set<sessionId>`；`ensureListeners()` 惰性注册一次 `chat:run` / `chat:run_end`。

`ActiveRun { runId, sessionId, token, chunks, afterSeq, interrupted, error, model, providerId, agent, sourcesJson }`。

导出：

- `runChatStream(params): Promise<void>`
  1. 同会话已有 run → 报错返回；`crypto.randomUUID()` 作 runId。
  2. `useChatRunsStore.begin(sessionId, runId)`；`useMessagesStore.startStreaming(sessionId)` 取 token；建 `createStreamChunks`（含任务 id 归因，与原 `useChatStream` 一致）。
  3. `sessionStorage['maf-run:'+sessionId] = {runId, model, providerId, sourcesJson}`。
  4. `collectSessionTools`；若期间被 `pendingCancels` 取消 → 本地收尾（不落库）。
  5. `await ipc.startChatRun({...})`；失败 → 清理内存、`endStreaming`、`finish(sessionId)` 后**抛出**（供 `onSend` 回填输入框）。
  6. 成功即返回，后续由事件驱动。
- `cancelChatRun(sessionId)`：有 runId → 标记 `interrupted` + `ipc.cancelStream(runId)`；否则记入 `pendingCancels`。
- `isChatRunBusy(sessionId)`：内存是否有活跃 run。
- `resumeChatRun(sessionId)`：刷新恢复。读 sessionStorage → `chatRunState(runId, 0)` → 重新 `startStreaming` 并按 seq 重放 → `status !== 'running'` 则立即收尾落库；否则继续听实时事件。
- 内部 `handleChunk`：按 `seq <= afterSeq` 去重后喂 `onChunk`，记录 Error 文案。
- 内部 `finalize(run, mode)`：清 map/sessionStorage → `endStreaming` 取完整内容 → `appendAssistant(..., partial = mode !== 'done' || interrupted)`（正文非空才写）→ done 且未中断时 `maybeAutoSummarize` → `useChatRunsStore.finish(sessionId, error?)` → `useDeliverablesStore.load(sessionId, true)`。

### 4.4 `src/pages/chat/useChatStream.ts` 改为薄适配层

保留 `RunChatStream` 类型与 `useChatStream(params)` 签名（去掉 `setSending`/`cancelRef`），内部只做「模型兜底 + 调 `runChatStream`」，错误抛给调用方。

### 4.5 `src/pages/chat/useChatSend.ts`

- 删除 `sending` 局部 state，改为由 `useChatRunsStore` 派生；`error` / `setError` 也改走 store（保持 `setError: (msg: string|null) => void` 签名）。
- 发送时 `startStreaming(sessionId)` 后调 `useChatRunsStore.prepare(sessionId)`。
- `onCancel` 与 `CHAT_CANCEL_EVENT` 处理：`prepCancelRef.current = true; cancelChatRun(sessionId)`。
- 准备阶段失败：`useChatRunsStore.finish(sessionId)`（替代 `setSending(false)`）。

### 4.6 `src/pages/chat/useChatSessionEffects.ts`

卸载 cleanup 改为：

```ts
if (!isChatRunBusy(sessionId)) void useSessionsStore.getState().removeIfEmpty(sessionId);
```

### 4.7 `src/pages/chat/ChatPage.tsx`

首次进入会话时 `useEffect(() => { if (sessionId) void resumeChatRun(sessionId); }, [sessionId])`。

### 4.8 `src/pages/chat/useMessageActions.ts`

`await runStream(...)` 后追加 `.catch(() => {})`（错误已写入 store，避免未处理拒绝）。

## 5. 已知限制（需在交付说明中写明）

- run 属于发起它的 webview：主窗与小窗的 zustand store 不共享，另一窗口不会自动显示该 run（可后续按 `chat_run_state` 只读订阅）。
- 进程退出 = 生成结束；重启后不可恢复。
- 环形缓冲 4096 条，超出部分无法重放（`truncated=true`）。

## 6. 验证

1. `node node_modules/typescript/bin/tsc -b`
2. `node node_modules/vitest/vitest.mjs run`（含 `fileSize.test.ts`：provider.ts ≤477 行，dispatcher.rs ≤402 行，新文件 ≤400 行/24KB）
3. `cargo test --manifest-path src-tauri/Cargo.toml --lib`
4. 手工：长回复生成中切会话/切页面再回来（连续、不重复）；跨页面停止；刷新页面恢复；生成结束正常落库与产物刷新。
5. 版本保持 2.10.1；不结束用户正在运行的 dev 实例。

## 7. 变更文件清单

后端：
- 新增 `src-tauri/src/commands/provider/chat/run.rs`
- 改 `chat/command.rs`、`chat/mod.rs`、`chat/failover/{mod,runner,stream}.rs`、`mcp/dispatcher.rs`、`commands/mod.rs`、`commands/provider/mod.rs`、`lib.rs`

前端：
- 新增 `src/stores/chatRuns.ts`、`src/pages/chat/chatRuntime.ts`
- 改 `src/lib/ipc/provider.ts`、`src/pages/chat/{useChatStream,useChatSend,useChatSessionEffects,useMessageActions,ChatPage}.ts(x)`
