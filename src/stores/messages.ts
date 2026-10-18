import { create } from 'zustand';
import * as ipc from '../lib/ipc';

interface MessagesState {
  bySession: Record<string, ipc.MessageRow[]>;
  /** 正在流式输出的会话 -> 当前已累积的临时内容 */
  streaming: Record<string, string>;
  /** 正在流式输出的会话 -> 已累积的思考文本(v1.9.0;与正文分流,折叠块展示) */
  streamingReasoning: Record<string, string>;
  /** 思考元信息:token 数(端点上报)/思考耗时(ms);done 时定格 */
  streamingReasoningInfo: Record<
    string,
    { tokens: number | null; elapsedMs: number | null } | undefined
  >;
  /** 加载会话消息;force=false 时已缓存且非空则跳过(页面切换回会话不重复拉取) */
  load: (sessionId: string, force?: boolean) => Promise<void>;
  appendUser: (
    sessionId: string,
    content: string,
    modelId?: string | null,
    attachments?: string | null,
  ) => Promise<number>;
  appendAssistant: (
    sessionId: string,
    content: string,
    modelId?: string | null,
    tokens?: number,
    sources?: string | null,
    reasoning?: string,
  ) => Promise<number>;
  /** 删除单条消息并刷新 */
  remove: (sessionId: string, id: number) => Promise<void>;
  startStreaming: (sessionId: string) => void;
  pushStreamDelta: (sessionId: string, delta: string) => void;
  pushStreamReasoning: (sessionId: string, delta: string) => void;
  /** 记录思考 token(done chunk 上报;仅计数无明文端点用) */
  setStreamReasoningTokens: (sessionId: string, tokens: number | null) => void;
  /** 结束流式:返回 { content, reasoning } 完整累积,便于持久化;思考耗时同步定格 */
  endStreaming: (sessionId: string) => { content: string; reasoning: string };
}

/** 流式去抖缓冲:delta 先进缓冲,定时批量刷进 streaming map,避免每个 chunk 都触发 React 渲染。
 *  50ms(约 20fps)在视觉流畅与渲染开销间取平衡;流式气泡独立订阅,主树不随刷新重渲染。 */
const FLUSH_MS = 50;
const streamBuffers: Record<string, string> = {};
const streamTimers: Record<string, ReturnType<typeof setTimeout> | undefined> = {};
// 思考流缓冲(与正文同节奏去抖)
const reasoningBuffers: Record<string, string> = {};
const reasoningTimers: Record<string, ReturnType<typeof setTimeout> | undefined> = {};
// 思考计时:首个 reasoning chunk 到首个正文 chunk / done
const reasoningStarts: Record<string, number> = {};

