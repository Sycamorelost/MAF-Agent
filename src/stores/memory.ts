import { create } from 'zustand';
import * as ipc from '../lib/ipc';

interface MemoryState {
  /** 按会话缓存 memory.md 内容 */
  bySession: Record<string, ipc.MemoryDto>;
  versions: Record<string, ipc.VersionItem[]>;
  /** 当前正在监听的会话 id 集合 */
  watching: Set<string>;
  load: (sessionId: string) => Promise<void>;
  write: (sessionId: string, content: string, note?: string) => Promise<void>;
  loadVersions: (sessionId: string) => Promise<void>;
  rollback: (sessionId: string, versionId: number) => Promise<void>;
  startWatch: (sessionId: string) => Promise<void>;
  stopWatch: (sessionId: string) => Promise<void>;
}

export const useMemoryStore = create<MemoryState>((set, get) => ({
  bySession: {},
  versions: {},
  watching: new Set<string>(),

  load: async (sessionId) => {
    const dto = await ipc.readMemory(sessionId);
    set((s) => ({ bySession: { ...s.bySession, [sessionId]: dto } }));
  },

  write: async (sessionId, content, note) => {
    await ipc.writeMemory(sessionId, content, note);
    await get().load(sessionId);
  },

  loadVersions: async (sessionId) => {
    const rows = await ipc.listMemoryVersions(sessionId);
    set((s) => ({ versions: { ...s.versions, [sessionId]: rows } }));
  },

  rollback: async (sessionId, versionId) => {
    await ipc.rollbackMemory(sessionId, versionId);
    await get().load(sessionId);
    await get().loadVersions(sessionId);
  },

  startWatch: async (sessionId) => {
    if (get().watching.has(sessionId)) return;
    await ipc.watchMemory(sessionId);
    set((s) => {
      const next = new Set(s.watching);
      next.add(sessionId);
      return { watching: next };
    });
  },

  stopWatch: async (sessionId) => {
    if (!get().watching.has(sessionId)) return;
    await ipc.unwatchMemory(sessionId);
    set((s) => {
      const next = new Set(s.watching);
      next.delete(sessionId);
      return { watching: next };
    });
  },
}));
