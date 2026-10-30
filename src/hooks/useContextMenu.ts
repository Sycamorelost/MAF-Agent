/**
 * 右键菜单状态 hook:open 时返回 {pos, target},close 时全 null。
 * close/open 用 useCallback 稳定引用,避免 ContextMenu 的 useEffect 反复重绑。
 */
import { useCallback, useState } from 'react';

export function useContextMenu<T = unknown>() {
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const [target, setTarget] = useState<T | null>(null);

  const open = useCallback((e: React.MouseEvent, t?: T) => {
    e.preventDefault();
    e.stopPropagation();
    setTarget((t as T) ?? null);
    setPos({ x: e.clientX, y: e.clientY });
  }, []);

  const close = useCallback(() => {
    setPos(null);
    setTarget(null);
  }, []);

  return { pos, target, open, close };
}
