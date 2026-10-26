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
  /** 用户上传壁纸的绝对路径（null 表示未设置，仅展示/持久化用）。 */
  wallpaperPath: string | null;
  /** 当前壁纸的 blob URL（用于渲染，data URL 过大无法写入 inline style）。 */
  wallpaperBlobUrl: string | null;
  /** 切换主题：挂载 data-theme + 持久化到 localStorage。 */
  applyTheme: (t: ThemeKey) => void;
  /** 设置/清除壁纸：path 用于展示，dataUrl（base64）用于实际渲染（WebView2 不支持 file://）。 */
  setWallpaper: (path: string | null, dataUrl?: string | null) => void;
  /** 设置壁纸遮罩不透明度（0-100），0=壁纸完全可见，100=主题默认遮罩。 */
  setWallpaperCover: (cover: number) => void;
}

/** 将 base64 data URL 同步转为 Blob URL（避免 2MB+ data URL 写入 inline style 被 WebView2 静默丢弃）。 */
function dataUrlToBlobUrl(dataUrl: string): string {
  const commaIdx = dataUrl.indexOf(',');
  const header = dataUrl.slice(0, commaIdx);
  const base64 = dataUrl.slice(commaIdx + 1);
  const mime = header.match(/data:([^;]+);base64/)?.[1] ?? 'application/octet-stream';
  const binary = atob(base64);
  const len = binary.length;
  const bytes = new Uint8Array(len);
  for (let i = 0; i < len; i++) bytes[i] = binary.charCodeAt(i);
  const blob = new Blob([bytes], { type: mime });
  return URL.createObjectURL(blob);
}

/**
 * 主题管理：
 * - 主题切换走 localStorage 即时持久化，不依赖后端；
 * - 壁纸路径由设置页从 AppSettings 同步进来，data URL 由 read_wallpaper 命令获取，
 *   setWallpaper 只负责渲染层挂载。
 */
export const useThemeStore = create<ThemeState>((set, get) => ({
  theme: readTheme(),
  wallpaperPath: null,
  wallpaperBlobUrl: null,

  applyTheme: (t) => {
    try {
      localStorage.setItem(LS_THEME, t);
    } catch {
      // 忽略隐私模式等存储异常
    }
    document.documentElement.setAttribute('data-theme', t);
    set({ theme: t });
  },

  setWallpaper: (path, dataUrl) => {
    const root = document.documentElement;
    const win = document.querySelector('.window') as HTMLElement | null;
    const prevBlobUrl = get().wallpaperBlobUrl;

    if (path && dataUrl) {
      // data URL 可能超过 inline style 长度上限(约 2MB)，转 blob URL 再挂载
      const blobUrl = dataUrlToBlobUrl(dataUrl);
      if (win) {
        win.style.backgroundImage =
          `linear-gradient(135deg,` +
          ` rgba(var(--bg-overlay-rgb), calc(var(--bg-overlay-a) * var(--wallpaper-cover, 1))),` +
          ` rgba(var(--bg-overlay-rgb), calc(var(--bg-overlay-b) * var(--wallpaper-cover, 1)))),` +
          ` url("${blobUrl}")`;
        win.style.backgroundSize = 'cover';
        win.style.backgroundPosition = 'center';
        win.style.backgroundRepeat = 'no-repeat';
      }
      root.style.setProperty('--bg-user-image', `url("${blobUrl}")`);
      root.setAttribute('data-has-wallpaper', '');
      root.removeAttribute('data-theme');
      set({ wallpaperPath: path, wallpaperBlobUrl: blobUrl });
    } else {
      if (win) {
        win.style.backgroundImage = '';
        win.style.backgroundSize = '';
        win.style.backgroundPosition = '';
        win.style.backgroundRepeat = '';
      }
      root.style.removeProperty('--bg-user-image');
      root.removeAttribute('data-has-wallpaper');
      root.setAttribute('data-theme', useThemeStore.getState().theme);
      set({ wallpaperPath: null, wallpaperBlobUrl: null });
    }

    // 延迟释放旧 blob URL:同步 revoke 可能使仍在合成层中的旧图失效导致闪白;
    // 500ms 足够新图完成解码。即使期间再次更换,每个旧 URL 各有定时器,到期都会回收。
    if (prevBlobUrl) {
      const stale = prevBlobUrl;
      setTimeout(() => URL.revokeObjectURL(stale), 500);
    }
  },

  setWallpaperCover: (cover) => {
    // 0-100 → 0-1，作为遮罩 alpha 的乘数
    const factor = Math.max(0, Math.min(100, cover)) / 100;
    document.documentElement.style.setProperty('--wallpaper-cover', String(factor));
  },
}));
