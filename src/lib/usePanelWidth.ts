import { useCallback, useRef, useState } from 'react';
import type { PointerEvent as ReactPointerEvent } from 'react';

/**
 * 可拖拽调宽的侧栏 hook(布局统一改造)。
 * 宽度缓存到 localStorage(key = panel-w:{key}),下次启动恢复。
 * direction:
 *  - 'left'  面板在左侧,把手在面板右缘,向右拖 = 变宽
 *  - 'right' 面板在右侧,把手在面板左缘,向左拖 = 变宽
 */
const KEY_PREFIX = 'panel-w:';

function readStored(key: string, def: number, min: number, max: number): number {
  try {
    const v = Number(localStorage.getItem(KEY_PREFIX + key));
    if (Number.isFinite(v) && v >= min && v <= max) return Math.round(v);
  } catch {
    /* localStorage 不可用时用默认值 */
  }
  return def;
}

export interface PanelResize {
  /** 当前宽度(px),直接作为面板 inline width */
  width: number;
  /** 是否窄档(用于降字号/紧凑样式) */
  narrow: boolean;
  dragging: boolean;
  /** 绑定到把手元素的 pointer 事件 */
  onHandleDown: (e: ReactPointerEvent) => void;
  onHandleMove: (e: ReactPointerEvent) => void;
  onHandleUp: (e: ReactPointerEvent) => void;
}

export function usePanelWidth(
  key: string,
  def: number,
  min: number,
  max: number,
  direction: 'left' | 'right' = 'left',
  narrowAt = Math.round(def * 0.85),
): PanelResize {
  const [width, setWidth] = useState(() => readStored(key, def, min, max));
  const [dragging, setDragging] = useState(false);
  const start = useRef({ x: 0, w: 0 });

  const onHandleDown = useCallback(
    (e: ReactPointerEvent) => {
      e.preventDefault();
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      start.current = { x: e.clientX, w: width };
      setDragging(true);
    },
    [width],
  );

  const onHandleMove = useCallback(
    (e: ReactPointerEvent) => {
      if (!dragging) return;
      const dx = e.clientX - start.current.x;
      const raw = direction === 'left' ? start.current.w + dx : start.current.w - dx;
      setWidth(Math.min(max, Math.max(min, Math.round(raw))));
    },
    [dragging, direction, min, max],
  );

  const onHandleUp = useCallback(
    (e: ReactPointerEvent) => {
      if (!dragging) return;
      try {
        (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
      } catch {
        /* ignore */
      }
      setDragging(false);
      setWidth((w) => {
        try {
          localStorage.setItem(KEY_PREFIX + key, String(w));
        } catch {
          /* ignore */
        }
        return w;
      });
    },
    [dragging, key],
  );

  return { width, narrow: width < narrowAt, dragging, onHandleDown, onHandleMove, onHandleUp };
}
