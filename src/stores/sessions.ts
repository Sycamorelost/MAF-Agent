import { create } from 'zustand';
import * as ipc from '../lib/ipc';

interface SessionsState {
  sessions: ipc.SessionRow[];
  /** 是否已完成至少一次加载(用于区分「还没拉取」与「确实不存在」) */
  loaded: boolean;
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
  /** 更新会话级工作台模式覆盖(D16) */
  setWorkbenchMode: (id: string, mode: ipc.WorkbenchMode) => Promise<void>;
  /** 更新会话级权限档覆盖(D11) */
  setPermissionMode: (id: string, mode: ipc.PermissionMode) => Promise<void>;
  /** 钉住/恢复会话级运行时预设(D19;null = 恢复继承) */
  setPreset: (id: string, presetId: string | null) => Promise<void>;
  /** 绑定/解绑会话的 Agent(B2;null 解绑,会同步记忆注入策略) */
  setAgent: (id: string, agentId: string | null) => Promise<void>;
  /** 切换会话激活分支(F4),并同步本地 sessions 状态 */
  setActiveBranch: (id: string, branchId: string) => Promise<void>;
  /** 把会话归入文件夹(P8);null = 移出文件夹 */
  setProject: (id: string, projectId: string | null) => Promise<void>;
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
  loaded: false,
  loading: false,
  pendingComposer: null,
  setPendingComposer: (p) => set({ pendingComposer: p }),

  refresh: async () => {
    set({ loading: true });
    try {
      const sessions = await ipc.listSessions();
      set({ sessions });
    } finally {
      set({ loading: false, loaded: true });
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

  setWorkbenchMode: async (id, mode) => {
    await ipc.setSessionWorkbenchMode(id, mode);
    set((s) => ({
      sessions: s.sessions.map((x) => (x.id === id ? { ...x, workbench_mode: mode } : x)),
    }));
  },

  setPermissionMode: async (id, mode) => {
    await ipc.setSessionPermissionMode(id, mode);
    set((s) => ({
      sessions: s.sessions.map((x) => (x.id === id ? { ...x, permission_mode: mode } : x)),
    }));
  },

  setPreset: async (id, presetId) => {
    const info = await ipc.setSessionPreset(id, presetId);
    set((s) => ({
      sessions: s.sessions.map((x) =>
        x.id === id
          ? { ...x, preset_id: info.pinned_id, preset_version: info.pinned_version }
          : x,
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

  setProject: async (id, projectId) => {
    await ipc.setSessionProject(id, projectId);
    set((s) => ({
      sessions: s.sessions.map((x) => (x.id === id ? { ...x, project_id: projectId } : x)),
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
