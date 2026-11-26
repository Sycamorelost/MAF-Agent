import { create } from 'zustand';

/** 单个会话的生成状态。preparing = 已点亮占位、但后端 run 尚未登记(准备阶段)。 */
export interface SessionRunState {
  /** 后端 run id;null = 没有进行中的生成 */
  runId: string | null;
  /** 准备阶段(联网搜索 / 预检索 / 落库 / 工具收集) */
  preparing: boolean;
  error: string | null;
}

interface ChatRunsState {
  bySession: Record<string, SessionRunState | undefined>;
  /** 点亮「准备中」:发送开始、run 尚未创建 */
  prepare: (sessionId: string) => void;
  /** 后端 run 已登记 */
  begin: (sessionId: string, runId: string) => void;
  /** 生成收尾;error 省略时保留原错误 */
  finish: (sessionId: string, error?: string | null) => void;
  setError: (sessionId: string, error: string | null) => void;
  isBusy: (sessionId: string) => boolean;
}

const EMPTY: SessionRunState = { runId: null, preparing: false, error: null };

export const useChatRunsStore = create<ChatRunsState>((set, get) => ({
  bySession: {},
  prepare: (sessionId) =>
    set((s) => {
      const cur = s.bySession[sessionId] ?? EMPTY;
      return {
        bySession: { ...s.bySession, [sessionId]: { ...cur, preparing: true, error: null } },
      };
    }),
  begin: (sessionId, runId) =>
    set((s) => {
      const cur = s.bySession[sessionId] ?? EMPTY;
      return { bySession: { ...s.bySession, [sessionId]: { ...cur, runId, preparing: false } } };
    }),
  finish: (sessionId, error) =>
    set((s) => {
      const cur = s.bySession[sessionId] ?? EMPTY;
      return {
        bySession: {
          ...s.bySession,
          [sessionId]: {
            ...cur,
            runId: null,
            preparing: false,
            error: error !== undefined ? error : cur.error,
          },
        },
      };
    }),
  setError: (sessionId, error) =>
    set((s) => {
      const cur = s.bySession[sessionId] ?? EMPTY;
      return { bySession: { ...s.bySession, [sessionId]: { ...cur, error } } };
    }),
  isBusy: (sessionId) => {
    const r = get().bySession[sessionId];
    return !!r && (r.preparing || r.runId !== null);
  },
}));

/** 非 React 环境(模块级 runtime)读取忙碌状态。 */
export const isChatRunBusy = (sessionId: string): boolean =>
  useChatRunsStore.getState().isBusy(sessionId);
