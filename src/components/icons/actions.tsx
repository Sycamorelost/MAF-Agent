import { I, type IconProps } from './base';

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

export const IconCopy = (p: IconProps) => (
  <I {...p}>
    <rect x="9" y="9" width="12" height="12" rx="2" />
    <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
  </I>
);

/** 更多(三点):消息操作栏溢出菜单入口 */

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

export const IconEye = (p: IconProps) => (
  <I {...p}>
    <path d="M2.036 12.322a1.012 1.012 0 0 1 0-.639C3.423 7.51 7.36 4.5 12 4.5c4.638 0 8.573 3.007 9.963 7.178.07.207.07.431 0 .639C20.577 16.49 16.64 19.5 12 19.5c-4.638 0-8.573-3.007-9.963-7.178Z" />
    <path d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z" />
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

/** 闪电(快捷提示)。 */
export const IconZap = (p: IconProps) => (
  <I {...p}>
    <path d="M13 2 4 14h6l-1 8 9-12h-6l1-8Z" />
  </I>
);

/** 时钟(已暂停 / 等待确认)。 */

/** 时钟(已暂停 / 等待确认)。 */
export const IconClock = (p: IconProps) => (
  <I {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v5l3 2" />
  </I>
);

/** 右箭头(「全部任务」等跳转入口)。 */

export const IconPin = (p: IconProps) => (
  <I {...p}>
    <path d="M9 4h6l-1 6 3 3v2H7v-2l3-3zM12 15v5" />
  </I>
);

/** 柱状图(用量统计)。 */

/** 拖拽手柄(六点,阶段排序)。 */
export const IconDrag = (p: IconProps) => (
  <I {...p} strokeWidth={2.2}>
    <path d="M9 6h.01M15 6h.01M9 12h.01M15 12h.01M9 18h.01M15 18h.01" />
  </I>
);

/** 上箭头(阶段上移)。 */
