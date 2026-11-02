//! 附件中心顶部栏:搜索、类型筛选、批量管理 / 拒绝记录入口;批量模式下附带操作条。

import type { Kind } from './shared';
import { KIND_FILTERS } from './shared';
import { IconCheck, IconInfo, IconSearch, IconTrash, IconX } from '../../components/icons';

export interface AttachmentsHeaderProps {
  q: string;
  onQuery: (v: string) => void;
  kind: 'all' | Kind;
  onKind: (k: 'all' | Kind) => void;
  batchMode: boolean;
  selectedCount: number;
  shownCount: number;
  rejectTotal: number;
  deleting: boolean;
  onToggleBatch: () => void;
  onSelectAll: () => void;
  onClearSelect: () => void;
  onDeleteSelected: () => void;
  onOpenRejects: () => void;
}

export function AttachmentsHeader({
  q,
  onQuery,
  kind,
  onKind,
  batchMode,
  selectedCount,
  shownCount,
  rejectTotal,
  deleting,
  onToggleBatch,
  onSelectAll,
  onClearSelect,
  onDeleteSelected,
  onOpenRejects,
}: AttachmentsHeaderProps) {
  return (
    <>
      <div className="att-head">
        <span className="att-title">附件中心</span>
        <div className="att-search">
          <IconSearch size={14} />
          <input
            value={q}
            onChange={(e) => onQuery(e.target.value)}
            placeholder="按文件名搜索…"
            spellCheck={false}
          />
          {q && (
            <button className="att-search-clear" title="清空搜索" onClick={() => onQuery('')}>
              <IconX size={12} />
            </button>
          )}
        </div>
        <div className="att-kinds">
          {KIND_FILTERS.map((f) => (
            <button
              key={f.key}
              className={`att-kind ${kind === f.key ? 'active' : ''}`}
              onClick={() => onKind(f.key)}
            >
              {f.label}
            </button>
          ))}
        </div>
        <div className="att-actions">
          <button
            className={`att-btn${batchMode ? ' active' : ''}`}
            onClick={onToggleBatch}
            title={batchMode ? '退出批量管理' : '批量选择并清理附件'}
          >
            {batchMode ? <IconCheck size={13} /> : <IconTrash size={13} />}
            {batchMode ? '完成' : '批量管理'}
          </button>
          <button className="att-btn" onClick={onOpenRejects} title="查看被拒绝的导入记录">
            <IconInfo size={13} />
            拒绝记录
            {rejectTotal > 0 && (
              <span className="att-badge">{rejectTotal > 99 ? '99+' : rejectTotal}</span>
            )}
          </button>
        </div>
      </div>
      {batchMode && (
        <div className="att-batch-bar">
          <span className="att-batch-count">已选 {selectedCount} 个</span>
          <button className="att-link" onClick={onSelectAll} disabled={shownCount === 0}>
            全选当前筛选({shownCount})
          </button>
          <button className="att-link" onClick={onClearSelect} disabled={selectedCount === 0}>
            取消选择
          </button>
          <span className="sp" />
          <button
            className="att-btn danger"
            disabled={selectedCount === 0 || deleting}
            onClick={onDeleteSelected}
          >
            <IconTrash size={13} />
            {deleting ? '清理中…' : `删除所选(${selectedCount})`}
          </button>
        </div>
      )}
    </>
  );
}
