//! 会话生成的模块级 runtime:生成的生命周期不随页面组件挂载/卸载而变化。
//!
//! 后端把每个 chunk 以 `chat:run { run_id, seq, chunk }` 广播,结束时发
//! `chat:run_end { run_id, status }`;这里全局只监听一次,按 seq 去重后写入
//! messages store,结束时统一落库。页面重挂只需继续读 store;
//! 整页刷新则凭 sessionStorage 里的 run 元信息 + `chat_run_state` 重放。

import type { AgentTemplate, ReasoningCapability, SourceItem, StreamChunk } from '../../lib/ipc';
import * as ipc from '../../lib/ipc';
import { useMessagesStore } from '../../stores/messages';
import { useDeliverablesStore } from '../../stores/deliverables';
import { useChatRunsStore } from '../../stores/chatRuns';
import { runningTask, useTasksStore } from '../../stores/tasks';
import type { ThinkingSelection } from '../../stores/thinking';
import { maybeAutoSummarize } from './memoryContext';
import { collectSessionTools, resolveReasoningEffort } from './chatTools';
import { createStreamChunks } from './chatStream';

export interface RunChatStreamParams {
  sessionId: string;
  providerId: string;
  model: string;
  history: { id?: number; role: string; content: string; images?: string[] }[];
  sources?: SourceItem[] | null;
  agent: AgentTemplate | null;
  thinkCap: ReasoningCapability | null;
  thinkingSel: ThinkingSelection | undefined;
}

interface ActiveRun {
  runId: string;
  sessionId: string;
  /** messages store 的流式 token(旧流收尾不覆盖新流) */
  token: number;
  chunks: ReturnType<typeof createStreamChunks>;
  /** 已处理到的最大 seq:事件与重放重叠时去重 */
  afterSeq: number;
  interrupted: boolean;
  error: string | null;
  model: string;
  providerId: string;
  agent: AgentTemplate | null;
  sourcesJson: string | null;
  /** 重放中:实时 chunk 先入 buffered,避免抢在快照之前把 seq 抬高造成丢段 */
  resuming: boolean;
  buffered: { seq: number; chunk: StreamChunk }[];
  /** 重放期间收到的终态,等快照应用完再收尾 */
  endStatus: ipc.RunStatus | null;
}

/** 刷新恢复用的元信息(同一 webview 的 sessionStorage,不属于后端契约)。 */
interface RunMeta {
  runId: string;
  model: string;
  providerId: string;
  sourcesJson: string | null;
}

const runs = new Map<string, ActiveRun>();
const runBySession = new Map<string, string>();
/** run 尚未登记就被「停止」的会话(工具收集阶段) */
const pendingCancels = new Set<string>();
const META_PREFIX = 'maf-run:';

let listenersReady = false;

function ensureListeners() {
  if (listenersReady) return;
  listenersReady = true;
  void ipc
    .onChatRun((p) => handleChunk(p.run_id, p.seq, p.chunk))
    .catch(() => {
      listenersReady = false;
    });
  void ipc
    .onChatRunEnd((p) => {
      const run = runs.get(p.run_id);
      if (!run) return;
      // 重放未完成:先记下终态,等快照按序补齐后再落库(否则会丢快照里的前半段)
      if (run.resuming) {
        run.endStatus = p.status;
        return;
      }
      void finalize(run, toMode(p.status));
    })
    .catch(() => {
      listenersReady = false;
    });
}

function toMode(status: ipc.RunStatus): 'done' | 'error' | 'cancelled' {
  return status === 'done' ? 'done' : status === 'error' ? 'error' : 'cancelled';
}

/** 按 seq 单调应用一个 chunk(事件与重放共用)。 */
function applyChunk(run: ActiveRun, seq: number, chunk: StreamChunk) {
  if (seq <= run.afterSeq) return;
  run.afterSeq = seq;
  run.chunks.onChunk(chunk);
  if (chunk.type === 'error') run.error = chunk.message;
}

function handleChunk(runId: string, seq: number, chunk: StreamChunk) {
  const run = runs.get(runId);
  if (!run || seq <= run.afterSeq) return;
  if (run.resuming) {
    run.buffered.push({ seq, chunk });
    return;
  }
  applyChunk(run, seq, chunk);
}

