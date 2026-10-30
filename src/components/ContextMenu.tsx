/**
 * 可复用右键/上下文菜单组件 —— 严格匹配项目 `.nav-pop` 玻璃风格。
 * 支持:图标、快捷键、danger、disabled、divider、子菜单(hov 展开)。
 */
import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

export interface MenuItem {
  key: string;
  label?: string;
  icon?: React.ReactNode;
  shortcut?: string;
  danger?: boolean;
  disabled?: boolean;
  checked?: boolean;
  divider?: boolean;
  onClick?: () => void;
  children?: MenuItem[];
}

interface ContextMenuProps {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
  /** 内部使用:子菜单时不 portal,直接渲染在父 DOM 内 */
  _inline?: boolean;
}

/** 边界保护 + 偏移定位(仅顶级菜单) */
function adjustPosition(x: number, y: number, w: number, h: number): { x: number; y: number } {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  let fx = x, fy = y;
  if (fx + w > vw - 8) fx = Math.max(8, vw - w - 8);
  if (fy + h > vh - 8) fy = Math.max(8, vh - h - 8);
  if (fx < 8) fx = 8;
  if (fy < 8) fy = 8;
  return { x: fx, y: fy };
}

export function ContextMenu({ x, y, items, onClose, _inline }: ContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [openKey, setOpenKey] = useState<string | null>(null);
  const [renderPos, setRenderPos] = useState({ x, y });

  // 顶级菜单:首挂载测尺寸修正越界;子菜单:由父控制布局
  useEffect(() => {
    if (_inline) return;
    const el = ref.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    setRenderPos(adjustPosition(x, y, rect.width, rect.height));
  }, [x, y, _inline]);

  useEffect(() => {
    if (_inline) return;
    const el = ref.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    setRenderPos((prev) => {
      const next = adjustPosition(prev.x, prev.y, rect.width, rect.height);
      if (next.x === prev.x && next.y === prev.y) return prev;
      return next;
    });
  }, [openKey, _inline]);

  // 点击外部 / ESC 关闭(仅顶级菜单)
  useEffect(() => {
    if (_inline) return;
    const onDocClick = (e: MouseEvent) => {
      const t = e.target as Node;
      if (ref.current && !ref.current.contains(t)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    const id = setTimeout(() => {
      document.addEventListener('mousedown', onDocClick);
      document.addEventListener('contextmenu', onDocClick);
      document.addEventListener('keydown', onKey);
    }, 0);
    return () => {
      clearTimeout(id);
      document.removeEventListener('mousedown', onDocClick);
      document.removeEventListener('contextmenu', onDocClick);
      document.removeEventListener('keydown', onKey);
    };
  }, [onClose, _inline]);

  const handleClick = (item: MenuItem) => {
    if (item.disabled || item.divider || item.children) return;
    item.onClick?.();
    onClose();
  };

  const body = (
    <div
      ref={ref}
      className={`ctx-menu${_inline ? ' ctx-sub' : ''}`}
      style={_inline ? undefined : { position: 'fixed', left: renderPos.x, top: renderPos.y, zIndex: 'var(--z-menu)' }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((it) => {
        if (it.divider) return <div key={it.key} className="ctx-sep" />;
        const hasSub = !!it.children;
        const subOpen = openKey === it.key;
        return (
          <div
            key={it.key}
            className="ctx-item-wrap"
            onMouseEnter={() => hasSub && setOpenKey(it.key)}
            onMouseLeave={() => subOpen && setOpenKey(null)}
          >
            <button
              type="button"
              className={`ctx-item${it.danger ? ' danger' : ''}${it.disabled ? ' disabled' : ''}${it.checked ? ' checked' : ''}`}
              onClick={(e) => {
                e.stopPropagation();
                handleClick(it);
              }}
              disabled={it.disabled}
            >
              <span className="ctx-item-ico">{it.icon}</span>
              <span className="ctx-item-label">{it.label}</span>
              {it.checked && <span className="ctx-check">✓</span>}
              {it.shortcut && <span className="ctx-short">{it.shortcut}</span>}
              {hasSub && <span className="ctx-arr">›</span>}
            </button>
            {hasSub && subOpen && (
              // 子菜单:inline 渲染,靠 CSS absolute 定位在父项右侧
              <ContextMenu x={0} y={0} items={it.children!} onClose={onClose} _inline />
            )}
          </div>
        );
      })}
    </div>
  );

  return _inline ? body : createPortal(body, document.body);
}
