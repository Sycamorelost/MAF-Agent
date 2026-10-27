import { create } from 'zustand';

const LS_THEME = 'metaagent.theme';

/** 预制主题 key。 */
export type ThemeKey = 'default' | 'haze' | 'midnight' | 'pine' | 'sunset' | 'sakura';

/** 主题元信息（设置页色卡展示用）。 */
export interface ThemeMeta {
  key: ThemeKey;
  label: string;
  /** 色卡预览主色（对应 --bg-main）。 */
  swatch: string;
}

/** 全部预制主题。 */
export const THEMES: ThemeMeta[] = [
  { key: 'default', label: '默认纯白', swatch: '#FFFFFF' },
  { key: 'haze', label: '雾霾蓝', swatch: '#F4F6FA' },
  { key: 'midnight', label: '深夜蓝', swatch: '#1E222C' },
  { key: 'pine', label: '松烟绿', swatch: '#F1F4F1' },
  { key: 'sunset', label: '暖橙日落', swatch: '#FBF6EF' },
  { key: 'sakura', label: '樱花粉', swatch: '#FBF5F8' },
];

const readTheme = (): ThemeKey => {
  try {
    const v = localStorage.getItem(LS_THEME);
    if (v && THEMES.some((t) => t.key === v)) return v as ThemeKey;
  } catch {
    // 忽略隐私模式等存储异常
  }
  return 'default';
};

interface ThemeState {
  /** 当前主题 key。 */
  theme: ThemeKey;
  /** 切换主题：挂载 data-theme + 持久化到 localStorage。 */
  applyTheme: (t: ThemeKey) => void;
}

/**
 * 主题管理：主题切换走 localStorage 即时持久化，不依赖后端。
 */
export const useThemeStore = create<ThemeState>((set) => ({
  theme: readTheme(),

  applyTheme: (t) => {
    try {
      localStorage.setItem(LS_THEME, t);
    } catch {
      // 忽略隐私模式等存储异常
    }
    document.documentElement.setAttribute('data-theme', t);
    set({ theme: t });
  },
}));