function streamHandlers(token: number) {
  const ms = () => useMessagesStore.getState();
  return {
    pushStreamDelta: (sid: string, delta: string) => ms().pushStreamDelta(sid, delta, token),
    pushStreamReasoning: (sid: string, delta: string) => ms().pushStreamReasoning(sid, delta, token),
    setStreamReasoningTokens: (sid: string, tokens: number | null) =>
      ms().setStreamReasoningTokens(sid, tokens),
    setStreamPhase: (sid: string, phase: string | null) => ms().setStreamPhase(sid, phase),
  };
}

function taskIdOf(sessionId: string) {
  // 工具块打上当前进行中任务 id(val.md §5.4)
  return () => runningTask(useTasksStore.getState().bySession[sessionId])?.id ?? null;
}

function cleanup(run: ActiveRun) {
  runs.delete(run.runId);
  if (runBySession.get(run.sessionId) === run.runId) runBySession.delete(run.sessionId);
}

/** 只在落库成功后清掉落盘元信息:否则「生成结束→落库中刷新页面」会丢最后一段回复。 */
function clearMeta(sessionId: string) {
  try {
    sessionStorage.removeItem(META_PREFIX + sessionId);
  } catch {
    /* 存储不可用则忽略 */
  }
}

function writeMeta(sessionId: string, meta: RunMeta) {
  try {
    sessionStorage.setItem(META_PREFIX + sessionId, JSON.stringify(meta));
  } catch {
    /* 存不下就不支持刷新恢复 */
  }
}

function readMeta(sessionId: string): RunMeta | null {
  try {
    const raw = sessionStorage.getItem(META_PREFIX + sessionId);
    return raw ? (JSON.parse(raw) as RunMeta) : null;
  } catch {
    return null;
  }
}

/** 收尾:取完整缓冲落库、刷新产物、释放会话忙碌态。 */
async function finalize(run: ActiveRun, mode: 'done' | 'error' | 'cancelled') {
  if (runs.get(run.runId) !== run) return;
  cleanup(run);
  const { sessionId } = run;
  const ms = useMessagesStore.getState();
  const { content, reasoning } = ms.endStreaming(sessionId, run.token);
  const partial = mode !== 'done' || run.interrupted;
  let err = mode === 'error' ? run.error || '生成失败,请重试' : null;
  /** 是否已成功落库(含「本来就没有正文」):失败时保留元信息,下次进入会话可重放补写 */
  let saved = true;
  if (content.trim()) {
    saved = false;
    try {
      await ms.appendAssistant(
        sessionId, content, run.model, run.chunks.getTokens(),
        run.sourcesJson, reasoning, partial,
      );
      saved = true;
    } catch (e) {
      err = `回复保存失败:${e instanceof Error ? e.message : String(e)}`;
    }
  }
  if (mode === 'done' && !run.interrupted) {
    maybeAutoSummarize(sessionId, run.providerId, run.model, run.agent?.memory_write ?? true);
  }
  useChatRunsStore.getState().finish(sessionId, err);
  if (saved) clearMeta(sessionId);
  // M2:助手消息落库后刷新产物(本轮 present 登记的交付物已绑定到该消息)
  void useDeliverablesStore.getState().load(sessionId, true);
}

/** 未真正发出请求就收尾(工具收集期间被停止 / run 已过期)。 */
function abandon(run: ActiveRun) {
  cleanup(run);
  clearMeta(run.sessionId);
  useMessagesStore.getState().endStreaming(run.sessionId, run.token);
  useChatRunsStore.getState().finish(run.sessionId);
}

export function hasActiveChatRun(sessionId: string): boolean {
  return runBySession.has(sessionId);
}

/** 停止该会话的生成:已登记则通知后端;仍在准备阶段则记下标记。 */
export function cancelChatRun(sessionId: string) {
  ensureListeners();
  const runId = runBySession.get(sessionId);
  if (!runId) {
    pendingCancels.add(sessionId);
    return;
  }
  const run = runs.get(runId);
  if (run) run.interrupted = true;
  void ipc.cancelStream(runId).catch(() => {
    /* 送达失败也由前端收流 */
  });
}

