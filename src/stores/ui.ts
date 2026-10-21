import { create } from 'zustand';

const LS_SIDEBAR = 'metaagent.sidebar';
const LS_ASIDE = 'metaagent.aside';

const readFlag = (key: string) => {
  try {
    return localStorage.getItem(key) === '1';
  } catch {
    return false;
  }
};

const writeFlag = (key: string, v: boolean) => {
  try {
    localStorage.setItem(key, v ? '1' : '0');
  } catch {
    // 忽略隐私模式等存储异常
  }
};

/** 全局 UI 状态(标题栏玻璃组件 ↔ 会话抽屉/页面抽屉联动)。 */
interface UiState {
  /** 会话历史抽屉是否展开(玻璃侧栏,从标题栏 pill 开关;持久化到 localStorage) */
  sidebarOpen: boolean;
  setSidebarOpen: (v: boolean) => void;
  /** 页面左栏抽屉是否展开(研究/绘图/翻译/记忆/知识库/设置,标题栏「面板」按钮开关;持久化) */
  asideOpen: boolean;
  setAsideOpen: (v: boolean) => void;
}

/**
 * 抽屉状态持久化:打开后保持,直到用户再次点击 pill 按钮或抽屉 × 才关闭;
 * 路由切换 / 应用重启均不复位。
 */
export const useUiStore = create<UiState>((set) => ({
  sidebarOpen: readFlag(LS_SIDEBAR),
  setSidebarOpen: (v) => {
    writeFlag(LS_SIDEBAR, v);
    set({ sidebarOpen: v });
  },
  asideOpen: readFlag(LS_ASIDE),
  setAsideOpen: (v) => {
    writeFlag(LS_ASIDE, v);
    set({ asideOpen: v });
  },
}));
