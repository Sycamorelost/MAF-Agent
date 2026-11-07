import { I, type IconProps } from './base';

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

/** 上箭头(阶段上移)。 */
export const IconArrowUp = (p: IconProps) => (
  <I {...p}>
    <path d="M12 19V5m-6 6 6-6 6 6" />
  </I>
);

/** 下箭头(阶段下移)。 */

/** 下箭头(阶段下移)。 */
export const IconArrowDown = (p: IconProps) => (
  <I {...p}>
    <path d="M12 5v14m-6-6 6 6 6-6" />
  </I>
);

/** 恢复/重新启用(逆时针回环箭头)。 */

/** 右箭头(「全部任务」等跳转入口)。 */
export const IconArrowRight = (p: IconProps) => (
  <I {...p}>
    <path d="M4 12h15m-6-6 6 6-6 6" />
  </I>
);

/** 侧栏面板(检查器展开/收起)。 */

/** 侧栏面板(检查器展开/收起)。 */
export const IconPanel = (p: IconProps) => (
  <I {...p}>
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <path d="M15 4v16" />
  </I>
);

/** Agent 模板 icon key → 组件(未知 key 回退 IconBot)。 */