/** 启动一次生成:准备工具 → 调后端登记 run;chunk 由全局事件驱动。 */
export async function runChatStream(params: RunChatStreamParams): Promise<void> {
  ensureListeners();
  const { sessionId } = params;
  if (runBySession.has(sessionId)) {
    // 抛出以便调用方回滚乐观用户消息并回填输入框
    throw new Error('当前会话正在生成回复,请稍候');
  }
  const runId = crypto.randomUUID();
  // 清掉上一轮准备阶段的残留取消标记,避免误伤本次发送
  pendingCancels.delete(sessionId);
  useChatRunsStore.getState().begin(sessionId, runId);
  const token = useMessagesStore.getState().startStreaming(sessionId);
  const sourcesJson =
    params.sources && params.sources.length > 0 ? JSON.stringify(params.sources) : null;
  const run: ActiveRun = {
    runId,
    sessionId,
    token,
    chunks: createStreamChunks(sessionId, streamHandlers(token), taskIdOf(sessionId)),
    afterSeq: 0,
    interrupted: false,
    error: null,
    model: params.model,
    providerId: params.providerId,
    agent: params.agent,
    sourcesJson,
    resuming: false,
    buffered: [],
    endStatus: null,
  };
  runs.set(runId, run);
  runBySession.set(sessionId, runId);
  writeMeta(sessionId, { runId, model: params.model, providerId: params.providerId, sourcesJson });

  try {
    const tools = await collectSessionTools(params.agent, sessionId);
    // 工具收集期间用户点了停止:不发起请求(interrupted 覆盖「后端尚未登记」的情况)
    if (pendingCancels.delete(sessionId) || run.interrupted || !runs.has(runId)) {
      abandon(run);
      return;
    }
    await ipc.startChatRun({
      run_id: runId,
      provider_id: params.providerId,
      model: params.model,
      messages: params.history,
      temperature: params.agent?.temperature ?? null,
      max_tokens: null,
      top_p: null,
      custom_base_url: null,
      custom_display_name: null,
      session_id: sessionId,
      reasoning_effort: resolveReasoningEffort(params.thinkCap, params.thinkingSel),
      tools,
    });
  } catch (e) {
    if (runs.get(runId) === run) abandon(run);
    throw e;
  }
}

/** 页面刷新后恢复进行中的 run:重放已有 chunk,终态则立即落库。 */
export async function resumeChatRun(sessionId: string): Promise<void> {
  ensureListeners();
  if (runBySession.has(sessionId)) return;
  const meta = readMeta(sessionId);
  if (!meta) return;
  const token = useMessagesStore.getState().startStreaming(sessionId);
  const run: ActiveRun = {
    runId: meta.runId,
    sessionId,
    token,
    chunks: createStreamChunks(sessionId, streamHandlers(token), taskIdOf(sessionId)),
    afterSeq: 0,
    interrupted: false,
    error: null,
    model: meta.model,
    providerId: meta.providerId,
    agent: null,
    sourcesJson: meta.sourcesJson,
    resuming: true,
    buffered: [],
    endStatus: null,
  };
  runs.set(meta.runId, run);
  runBySession.set(sessionId, meta.runId);
  useChatRunsStore.getState().begin(sessionId, meta.runId);
  try {
    const state = await ipc.chatRunState(meta.runId, 0);
    // 先补齐落库前的历史 chunk,再按序应用重放期间到达的实时 chunk
    for (const item of state.chunks) applyChunk(run, item.seq, item.chunk);
    run.resuming = false;
    for (const item of run.buffered.sort((a, b) => a.seq - b.seq)) {
      applyChunk(run, item.seq, item.chunk);
    }
    run.buffered = [];
    // 快照与重放期间收到的终态取其一(快照可能落后于实时事件)
    const status = state.status !== 'running' ? state.status : run.endStatus;
    if (status) await finalize(run, toMode(status));
  } catch {
    // run 已过期/不存在:清掉残留占位与元信息
    if (runs.get(meta.runId) === run) abandon(run);
  }
}
