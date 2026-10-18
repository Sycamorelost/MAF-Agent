/**
 * 附件「插入当前会话」跨页传递队列(迭代 F7)。
 * 附件中心点「插入」→ enqueue(目标会话) 并跳转;ChatPage 挂载时按 sessionId 消费,
 * 把 AttachmentMeta 追加进输入框待发附件列表。
 */

import { create } from 'zustand';
import type { AttachmentMeta } from '../lib/ipc';

interface AttachmentInsertState {
  queue: { sessionId: string; items: AttachmentMeta[] } | null;
  enqueue: (sessionId: string, items: AttachmentMeta[]) => void;
  /** 仅当队列属于该会话时取出并清空,否则返回 null。 */
  consume: (sessionId: string) => AttachmentMeta[] | null;
}

export const useAttachmentInsertStore = create<AttachmentInsertState>((set, get) => ({
  queue: null,
  enqueue: (sessionId, items) => set({ queue: { sessionId, items } }),
  consume: (sessionId) => {
    const q = get().queue;
    if (!q || q.sessionId !== sessionId) return null;
    set({ queue: null });
    return q.items;
  },
}));
