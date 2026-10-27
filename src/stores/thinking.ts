import { create } from 'zustand';

/** 统一思考档位(improving.md §1.4;Light/Medium/High/Ultra 四档 + 开关)。 */
export type ThinkingLevel = 'light' | 'medium' | 'high' | 'ultra';

export interface ThinkingSelection {
  /** 思考开关(不可关模型锁定为 true,由 UI 侧保证) */
  on: boolean;
  level: ThinkingLevel;
}

/** 全局默认档位(选择开启思考时使用的档位)。 */
export const DEFAULT_THINKING_LEVEL: ThinkingLevel = 'medium';

interface ThinkingState {
  /** sessionId -> 用户选择;缺省 = 关闭思考(需要时在对话页手动开启) */
  bySession: Record<string, ThinkingSelection>;
  init: (sessionId: string, defaultLevel?: ThinkingLevel) => void;
  set: (sessionId: string, sel: ThinkingSelection) => void;
}

/** 会话级思考选择:与会话同生命周期(切换会话各自记忆;不持久化,刷新回到默认)。 */
export const useThinkingStore = create<ThinkingState>((set, get) => ({
  bySession: {},
  init: (sessionId, defaultLevel) => {
    if (get().bySession[sessionId]) return;
    set((s) => ({
      bySession: {
        ...s.bySession,
        // 默认关闭思考:长上下文下思考 token 会显著拉长首字延迟,需要时由用户手动开启
        [sessionId]: { on: false, level: defaultLevel ?? DEFAULT_THINKING_LEVEL },
      },
    }));
  },
  set: (sessionId, sel) =>
    set((s) => ({ bySession: { ...s.bySession, [sessionId]: sel } })),
}));
