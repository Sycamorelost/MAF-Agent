import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from 'react';

/** 浮层展开方向:up = 向上(默认),down = 上方空间不足时向下翻转 */
export type MenuPlacement = 'up' | 'down';

export interface MenuPos {
  placement: MenuPlacement;
  /** up:浮层底边距窗口底部的距离 */
  bottom?: number;
  /** down:浮层顶边距窗口顶部的距离 */
  top?: number;
  left: number;
  maxHeight: number;
}

interface AnchorRect {
  top: number;
  bottom: number;
  left: number;
}

interface Viewport {
  width: number;
  height: number;
}

interface MenuMetrics {
  width: number;
  /** 判断是否需要翻转的最小高度:上方空间不足时改为向下 */
  minHeight: number;
  /** 最大高度上限,默认 320 */
  maxHeight?: number;
}

const MARGIN = 8; // 距窗口边缘的最小留白
const GAP = 8; // 浮层与锚点之间的间距
const FLOOR = 96; // 极小窗口下的兜底高度,保证内容可见

/**
 * 计算浮层固定定位坐标(纯函数,便于单测)。
 * - 左右钳制在窗口内,留 MARGIN 边距;
 * - 上方可用空间够 minHeight(或优于下方)时向上展开,否则向下翻转;
 * - maxHeight 取可用空间与上限的较小值,允许内部滚动。
 */
export function computeMenuPos(
  anchor: AnchorRect,
  viewport: Viewport,
  menu: MenuMetrics,
): MenuPos {
  const menuW = Math.max(0, Math.min(menu.width, viewport.width - MARGIN * 2));
  const left = Math.max(
    MARGIN,
    Math.min(anchor.left, viewport.width - menuW - MARGIN),
  );
  const above = anchor.top - GAP - MARGIN;
  const below = viewport.height - anchor.bottom - GAP - MARGIN;
  // 输入区在窗口底部,优先向上展开;窗口很矮时向下翻转避开任务条
  const up = above >= menu.minHeight || above >= below;
  const space = up ? above : below;
  const maxHeight = Math.max(FLOOR, Math.min(space, menu.maxHeight ?? 320));
  return up
    ? { placement: 'up', bottom: viewport.height - anchor.top + GAP, left, maxHeight }
    : { placement: 'down', top: anchor.bottom + GAP, left, maxHeight };
}

interface Options {
  minHeight?: number;
  maxHeight?: number;
  /** 菜单首帧尚未完成测量时的宽度兜底 */
  fallbackWidth?: number;
}

/**
 * 输入区浮层定位:实测锚点矩形 + 窗口尺寸,返回 fixed 定位所需字段。
 * 浮层需 createPortal 到 document.body 并将 pos 写成内联样式,
 * 才能脱离输入区容器的层叠上下文,不被顶部任务条遮住。
 */
export function usePinMenu(
  open: boolean,
  anchorRef: RefObject<HTMLElement | null>,
  menuRef: RefObject<HTMLElement | null>,
  options: Options = {},
) {
  const [pos, setPos] = useState<MenuPos>({
    placement: 'up',
    bottom: GAP,
    left: MARGIN,
    maxHeight: 240,
  });
  // options 常以字面量传入,固定引用避免反复重建 sync
  const optRef = useRef(options);
  optRef.current = options;

  const sync = useCallback(() => {
    const anchor = anchorRef.current;
    if (!anchor) return;
    const measured = menuRef.current?.getBoundingClientRect().width ?? 0;
    setPos(
      computeMenuPos(
        anchor.getBoundingClientRect(),
        { width: window.innerWidth, height: window.innerHeight },
        {
          width: measured > 0 ? measured : (optRef.current.fallbackWidth ?? 200),
          minHeight: optRef.current.minHeight ?? 120,
          maxHeight: optRef.current.maxHeight,
        },
      ),
    );
  }, [anchorRef, menuRef]);

  // 打开时在绘制前完成定位,避免先闪现默认位置的跳动
  useLayoutEffect(() => {
    if (open) sync();
  }, [open, sync]);

  useEffect(() => {
    if (!open) return;
    window.addEventListener('resize', sync);
    document.addEventListener('fullscreenchange', sync);
    return () => {
      window.removeEventListener('resize', sync);
      document.removeEventListener('fullscreenchange', sync);
    };
  }, [open, sync]);

  return { pos, sync };
}
