import { create } from 'zustand';
import * as ipc from '../lib/ipc';
import type { DeliverableDto } from '../lib/ipc';

interface DeliverablesState {
  bySession: Record<string, DeliverableDto[]>;
  /** 按会话加载产物;force=true 时强制刷新(流式结束后调用) */
  load: (sessionId: string, force?: boolean) => Promise<void>;
}

/**
 * 产物(交付物)缓存:M2 起由后端 `present` 工具登记,
 * 前端按 message_id 归入各条助手消息;正文路径解析只作兜底。
 */
export const useDeliverablesStore = create<DeliverablesState>((set, get) => ({
  bySession: {},
  load: async (sessionId, force = false) => {
    if (!force && get().bySession[sessionId]) return;
    try {
      const list = await ipc.listDeliverables(sessionId);
      set((s) => ({ bySession: { ...s.bySession, [sessionId]: list } }));
    } catch {
      /* 历史会话没有产物表或后端未就绪时静默降级到正文解析 */
    }
  },
}));
