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
  /** 流式占位阶段(P0-2/P1-4):preparing/search/connecting;undefined = 默认「正在思考…」 */
  streamingPhase: Record<string, string | undefined>;
  /** 占位开始时间戳(ms;P0-2「已等待 Xs」计时),endStreaming 清除 */
  streamStartedAt: Record<string, number>;
  /** 会话是否还有更早的消息未加载(分页) */
  hasMore: Record<string, boolean>;
  /** 加载会话消息;force=false 时已缓存且非空则跳过(页面切换回会话不重复拉取) */
  load: (sessionId: string, force?: boolean) => Promise<void>;
  /** 加载更早一页(前置拼接,保持当前内容不变) */
  loadOlder: (sessionId: string) => Promise<void>;
  appendUser: (
    sessionId: string,
    content: string,
    modelId?: string | null,
    attachments?: string | null,
    /** P0-1:传入乐观渲染的临时行 id,落库成功后就地替换(不重插、不闪动) */
    optimisticId?: number,
  ) => Promise<number>;
  /** P0-1:乐观插入用户消息(临时负 id),返回临时 id;失败/取消时用 removeLocalRow 清掉 */
  appendUserOptimistic: (
    sessionId: string,
    content: string,
    modelId?: string | null,
    attachments?: string | null,
  ) => number;
  /** P0-1:移除本地临时行(发送失败/准备阶段取消时兜底) */
  removeLocalRow: (sessionId: string, id: number) => void;
  appendAssistant: (
    sessionId: string,
    content: string,
    modelId?: string | null,
    tokens?: number,
    sources?: string | null,
    reasoning?: string,
    /** V36(P1):断流/报错时保存的部分内容(true = 已中断) */
    partial?: boolean,
  ) => Promise<number>;
  /** 删除单条消息并刷新 */
  remove: (sessionId: string, id: number) => Promise<void>;
  startStreaming: (sessionId: string) => void;
  /** P0-2/P1-4:更新占位阶段文案(preparing/search/connecting;null = 恢复默认) */
  setStreamPhase: (sessionId: string, phase: string | null) => void;
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
/** 初始页 / 每次「加载更早」的条数(覆盖上下文裁剪的 80 条窗口) */
const PAGE_SIZE = 100;
const streamBuffers: Record<string, string> = {};
const streamTimers: Record<string, ReturnType<typeof setTimeout> | undefined> = {};
// 思考流缓冲(与正文同节奏去抖)
const reasoningBuffers: Record<string, string> = {};
const reasoningTimers: Record<string, ReturnType<typeof setTimeout> | undefined> = {};
// 思考计时:首个 reasoning chunk 到首个正文 chunk / done
const reasoningStarts: Record<string, number> = {};

