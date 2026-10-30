/**
 * 共享线性 SVG 图标库。
 *
 * 规则(见 project_memory.md):
 *   - stroke 风格,1.5px 描边,round caps & joins
 *   - 默认 18px,可通过 size 覆盖
 *   - 颜色继承 currentColor,由父级控制
 *   - 禁止在 UI 中使用 emoji;所有图标必须走这里
 */

interface IconProps {
  size?: number;
  className?: string;
  strokeWidth?: number;
}

function I({
  size = 18,
  className,
  strokeWidth = 1.5,
  children,
  viewBox = '0 0 24 24',
}: IconProps & { children: React.ReactNode; viewBox?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox={viewBox}
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export const IconChat = (p: IconProps) => (
  <I {...p}>
    <path d="M21 12c0 4.418-4.03 8-9 8-1.05 0-2.06-.16-3-.46L4 21l1.46-4.38C4.55 15.4 4 13.76 4 12c0-4.418 4.03-8 9-8s9 3.582 9 8Z" />
  </I>
);

export const IconBrain = (p: IconProps) => (
  <I {...p}>
    <path d="M12 4a4 4 0 0 0-4 4 4 4 0 0 0-3 3.87A4 4 0 0 0 7 20h1" />
    <path d="M12 4a4 4 0 0 1 4 4 4 4 0 0 1 3 3.87A4 4 0 0 1 17 20h-1" />
    <path d="M12 4v16" />
  </I>
);

export const IconLayers = (p: IconProps) => (
  <I {...p}>
    <path d="m12 3 9 5-9 5-9-5 9-5Z" />
    <path d="m3 13 9 5 9-5" />
  </I>
);

export const IconSearch = (p: IconProps) => (
  <I {...p}>
    <circle cx="11" cy="11" r="7" />
    <path d="m20 20-3.5-3.5" />
  </I>
);

export const IconGraduation = (p: IconProps) => (
  <I {...p}>
    <path d="m22 9-10-5L2 9l10 5 10-5Z" />
    <path d="M6 11.5V16c0 1.66 2.69 3 6 3s6-1.34 6-3v-4.5" />
    <path d="M22 9v5" />
  </I>
);

export const IconResearch = (p: IconProps) => (
  <I {...p}>
    <path d="M6 18h8" />
    <path d="M3 22h18" />
    <path d="M14 22a7 7 0 1 0 0-14h-1" />
    <path d="M9 14h2" />
    <path d="M9 12a2 2 0 0 1-2-2V6h6v4a2 2 0 0 1-2 2Z" />
    <path d="M12 6V3a1 1 0 0 0-1-1H9a1 1 0 0 0-1 1v3" />
  </I>
);

export const IconSettings = (p: IconProps) => (
  <I {...p}>
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z" />
  </I>
);

export const IconPlus = (p: IconProps) => (
  <I {...p}>
    <path d="M12 5v14M5 12h14" />
  </I>
);

export const IconX = (p: IconProps) => (
  <I {...p}>
    <path d="M18 6 6 18M6 6l12 12" />
  </I>
);

export const IconSend = (p: IconProps) => (
  <I {...p}>
    <path d="m22 2-7 20-4-9-9-4Z" />
    <path d="M22 2 11 13" />
  </I>
);

export const IconEdit = (p: IconProps) => (
  <I {...p}>
    <path d="M12 20h9" />
    <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4Z" />
  </I>
);

export const IconHistory = (p: IconProps) => (
  <I {...p}>
    <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
    <path d="M3 3v5h5" />
    <path d="M12 7v5l4 2" />
  </I>
);

export const IconRefresh = (p: IconProps) => (
  <I {...p}>
    <path d="M3 12a9 9 0 0 1 15-6.7L21 8" />
    <path d="M21 3v5h-5" />
    <path d="M21 12a9 9 0 0 1-15 6.7L3 16" />
    <path d="M3 21v-5h5" />
  </I>
);

export const IconTrash = (p: IconProps) => (
  <I {...p}>
    <path d="M3 6h18" />
    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
    <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
  </I>
);

export const IconDoc = (p: IconProps) => (
  <I {...p}>
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z" />
    <path d="M14 2v6h6" />
    <path d="M16 13H8M16 17H8M10 9H8" />
  </I>
);

import logoUrl from '../assets/logo.png';

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

export const IconCopy = (p: IconProps) => (
  <I {...p}>
    <rect x="9" y="9" width="12" height="12" rx="2" />
    <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
  </I>
);

/** 更多(三点):消息操作栏溢出菜单入口 */
export const IconMore = (p: IconProps) => (
  <I {...p}>
    <circle cx="5" cy="12" r="1.6" />
    <circle cx="12" cy="12" r="1.6" />
    <circle cx="19" cy="12" r="1.6" />
  </I>
);

export const IconCheck = (p: IconProps) => (
  <I {...p}>
    <path d="M20 6 9 17l-5-5" />
  </I>
);

export const IconPlay = (p: IconProps) => (
  <I {...p}>
    <path d="M6 4l14 8-14 8V4z" />
  </I>
);

export const IconStop = (p: IconProps) => (
  <I {...p}>
    <rect x="6" y="6" width="12" height="12" rx="2" />
  </I>
);

export const IconSkipForward = (p: IconProps) => (
  <I {...p}>
    <path d="M5 5l9 7-9 7V5Z" />
    <path d="M19 5v14" />
  </I>
);

export const IconDownload = (p: IconProps) => (
  <I {...p}>
    <path d="M12 15V3M12 15l-4-4m4 4l4-4M3 17v2a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-2" />
  </I>
);

export const IconUpload = (p: IconProps) => (
  <I {...p}>
    <path d="M12 15V3m0 0L8 7m4-4l4 4M3 17v2a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-2" />
  </I>
);

export const IconPause = (p: IconProps) => (
  <I {...p}>
    <rect x="6.5" y="5" width="3.5" height="14" rx="1" />
    <rect x="14" y="5" width="3.5" height="14" rx="1" />
  </I>
);

export const IconGlobe = (p: IconProps) => (
  <I {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M3 12h18" />
    <path d="M12 3a13.5 13.5 0 0 1 0 18M12 3a13.5 13.5 0 0 0 0 18" />
  </I>
);

export const IconChevronDown = (p: IconProps) => (
  <I {...p}>
    <path d="m6 9 6 6 6-6" />
  </I>
);

export const IconChevronRight = (p: IconProps) => (
  <I {...p}>
    <path d="m9 18 6-6-6-6" />
  </I>
);

export const IconChevronLeft = (p: IconProps) => (
  <I {...p}>
    <path d="m15 18-6-6 6-6" />
  </I>
);

export const IconExternalLink = (p: IconProps) => (
  <I {...p}>
    <path d="M15 3h6v6" />
    <path d="M10 14 21 3" />
    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
  </I>
);

export const IconArrowLeft = (p: IconProps) => (
  <I {...p}>
    <path d="M19 12H5" />
    <path d="m12 19-7-7 7-7" />
  </I>
);

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

export const IconEye = (p: IconProps) => (
  <I {...p}>
    <path d="M2.036 12.322a1.012 1.012 0 0 1 0-.639C3.423 7.51 7.36 4.5 12 4.5c4.638 0 8.573 3.007 9.963 7.178.07.207.07.431 0 .639C20.577 16.49 16.64 19.5 12 19.5c-4.638 0-8.573-3.007-9.963-7.178Z" />
    <path d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z" />
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
export const IconFolder = (p: IconProps) => (
  <I {...p}>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z" />
  </I>
);

/** 文件夹打开态(分组展开指示)。 */
export const IconFolderOpen = (p: IconProps) => (
  <I {...p}>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v1H3V7z" />
    <path d="M3 10h18l-2 9a1 1 0 0 1-1 .8H4a1 1 0 0 1-1-.8V10z" />
  </I>
);

/** 纲要 / 列表(会话纲要面板开关)。 */
export const IconList = (p: IconProps) => (
  <I {...p}>
    <path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" />
  </I>
);

/** 信息提示(附件解析异常等)。 */
export const IconInfo = (p: IconProps) => (
  <I {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 8h.01M11 12h1v4h1" />
  </I>
);

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

export const IconPin = (p: IconProps) => (
  <I {...p}>
    <path d="M9 4h6l-1 6 3 3v2H7v-2l3-3zM12 15v5" />
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
export const IconDrag = (p: IconProps) => (
  <I {...p} strokeWidth={2.2}>
    <path d="M9 6h.01M15 6h.01M9 12h.01M15 12h.01M9 18h.01M15 18h.01" />
  </I>
);

/** 上箭头(阶段上移)。 */
export const IconArrowUp = (p: IconProps) => (
  <I {...p}>
    <path d="M12 19V5m-6 6 6-6 6 6" />
  </I>
);

/** 下箭头(阶段下移)。 */
export const IconArrowDown = (p: IconProps) => (
  <I {...p}>
    <path d="M12 5v14m-6-6 6 6 6-6" />
  </I>
);

/** 恢复/重新启用(逆时针回环箭头)。 */
export const IconRestore = (p: IconProps) => (
  <I {...p}>
    <path d="M4 10a8 8 0 1 1 2 6" />
    <path d="M4 4v6h6" />
  </I>
);

/** 闪电(快捷提示)。 */
export const IconZap = (p: IconProps) => (
  <I {...p}>
    <path d="M13 2 4 14h6l-1 8 9-12h-6l1-8Z" />
  </I>
);

/** 时钟(已暂停 / 等待确认)。 */
export const IconClock = (p: IconProps) => (
  <I {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v5l3 2" />
  </I>
);

/** 右箭头(「全部任务」等跳转入口)。 */
export const IconArrowRight = (p: IconProps) => (
  <I {...p}>
    <path d="M4 12h15m-6-6 6 6-6 6" />
  </I>
);

/** 侧栏面板(检查器展开/收起)。 */
export const IconPanel = (p: IconProps) => (
  <I {...p}>
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <path d="M15 4v16" />
  </I>
);

/** Agent 模板 icon key → 组件(未知 key 回退 IconBot)。 */
export function agentIconComponent(key: string) {
  switch (key) {
    case 'sparkles':
      return IconSparkles;
    case 'languages':
      return IconLanguages;
    case 'clipboard-list':
      return IconClipboardList;
    case 'file-text':
      return IconFileText;
    case 'git-compare':
      return IconCompare;
    case 'git-pull-request':
      return IconPullRequest;
    case 'refresh-cw':
      return IconRefresh;
    case 'bug':
      return IconBug;
    case 'search':
      return IconSearch;
    case 'wand-sparkles':
      return IconWand;
    case 'shield-check':
      return IconShield;
    case 'archive':
      return IconArchive;
    case 'chat':
      return IconChat;
    case 'note':
      return IconNote;
    case 'brain':
      return IconBrain;
    case 'globe':
      return IconGlobe;
    case 'settings':
      return IconSettings;
    case 'image':
      return IconImage;
    case 'translate':
      return IconTranslate;
    case 'research':
      return IconResearch;
    case 'attach':
      return IconAttach;
    case 'doc':
      return IconDoc;
    case 'check':
      return IconCheck;
    case 'edit':
      return IconEdit;
    default:
      return IconBot;
  }
}
