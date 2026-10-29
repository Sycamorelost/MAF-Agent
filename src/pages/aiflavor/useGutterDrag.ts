import { useCallback, useEffect, useRef, useState } from 'react';
import type { PointerEvent as ReactPointerEvent } from 'react';

/** 三栏布局 gutter 拖拽 hook:直接写 CSS 变量 + localStorage 持久化,
 *  不通过 React state 传值,拖拽期间零重渲染。
 *  axis:  'x' = 竖向分隔条(调左右栏宽,鼠标 clientX)
 *         'y' = 横向分隔条(调上下栏高,鼠标 clientY)
 *  grow:  gutter 向哪个方向拖拽增加 target 尺寸。
 *         'prev' = 左/上面板增; 'next' = 右/下面板增
 *  varName: 被写入的 CSS 变量名
 *  min/max: 尺寸的 px 边界 */
export function useGutterDrag(
  axis: 'x' | 'y',
  grow: 'prev' | 'next',
  varName: string,
  key: string,
  def: number,
  min: number,
  max: number,
) {
  const [dragging, setDragging] = useState(false);
  const start = useRef({ p: 0, v: 0 });
  const varRef = useRef(varName);
  const boundsRef = useRef({ min, max });
  varRef.current = varName;
  boundsRef.current = { min, max };

  // 挂载时从 localStorage 恢复值写入 CSS 变量
  useEffect(() => {
    let val = def;
    try {
      const s = localStorage.getItem(`gutter:${key}`);
      if (s) val = Math.max(min, Math.min(max, Number.parseFloat(s)));
    } catch {
      /* ignore */
    }
    document.documentElement.style.setProperty(varName, `${val}px`);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const onDown = useCallback(
    (e: ReactPointerEvent) => {
      e.preventDefault();
      const el = e.currentTarget as HTMLElement;
      el.setPointerCapture(e.pointerId);
      const raw = getComputedStyle(document.documentElement).getPropertyValue(varName);
      const cur = Number.parseFloat(raw) || def;
      start.current = {
        p: axis === 'x' ? e.clientX : e.clientY,
        v: cur,
      };
      setDragging(true);

      const onMove = (ev: PointerEvent) => {
        const delta = (axis === 'x' ? ev.clientX : ev.clientY) - start.current.p;
        const { min: mn, max: mx } = boundsRef.current;
        const dir = grow === 'prev' ? delta : -delta;
        const next = Math.round(Math.max(mn, Math.min(mx, start.current.v + dir)));
        document.documentElement.style.setProperty(varRef.current, `${next}px`);
      };

      const onUp = () => {
        setDragging(false);
        const raw = getComputedStyle(document.documentElement).getPropertyValue(varRef.current);
        const v = Number.parseFloat(raw);
        if (Number.isFinite(v)) {
          try {
            localStorage.setItem(`gutter:${key}`, String(Math.round(v)));
          } catch {
            /* ignore */
          }
        }
        document.removeEventListener('pointermove', onMove);
      };

      document.addEventListener('pointermove', onMove);
      document.addEventListener('pointerup', onUp, { once: true });
    },
    [axis, grow, varName, def],
  );

  return { dragging, onDown };
}
