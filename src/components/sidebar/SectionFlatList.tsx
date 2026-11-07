import type { ReactNode } from 'react';

export interface FlatRow {
  id: string;
  title: string;
  /** 相对时间等次要信息 */
  when: string;
}

/**
 * 侧栏「记忆库 / 知识库」分区的扁平列表(分区头 + 标题/时间行),
 * 两个分区结构与样式完全一致,收敛到同一组件避免重复。
 */
export function SectionFlatList(props: {
  title: string;
  rows: FlatRow[];
  activeId: string | null;
  emptyText: string;
  headActions?: ReactNode;
  onSelect: (id: string) => void;
}) {
  const { title, rows, activeId, emptyText, headActions, onSelect } = props;
  return (
    <div className="side-sec">
      <div className="side-sec-head">
        <span className="side-sec-title">{title}</span>
        {rows.length > 0 && <span className="side-sec-count">{rows.length}</span>}
        {headActions && <div className="side-head-actions">{headActions}</div>}
      </div>
      <div className="side-list">
        {rows.length === 0 ? (
          <div className="side-empty">{emptyText}</div>
        ) : (
          rows.map((r) => (
            <div
              key={r.id}
              className={`side-item side-flat-item${activeId === r.id ? ' on' : ''}`}
              title={r.title}
              onClick={() => onSelect(r.id)}
            >
              <span className="txt">{r.title}</span>
              <span className="when">{r.when}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
