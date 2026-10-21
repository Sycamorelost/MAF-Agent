import { useRef, useState } from 'react';
import type { PointerEvent as ReactPointerEvent, RefObject } from 'react';

/** 输入框高度:用户拖拽顶部把手调整,持久化到 localStorage(主页/对话页共用)。 */
const HEIGHT_KEY = 'composer-h';
const MIN_BOX_H = 48;
const MAX_BOX_H = 420;

function readStoredHeight(): number {
  try {
    const v = Number(localStorage.getItem(HEIGHT_KEY));
    if (Number.isFinite(v) && v >= MIN_BOX_H && v <= MAX_BOX_H) return Math.round(v);
  } catch {
    /* ignore */
  }
  return 0; // 0 = 未设置,使用 rows 默认高度
}

/** 顶部把手的上下拖拽高度调整(boxH=0 时用 rows 默认高度)。 */
export function useComposerHeight(ref: RefObject<HTMLTextAreaElement | null>) {
  const [boxH, setBoxH] = useState(() => readStoredHeight());
  const [resizing, setResizing] = useState(false);
  const hStart = useRef({ y: 0, h: 0 });

  const onResizeDown = (e: ReactPointerEvent) => {
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    hStart.current = { y: e.clientY, h: boxH || ref.current?.clientHeight || MIN_BOX_H };
    setResizing(true);
  };
  const onResizeMove = (e: ReactPointerEvent) => {
    if (!resizing) return;
    const raw = hStart.current.h + (hStart.current.y - e.clientY);
    setBoxH(Math.min(MAX_BOX_H, Math.max(MIN_BOX_H, Math.round(raw))));
  };
  const onResizeUp = (e: ReactPointerEvent) => {
    if (!resizing) return;
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
    setResizing(false);
    setBoxH((h) => {
      try {
        localStorage.setItem(HEIGHT_KEY, String(h));
      } catch {
        /* ignore */
      }
      return h;
    });
  };

  return { boxH, resizing, onResizeDown, onResizeMove, onResizeUp };
}