export const useMessagesStore = create<MessagesState>((set, get) => {
  /** P12:把刚写入的消息乐观插入缓存;缓存尚未加载时退回整页重拉,避免丢失更早历史。 */
  const pushRow = async (sessionId: string, row: ipc.MessageRow) => {
    const cur = get().bySession[sessionId];
    if (!cur) {
      await get().load(sessionId, true);
      return;
    }
    set((s) => {
      const list = s.bySession[sessionId] ?? [];
      // 防止与后台分页加载产生重复行
      if (list.some((m) => m.id === row.id)) return {};
      return { bySession: { ...s.bySession, [sessionId]: [...list, row] } };
    });
  };

  return {
  bySession: {},
  hasMore: {},
  streaming: {},
  streamingReasoning: {},
  streamingReasoningInfo: {},
  streamingPhase: {},
  streamStartedAt: {},

  load: async (sessionId, force = false) => {
    const cached = get().bySession[sessionId];
    if (!force && cached && cached.length > 0) return;
    const page = await ipc.listMessagesPage(sessionId, undefined, PAGE_SIZE);
    set((s) => ({
      bySession: { ...s.bySession, [sessionId]: page.messages },
      hasMore: { ...s.hasMore, [sessionId]: page.has_more },
    }));
  },

  loadOlder: async (sessionId) => {
    const cur = get().bySession[sessionId];
    if (!cur || cur.length === 0 || get().hasMore[sessionId] !== true) return;
    const page = await ipc.listMessagesPage(sessionId, cur[0].id, PAGE_SIZE);
    const seen = new Set(cur.map((m) => m.id));
    const older = page.messages.filter((m) => !seen.has(m.id));
    set((s) => ({
      bySession: {
        ...s.bySession,
        [sessionId]: [...older, ...(s.bySession[sessionId] ?? [])],
      },
      hasMore: { ...s.hasMore, [sessionId]: page.has_more },
    }));
  },

  appendUser: async (sessionId, content, modelId = null, attachments = null, optimisticId) => {
    const row = await ipc.appendMessage({
      session_id: sessionId,
      role: 'user',
      content,
      model_id: modelId,
      tokens: 0,
      attachments,
    });
    if (optimisticId !== undefined) {
      // P0-1:用真实行就地替换乐观行;临时行已被清掉(极端竞态)时按去重追加兜底
      set((s) => {
        const list = s.bySession[sessionId] ?? [];
        const idx = list.findIndex((m) => m.id === optimisticId);
        if (idx < 0) {
          if (list.some((m) => m.id === row.id)) return {};
          return { bySession: { ...s.bySession, [sessionId]: [...list, row] } };
        }
        const next = [...list];
        next[idx] = row;
        return { bySession: { ...s.bySession, [sessionId]: next } };
      });
    } else {
      await pushRow(sessionId, row);
    }
    return row.id;
  },

  appendUserOptimistic: (sessionId, content, modelId = null, attachments = null) => {
    // 负 id 与 SQLite 自增 id 不会冲突;仅在落库前存在,成功后由真实行替换
    const tempId = -Date.now();
    const row: ipc.MessageRow = {
      id: tempId,
      session_id: sessionId,
      role: 'user',
      content,
      model_id: modelId,
      created: new Date().toISOString(),
      tokens: 0,
      attachments,
      sources: null,
      parent_id: null,
      branch_id: 'main',
      reasoning: '',
    };
    set((s) => ({
      bySession: { ...s.bySession, [sessionId]: [...(s.bySession[sessionId] ?? []), row] },
    }));
    return tempId;
  },

  removeLocalRow: (sessionId, id) => {
    set((s) => ({
      bySession: {
        ...s.bySession,
        [sessionId]: (s.bySession[sessionId] ?? []).filter((m) => m.id !== id),
      },
    }));
  },

  appendAssistant: async (
    sessionId,
    content,
    modelId = null,
    tokens = 0,
    sources = null,
    reasoning = '',
    partial = false,
  ) => {
    const row = await ipc.appendMessage({
      session_id: sessionId,
      role: 'assistant',
      content,
      model_id: modelId,
      tokens,
      sources,
      reasoning: reasoning || null,
      partial,
    });
    await pushRow(sessionId, row);
    return row.id;
  },

  startStreaming: (sessionId) => {
    reasoningStarts[sessionId] = 0; // 0 = 尚未开始计时
    set((s) => ({
      streaming: { ...s.streaming, [sessionId]: '' },
      streamingReasoning: { ...s.streamingReasoning, [sessionId]: '' },
      streamingReasoningInfo: { ...s.streamingReasoningInfo, [sessionId]: undefined },
      // P0-2:占位初始阶段为「准备中」;后续由联网搜索/后端 ack 推进
      streamingPhase: { ...s.streamingPhase, [sessionId]: 'preparing' },
      streamStartedAt: { ...s.streamStartedAt, [sessionId]: Date.now() },
    }));
  },

  setStreamPhase: (sessionId, phase) => {
    set((s) => ({ streamingPhase: { ...s.streamingPhase, [sessionId]: phase ?? undefined } }));
  },

  remove: async (sessionId, id) => {
    await ipc.deleteMessage(id);
    await get().load(sessionId, true);
  },

  pushStreamDelta: (sessionId, delta) => {
    // 流已结束(startStreaming 之后被 endStreaming 清掉):丢弃晚到的 chunk。
    // 否则这些 delta 会把 streaming 重新创建出来,留下一个永远收不住的闪烁光标。
    if (get().streaming[sessionId] === undefined) return;
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
    // P2-9:首包直通 —— 第一个 delta 立即渲染(不再等 50ms 合批),缩短「首字延迟」体感
    if ((get().streaming[sessionId] ?? '') === '') {
      const pending = streamBuffers[sessionId];
      streamBuffers[sessionId] = '';
      set((s) => ({
        streaming: { ...s.streaming, [sessionId]: pending },
        // 已有内容到达:阶段占位恢复默认(占位随后被正文取代)
        streamingPhase: { ...s.streamingPhase, [sessionId]: undefined },
      }));
      return;
    }
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
    // 同正文:流结束后的思考 chunk 直接丢弃,不复活气泡
    if (get().streamingReasoning[sessionId] === undefined) return;
    if (!reasoningStarts[sessionId]) reasoningStarts[sessionId] = Date.now();
    reasoningBuffers[sessionId] = (reasoningBuffers[sessionId] ?? '') + delta;
    if (reasoningTimers[sessionId]) return;
    // P2-9:首个思考 chunk 同样直通 —— 推理模型的「首字」就是思考内容
    if ((get().streamingReasoning[sessionId] ?? '') === '') {
      const pending = reasoningBuffers[sessionId];
      reasoningBuffers[sessionId] = '';
      set((s) => ({
        streamingReasoning: { ...s.streamingReasoning, [sessionId]: pending },
        streamingPhase: { ...s.streamingPhase, [sessionId]: undefined },
      }));
      return;
    }
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
      const nextP = { ...s.streamingPhase };
      delete nextP[sessionId];
      const nextT = { ...s.streamStartedAt };
      delete nextT[sessionId];
      return {
        streaming: next,
        streamingReasoning: nextR,
        streamingReasoningInfo: { ...s.streamingReasoningInfo, [sessionId]: info },
        streamingPhase: nextP,
        streamStartedAt: nextT,
      };
    });
    return { content, reasoning };
  },
  };
});
