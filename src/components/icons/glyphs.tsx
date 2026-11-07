import logoUrl from '../../assets/logo.png';
import { I, type IconProps } from './base';

/** 品牌 logo 图标:使用应用图标图片(非线性 SVG,品牌标识特例)。 */
export const IconLogo = (p: IconProps) => {
  const s = p.size ?? 18;
  return (
    <img
      src={logoUrl}
      width={s}
      height={s}
      alt=""
      className={p.className}
      style={{ display: 'block', borderRadius: '22%', flexShrink: 0 }}
    />
  );
};

export const IconImage = (p: IconProps) => (
  <I {...p}>
    <rect x="3" y="3" width="18" height="18" rx="2" />
    <circle cx="8.5" cy="8.5" r="1.5" />
    <path d="m21 15-4.35-4.35a1.5 1.5 0 0 0-2.12 0L6 19" />
  </I>
);

export const IconTranslate = (p: IconProps) => (
  <I {...p}>
    <path d="m5 8 6 6" />
    <path d="m4 14 6-6 2-3" />
    <path d="M2 5h12" />
    <path d="M7 2h1" />
    <path d="m22 22-5-10-5 10" />
    <path d="M14 18h6" />
  </I>
);

export const IconSwap = (p: IconProps) => (
  <I {...p}>
    <path d="M8 3 4 7l4 4" />
    <path d="M4 7h16" />
    <path d="m16 21 4-4-4-4" />
    <path d="M20 17H4" />
  </I>
);

export const IconAttach = (p: IconProps) => (
  <I {...p}>
    <path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48" />
  </I>
);

export const IconQuote = (p: IconProps) => (
  <I {...p}>
    <path d="M10 11H6a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1v7a3 3 0 0 1-3 3" />
    <path d="M19 11h-4a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1v7a3 3 0 0 1-3 3" />
  </I>
);

// ---------- Agent 模板图标(迭代 B2) ----------

export const IconSparkles = (p: IconProps) => (
  <I {...p}>
    <path d="M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9Z" />
    <path d="M19 15l.9 2.1L22 18l-2.1.9L19 21l-.9-2.1L16 18l2.1-.9Z" />
  </I>
);

export const IconLanguages = (p: IconProps) => (
  <I {...p}>
    <path d="M4 5h9" />
    <path d="M8.5 3v2c0 4-2.5 7.5-5.5 9" />
    <path d="M5 8c1 3 4 5.5 7 6" />
    <path d="m12.5 21 4.5-10 4.5 10" />
    <path d="M14 17h6" />
  </I>
);

export const IconClipboardList = (p: IconProps) => (
  <I {...p}>
    <rect x="8" y="2" width="8" height="4" rx="1" />
    <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" />
    <path d="M9 12h6M9 16h6" />
  </I>
);

export const IconFileText = (p: IconProps) => (
  <I {...p}>
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z" />
    <path d="M14 2v6h6" />
    <path d="M9 13h6M9 17h6" />
  </I>
);

export const IconCompare = (p: IconProps) => (
  <I {...p}>
    <circle cx="6" cy="6" r="2.5" />
    <circle cx="18" cy="18" r="2.5" />
    <path d="M6 8.5v5a2 2 0 0 0 2 2h4" />
    <path d="m10 13 2 2.5-2 2.5" />
    <path d="M18 15.5v-5a2 2 0 0 0-2-2h-4" />
    <path d="m14 6-2 2.5 2 2.5" />
  </I>
);

export const IconPullRequest = (p: IconProps) => (
  <I {...p}>
    <circle cx="6" cy="6" r="2.5" />
    <circle cx="6" cy="18" r="2.5" />
    <circle cx="18" cy="8" r="2.5" />
    <path d="M6 8.5v7" />
    <path d="M18 10.5v3a3 3 0 0 1-3 3h-4" />
    <path d="m13 13.5-2.5 3 2.5 3" />
  </I>
);

export const IconBug = (p: IconProps) => (
  <I {...p}>
    <rect x="8" y="7" width="8" height="12" rx="4" />
    <path d="M12 7V4M9 4l1.5 2M15 4l-1.5 2" />
    <path d="M8 11H4M8 15H4M20 11h-4M20 15h-4" />
    <path d="m8.5 19-2 2M15.5 19l2 2" />
  </I>
);

export const IconWand = (p: IconProps) => (
  <I {...p}>
    <path d="m15 4 5 5L8 21l-5-5Z" />
    <path d="m14 7 3 3" />
    <path d="M9 4v2M4 9h2M19 14v2M17 20h2" />
  </I>
);

export const IconShield = (p: IconProps) => (
  <I {...p}>
    <path d="M12 2 4 5.5V11c0 5 3.4 9.4 8 11 4.6-1.6 8-6 8-11V5.5Z" />
    <path d="m9 12 2 2 4-4" />
  </I>
);

export const IconGauge = (p: IconProps) => (
  <I {...p}>
    <path d="M4 14a8 8 0 1 1 16 0" />
    <path d="M12 14l4-4" />
    <path d="M4 14h1M19 14h1M12 6V5" />
    <circle cx="12" cy="14" r="1.4" />
  </I>
);

export const IconArchive = (p: IconProps) => (
  <I {...p}>
    <rect x="3" y="4" width="18" height="5" rx="1" />
    <path d="M5 9v10a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V9" />
    <path d="M10 13h4" />
  </I>
);

/** 文件夹(工作台会话分组)。 */

/** 文件夹(工作台会话分组)。 */
export const IconFolder = (p: IconProps) => (
  <I {...p}>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z" />
  </I>
);

/** 文件夹打开态(分组展开指示)。 */

/** 文件夹打开态(分组展开指示)。 */
export const IconFolderOpen = (p: IconProps) => (
  <I {...p}>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v1H3V7z" />
    <path d="M3 10h18l-2 9a1 1 0 0 1-1 .8H4a1 1 0 0 1-1-.8V10z" />
  </I>
);

/** 纲要 / 列表(会话纲要面板开关)。 */

/** 纲要 / 列表(会话纲要面板开关)。 */
export const IconList = (p: IconProps) => (
  <I {...p}>
    <path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" />
  </I>
);

/** 信息提示(附件解析异常等)。 */

/** 信息提示(附件解析异常等)。 */
export const IconInfo = (p: IconProps) => (
  <I {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 8h.01M11 12h1v4h1" />
  </I>
);

/** 终端(shell / 命令执行工具)。 */

/** 终端(shell / 命令执行工具)。 */
export const IconTerminal = (p: IconProps) => (
  <I {...p}>
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <path d="m7 9 3 3-3 3M13 15h4" />
  </I>
);

export const IconNote = (p: IconProps) => (
  <I {...p}>
    <rect x="4" y="3" width="16" height="18" rx="2" />
    <path d="M8 3v18" />
    <path d="M12 8h4M12 12h4M12 16h2" />
  </I>
);

export const IconBot = (p: IconProps) => (
  <I {...p}>
    <rect x="4" y="8" width="16" height="12" rx="2" />
    <path d="M12 8V4M8 4h8" />
    <circle cx="9" cy="13" r="0.5" fill="currentColor" />
    <circle cx="15" cy="13" r="0.5" fill="currentColor" />
    <path d="M9 17h6" />
  </I>
);

/** 柱状图(用量统计)。 */
export const IconChart = (p: IconProps) => (
  <I {...p}>
    <path d="M4 20h16" />
    <path d="M7 20v-6M12 20v-11M17 20v-4" />
  </I>
);

/** 拖拽手柄(六点,阶段排序)。 */
