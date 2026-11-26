# 实施任务清单（方案 C）

## 后端

- [x] B1 新增 `src-tauri/src/commands/provider/chat/run.rs`
  - `RunStatus` / `SeqChunk` / `RunEntry`（seq 自增、4096 环形缓冲、dropped_through、终态幂等）/ `RunSink`（emit `chat:run`、`chat:run_end`）
  - `ChatRunRegistry`（start 校验 run_id 唯一 + 单会话互斥、15 分钟惰性清理）
  - 命令 `chat_run_state(run_id, after_seq)`
- [x] B2 `chat/command.rs`：`chat_stream` → `chat_start_run`（沿用组装逻辑；`runs.start` + preparing Phase + `cancel_state.register(run_id)` + spawn；返回 run_id）
- [x] B3 `chat/mod.rs`：`pub(crate) mod run;`；`ChatStreamInput.cancel_id` → `run_id`；导出 `chat_start_run`
- [x] B4 `failover/mod.rs`：`Channel<StreamChunk>` → `RunSink`（含 panic 副本、consume_stream/dispatch 调用、runner::launch）
- [x] B5 `failover/runner.rs`：终态收口（Cancelled / Done 兜底 / panic → Error chunk）
- [x] B6 `failover/stream.rs`：参数换 `&RunSink`
- [x] B7 `mcp/dispatcher.rs`：参数换 `&RunSink`，6 处 send 改名（行数不增）
- [x] B8 `commands/mod.rs` 注册 `chat_start_run` + `chat_run_state`；`commands/provider/mod.rs` 导出；`lib.rs` manage `ChatRunRegistry`

## 前端

- [x] F1 `src/lib/ipc/provider.ts`：删 `chatStream`；新增 `startChatRun` / `chatRunState` / `onChatRun` / `onChatRunEnd`、`ChatRunState` 类型；`ChatStreamInput.run_id`
- [x] F2 新增 `src/stores/chatRuns.ts`（prepare / begin / finish / setError / isBusy）
- [x] F3 新增 `src/pages/chat/chatRuntime.ts`（全局监听、事件去重、runChatStream、cancelChatRun、resumeChatRun、finalize 落库）
- [x] F4 `useChatStream.ts` 改薄适配层（保留 `RunChatStream` 类型）
- [x] F5 `useChatSend.ts`：sending/error 走 store；prepare；onCancel → runtime；准备失败 finish
- [x] F6 `useChatSessionEffects.ts`：卸载时 busy 跳过 `removeIfEmpty`
- [x] F7 `ChatPage.tsx`：进入会话调用 `resumeChatRun`
- [x] F8 `useMessageActions.ts`：`runStream(...).catch(() => {})`

## 验证

- [x] V1 `node node_modules/typescript/bin/tsc -b`
- [x] V2 `node node_modules/vitest/vitest.mjs run`（251 个用例全过，含体积守卫）
- [x] V3 `cargo test --manifest-path src-tauri/Cargo.toml --lib`（651 passed）
- [x] V4 自动化核验：事件广播→序号去重→落库链路类型/单测通过；实机切换会话、跨页停止、刷新恢复需在运行中的应用里点一次确认
- [x] V5 重新打包（2.10.1）：`src-tauri/target/release/bundle/nsis/myAgent_2.10.1_x64-setup.exe`、`.../msi/myAgent_2.10.1_x64_en-US.msi`
- [x] V6 极端场景回归：新增 `src/pages/chat/chatRuntime.test.ts`（8 例：乱序/重复 seq、工具收集期停止、启动失败、同会话并发、报错 partial、取消 partial、刷新重放交织、run 过期）与 `run.rs` 5 个注册表单测；顺带修掉重放竞态、准备期停止失效、落库前清元信息、连点发送四个问题
