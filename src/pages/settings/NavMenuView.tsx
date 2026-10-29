//! 设置 · 导航菜单可见性与顺序视图。
//!
//! 顺序通过整行拖拽调整(HTML5 Drag & Drop),放下即写盘并广播,
//! 标题栏 ⋮ 菜单无需刷新即可重排;显示开关同样即时生效。

import { useMemo, useState } from 'react';
import { IconDrag } from '../../components/icons';
import { NAV_VISIBILITY } from './constants';

export interface NavMenuViewProps {
  /** 已隐藏的导航项 key */
  hiddenNav: string[];
  /** 已保存的显示顺序(功能项 key) */
  navOrder: string[];
  onToggleNavItem: (key: string) => void;
  /** 把 fromKey 拖到 toKey 的位置(两个方向都支持) */
  onReorderNavItem: (fromKey: string, toKey: string) => void;
}

export function NavMenuView({
  hiddenNav,
  navOrder,
  onToggleNavItem,
  onReorderNavItem,
}: NavMenuViewProps) {
  /** 有效顺序:已保存顺序在前,未列出的按内置顺序补在末尾(稳定排序)。 */
  const items = useMemo(() => {
    const rank = new Map(navOrder.map((k, i) => [k, i]));
    return [...NAV_VISIBILITY].sort(
      (a, b) =>
        (rank.get(a.key) ?? Number.MAX_SAFE_INTEGER) - (rank.get(b.key) ?? Number.MAX_SAFE_INTEGER),
    );
  }, [navOrder]);

  const [dragKey, setDragKey] = useState<string | null>(null);
  const [overKey, setOverKey] = useState<string | null>(null);

  const onDragStart = (e: React.DragEvent<HTMLDivElement>, key: string) => {
    setDragKey(key);
    e.dataTransfer.effectAllowed = 'move';
    // Firefox 必须写一次数据才会启动拖拽
    e.dataTransfer.setData('text/plain', key);
  };

  const onDragOver = (e: React.DragEvent<HTMLDivElement>, key: string) => {
    if (!dragKey) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    if (key !== overKey) setOverKey(key);
  };

  const onDrop = (e: React.DragEvent<HTMLDivElement>, key: string) => {
    e.preventDefault();
    const from = dragKey;
    setDragKey(null);
    setOverKey(null);
    if (!from || from === key) return;
    onReorderNavItem(from, key);
  };

  const endDrag = () => {
    setDragKey(null);
    setOverKey(null);
  };

  return (
    <>
      <h2>导航菜单</h2>
      <p className="set-desc">
        控制标题栏 ⋮ 下拉菜单中显示的功能入口与排列顺序。拖住左侧手柄即可调整顺序;
        「对话」与「设置」固定保留,其余项可自行决定是否展示,隐藏后不影响功能本身。
      </p>

      <div className="set-card">
        <div className="set-card-h">功能入口</div>
        <div className="set-card-b">
          {items.map((it) => {
            const shown = !hiddenNav.includes(it.key);
            const dragging = dragKey === it.key;
            const dropTarget = overKey === it.key && dragKey !== null && dragKey !== it.key;
            return (
              <div
                className={`set-row set-drag${dragging ? ' dragging' : ''}${dropTarget ? ' drop-target' : ''}`}
                key={it.key}
                draggable
                onDragStart={(e) => onDragStart(e, it.key)}
                onDragOver={(e) => onDragOver(e, it.key)}
                onDrop={(e) => onDrop(e, it.key)}
                onDragEnd={endDrag}
              >
                <span className="set-drag-handle" title="拖拽调整顺序" aria-hidden="true">
                  <IconDrag size={14} />
                </span>
                <label className="lbl">{it.label}</label>
                <button
                  type="button"
                  role="switch"
                  aria-checked={shown}
                  className={`set-switch${shown ? ' on' : ''}`}
                  onClick={() => onToggleNavItem(it.key)}
                >
                  <span className="knob" />
                </button>
                <span className="set-hint">{shown ? '在菜单中显示' : '已隐藏'}</span>
              </div>
            );
          })}
          <div className="set-hint" style={{ marginTop: 'var(--space-2)' }}>
            对话与设置始终显示;显示与顺序改动即时生效,无需保存。
          </div>
        </div>
      </div>
    </>
  );
}