export const useMessagesStore = create<MessagesState>((set, get) => ({
  bySession: {},
  streaming: {},
  streamingReasoning: {},
  streamingReasoningInfo: {},

  load: async (sessionId, force = false) => {
    const cached = get().bySession[sessionId];
    if (!force && cached && cached.length > 0) return;
    const rows = await ipc.listMessages(sessionId);
    set((s) => ({ bySession: { ...s.bySession, [sessionId]: rows } }));
  },

  appendUser: async (sessionId, content, modelId = null, attachments = null) => {
    const id = await ipc.appendMessage({
      session_id: sessionId,
      role: 'user',
      content,
      model_id: modelId,
      tokens: 0,
      attachments,
    });
    await get().load(sessionId, true);
    return id;
  },

  appendAssistant: async (
    sessionId,
    content,
    modelId = null,
    tokens = 0,
    sources = null,
    reasoning = '',
  ) => {
    const id = await ipc.appendMessage({
      session_id: sessionId,
      role: 'assistant',
      content,
      model_id: modelId,
      tokens,
      sources,
      reasoning: reasoning || null,
    });
    await get().load(sessionId, true);
    return id;
  },

  startStreaming: (sessionId) => {
    reasoningStarts[sessionId] = 0; // 0 = 尚未开始计时
    set((s) => ({
      streaming: { ...s.streaming, [sessionId]: '' },
      streamingReasoning: { ...s.streamingReasoning, [sessionId]: '' },
      streamingReasoningInfo: { ...s.streamingReasoningInfo, [sessionId]: undefined },
    }));
  },

  remove: async (sessionId, id) => {
    await ipc.deleteMessage(id);
    await get().load(sessionId, true);
  },

  pushStreamDelta: (sessionId, delta) => {
    // 首个正文 chunk 到达:思考阶段结束,定格耗时
    if (reasoningStarts[sessionId]) {
      const elapsed = Date.now() - reasoningStarts[sessionId];
      reasoningStarts[sessionId] = 0;
      set((s) => ({
        streamingReasoningInfo: {
          ...s.streamingReasoningInfo,
          [sessionId]: { ...(s.streamingReasoningInfo[sessionId] ?? { tokens: null }), elapsedMs: elapsed },
        },
      }));
    }
    streamBuffers[sessionId] = (streamBuffers[sessionId] ?? '') + delta;
    if (streamTimers[sessionId]) return;
    streamTimers[sessionId] = setTimeout(() => {
      streamTimers[sessionId] = undefined;
      const pending = streamBuffers[sessionId] ?? '';
      streamBuffers[sessionId] = '';
      if (!pending) return;
      set((s) => ({
        streaming: {
          ...s.streaming,
          [sessionId]: (s.streaming[sessionId] ?? '') + pending,
        },
      }));
    }, FLUSH_MS);
  },

  pushStreamReasoning: (sessionId, delta) => {
    if (!reasoningStarts[sessionId]) reasoningStarts[sessionId] = Date.now();
    reasoningBuffers[sessionId] = (reasoningBuffers[sessionId] ?? '') + delta;
    if (reasoningTimers[sessionId]) return;
    reasoningTimers[sessionId] = setTimeout(() => {
      reasoningTimers[sessionId] = undefined;
      const pending = reasoningBuffers[sessionId] ?? '';
      reasoningBuffers[sessionId] = '';
      if (!pending) return;
      set((s) => ({
        streamingReasoning: {
          ...s.streamingReasoning,
          [sessionId]: (s.streamingReasoning[sessionId] ?? '') + pending,
        },
      }));
    }, FLUSH_MS);
  },

  setStreamReasoningTokens: (sessionId, tokens) => {
    set((s) => ({
      streamingReasoningInfo: {
        ...s.streamingReasoningInfo,
        [sessionId]: {
          tokens,
          elapsedMs: s.streamingReasoningInfo[sessionId]?.elapsedMs ?? null,
        },
      },
    }));
  },

  endStreaming: (sessionId) => {
    // 冲掉正文与思考两条缓冲里未刷出的内容,保证返回完整累积
    for (const timers of [streamTimers, reasoningTimers] as const) {
      const timer = timers[sessionId];
      if (timer) {
        clearTimeout(timer);
        timers[sessionId] = undefined;
      }
    }
    const pendingContent = streamBuffers[sessionId] ?? '';
    streamBuffers[sessionId] = '';
    const pendingReasoning = reasoningBuffers[sessionId] ?? '';
    reasoningBuffers[sessionId] = '';
    const content = (get().streaming[sessionId] ?? '') + pendingContent;
    const reasoning = (get().streamingReasoning[sessionId] ?? '') + pendingReasoning;
    // 思考耗时:若到 done 仍未有正文(如被截断),以现在定格
    let info = get().streamingReasoningInfo[sessionId];
    if (reasoningStarts[sessionId]) {
      const elapsed = Date.now() - reasoningStarts[sessionId];
      reasoningStarts[sessionId] = 0;
      info = { ...(info ?? { tokens: null }), elapsedMs: elapsed };
    }
    set((s) => {
      const next = { ...s.streaming };
      delete next[sessionId];
      const nextR = { ...s.streamingReasoning };
      delete nextR[sessionId];
      return {
        streaming: next,
        streamingReasoning: nextR,
        streamingReasoningInfo: { ...s.streamingReasoningInfo, [sessionId]: info },
      };
    });
    return { content, reasoning };
  },
}));
