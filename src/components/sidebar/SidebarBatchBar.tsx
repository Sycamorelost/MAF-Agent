import { useState } from 'react';
import { ContextMenu } from '../ContextMenu';
import type { ProjectRow } from '../../lib/ipc';
import type { SelectApi } from './types';

interface Props {
  sel: SelectApi;
  count: number;
  /** 选中项中未归档 / 已归档的数量(决定展示「归档」或「恢复」) */
  activeCount: number;
  archivedCount: number;
  projects: ProjectRow[];
  /** 当前可全选的会话 id 集合 */
  allIds: string[];
  onArchive: () => void;
  onRestore: () => void;
  onMove: (projectId: string | null) => void;
  onRemove: () => void;
}

/**
 * 多选模式顶部的批量操作栏。
 * 归档 / 恢复按选中项构成显示;「移入文件夹」用二级菜单选择目标项目。
 */
export function SidebarBatchBar({
  sel,
  count,
  activeCount,
  archivedCount,
  projects,
  allIds,
  onArchive,
  onRestore,
  onMove,
  onRemove,
}: Props) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const allSelected = allIds.length > 0 && count === allIds.length;

  return (
    <div className="side-batch" role="toolbar" aria-label="批量操作">
      <div className="side-batch-head">
        <span className="side-batch-count">已选 {count} 项</span>
        <button
          type="button"
          className="side-batch-link"
          onClick={() => sel.toggleAll(allIds)}
        >
          {allSelected ? '取消全选' : '全选'}
        </button>
        <button type="button" className="side-batch-link" onClick={sel.end}>
          退出多选
        </button>
      </div>
      <div className="side-batch-actions">
        {activeCount > 0 && (
          <button type="button" className="btn btn-ghost" onClick={onArchive}>
            归档
          </button>
        )}
        {archivedCount > 0 && (
          <button type="button" className="btn btn-ghost" onClick={onRestore}>
            恢复
          </button>
        )}
        <button
          type="button"
          className="btn btn-ghost"
          disabled={count === 0}
          onClick={(e) => {
            const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
            setMenu({ x: r.left, y: r.bottom + 4 });
          }}
        >
          移入文件夹
        </button>
        <button
          type="button"
          className="btn btn-ghost danger"
          disabled={count === 0}
          onClick={onRemove}
        >
          删除
        </button>
      </div>

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={[
            { key: 'all', label: '全部会话(未分类)', onClick: () => onMove(null) },
            ...projects.map((p) => ({
              key: `m-${p.id}`,
              label: p.name || '(未命名项目)',
              onClick: () => onMove(p.id),
            })),
          ]}
        />
      )}
    </div>
  );
}
