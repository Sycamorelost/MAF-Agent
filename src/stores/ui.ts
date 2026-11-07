import { create } from 'zustand';
import { clampSidebarWidth, defaultSidebarWidth } from '../components/sidebar/utils';

const LS_SIDEBAR = 'metaagent.sidebar';
const LS_SIDEBAR_W = 'metaagent.sidebar.width';

/** 布尔持久化:键不存在时取 fallback(侧栏默认展开,避免老用户升级后无侧栏)。 */
const readFlag = (key: string, fallback: boolean) => {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : v === '1';
  } catch {
    return fallback;
  }
};

const writeFlag = (key: string, v: boolean) => {
  try {
    localStorage.setItem(key, v ? '1' : '0');
  } catch {
    // 忽略隐私模式等存储异常
  }
};

const readWidth = () => {
  try {
    const raw = localStorage.getItem(LS_SIDEBAR_W);
    if (raw === null) return defaultSidebarWidth();
    return clampSidebarWidth(Number(raw));
  } catch {
    return defaultSidebarWidth();
  }
};

/** 全局 UI 状态(标题栏 ↔ 侧栏/页面抽屉联动)。 */
interface UiState {
  /** 侧栏是否展开(false = 折叠为 56px 图标轨;持久化到 localStorage) */
  sidebarOpen: boolean;
  setSidebarOpen: (v: boolean) => void;
  /** 侧栏展开态宽度(200–320px,拖拽调整;持久化) */
  sidebarWidth: number;
  setSidebarWidth: (v: number) => void;
}

/**
 * 侧栏状态持久化:展开后保持,直到用户再次点击折叠或 Esc;
 * 路由切换 / 应用重启均不复位。
 */
export const useUiStore = create<UiState>((set) => ({
  sidebarOpen: readFlag(LS_SIDEBAR, true),
  setSidebarOpen: (v) => {
    writeFlag(LS_SIDEBAR, v);
    set({ sidebarOpen: v });
  },
  sidebarWidth: readWidth(),
  setSidebarWidth: (v) => {
    const w = clampSidebarWidth(v);
    try {
      localStorage.setItem(LS_SIDEBAR_W, String(w));
    } catch {
      // 忽略隐私模式等存储异常
    }
    set({ sidebarWidth: w });
  },
}));
