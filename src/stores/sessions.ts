import { create } from 'zustand';
import * as ipc from '../lib/ipc';

interface SessionsState {
  sessions: ipc.SessionRow[];
  loading: boolean;
  refresh: () => Promise<void>;
  create: (title: string, modelId: string, agentId?: string | null) => Promise<string>;
  rename: (id: string, title: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  setModel: (id: string, modelId: string) => Promise<void>;
  /** 更新对话内联网开关 */
  setWebSearch: (id: string, enabled: boolean) => Promise<void>;
  /** 更新记忆注入策略(D3.1) */
  setMemoryStrategy: (id: string, strategy: ipc.MemoryStrategy) => Promise<void>;
  /** 绑定/解绑会话的 Agent(B2;null 解绑,会同步记忆注入策略) */
  setAgent: (id: string, agentId: string | null) => Promise<void>;
  /** 切换会话激活分支(F4),并同步本地 sessions 状态 */
  setActiveBranch: (id: string, branchId: string) => Promise<void>;
  /** 仅当会话没有任何消息时删除(空对话默认不保留),返回是否已删 */
  removeIfEmpty: (id: string) => Promise<boolean>;
  /** 批量清理空会话(0 条消息且超过 grace 秒未更新) */
  pruneEmpty: (graceSecs?: number) => Promise<void>;
  /** 划词提问:待预填到指定会话输入框的文本(消费后清空) */
  pendingComposer: { sessionId: string; text: string } | null;
  setPendingComposer: (p: { sessionId: string; text: string } | null) => void;
}

export const useSessionsStore = create<SessionsState>((set, get) => ({
  sessions: [],
  loading: false,
  pendingComposer: null,
  setPendingComposer: (p) => set({ pendingComposer: p }),

  refresh: async () => {
    set({ loading: true });
    try {
      const sessions = await ipc.listSessions();
      set({ sessions });
    } finally {
      set({ loading: false });
    }
  },

  create: async (title, modelId, agentId) => {
    const { id } = await ipc.createSession(title, modelId, agentId);
    await get().refresh();
    return id;
  },

  rename: async (id, title) => {
    await ipc.renameSession(id, title);
    await get().refresh();
  },

  remove: async (id) => {
    await ipc.deleteSession(id);
    await get().refresh();
  },

  setModel: async (id, modelId) => {
    await ipc.setSessionModel(id, modelId);
    set((s) => ({
      sessions: s.sessions.map((x) => (x.id === id ? { ...x, model_id: modelId } : x)),
    }));
  },

  setWebSearch: async (id, enabled) => {
    await ipc.setSessionWebSearch(id, enabled);
    set((s) => ({
      sessions: s.sessions.map((x) => (x.id === id ? { ...x, web_search_enabled: enabled } : x)),
    }));
  },

  setMemoryStrategy: async (id, strategy) => {
    await ipc.setSessionMemoryStrategy(id, strategy);
    set((s) => ({
      sessions: s.sessions.map((x) =>
        x.id === id ? { ...x, memory_injection_strategy: strategy } : x,
      ),
    }));
  },

  setAgent: async (id, agentId) => {
    await ipc.setSessionAgent(id, agentId);
    await get().refresh();
  },

  setActiveBranch: async (id, branchId) => {
    await ipc.setActiveBranch(id, branchId);
    set((s) => ({
      sessions: s.sessions.map((x) => (x.id === id ? { ...x, active_branch: branchId } : x)),
    }));
  },

  removeIfEmpty: async (id) => {
    const deleted = await ipc.deleteSessionIfEmpty(id);
    if (deleted) {
      set((s) => ({ sessions: s.sessions.filter((x) => x.id !== id) }));
    }
    return deleted;
  },

  pruneEmpty: async (graceSecs) => {
    const ids = await ipc.pruneEmptySessions(graceSecs);
    if (ids.length > 0) {
      set((s) => ({ sessions: s.sessions.filter((x) => !ids.includes(x.id)) }));
    }
  },
}));
