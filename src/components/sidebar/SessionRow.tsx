import { useRef } from 'react';
import type { SessionRow } from '../../lib/ipc';
import { whenLabel } from './utils';

/** Sidebar 单行会话项(含内联重命名 + 多选勾选框) */
interface Props {
  activeId: string | undefined;
  session: SessionRow;
  indentClass: string;
  editingId: string | null;
  editingText: string;
  onNavigate: (id: string) => void;
  onStartRename: (s: SessionRow) => void;
  onEditingTextChange: (t: string) => void;
  onCommitRename: () => void;
  onCancelRename: () => void;
  onContextMenu: (e: React.MouseEvent, s: SessionRow) => void;
  /** 多选模式:点击改为切换选中,不再跳转/重命名 */
  selectMode?: boolean;
  selected?: boolean;
  onToggleSelect?: (id: string) => void;
}

export function SessionRowItem({
  activeId,
  session,
  indentClass,
  editingId,
  editingText,
  onNavigate,
  onStartRename,
  onEditingTextChange,
  onCommitRename,
  onCancelRename,
  onContextMenu,
  selectMode = false,
  selected = false,
  onToggleSelect,
}: Props) {
  const inputRef = useRef<HTMLInputElement>(null);
  const isActive = session.id === activeId;
  const isEditing = editingId === session.id;

  const onClick = () => {
    if (isEditing) return;
    if (selectMode) onToggleSelect?.(session.id);
    else onNavigate(session.id);
  };

  return (
    <div
      className={`side-item ${indentClass}${isActive && !selectMode ? ' on' : ''}${selected ? ' picked' : ''}${session.archived ? ' archived' : ''}`}
      onClick={onClick}
      onDoubleClick={() => !selectMode && onStartRename(session)}
      onContextMenu={(e) => onContextMenu(e, session)}
      title={
        selectMode
          ? `${session.title}\n点击切换选中`
          : `${session.title}\n双击重命名 · 右键菜单`
      }
    >
      {selectMode && (
        <span className={`side-check${selected ? ' on' : ''}`} aria-hidden="true">
          {selected && '✓'}
        </span>
      )}
      {isEditing ? (
        <input
          ref={inputRef}
          className="side-rename"
          value={editingText}
          autoFocus
          onFocus={(e) => e.target.select()}
          onChange={(e) => onEditingTextChange(e.target.value)}
          onBlur={onCommitRename}
          onKeyDown={(e) => {
            if (e.key === 'Enter') onCommitRename();
            if (e.key === 'Escape') onCancelRename();
          }}
          onClick={(e) => e.stopPropagation()}
        />
      ) : (
        <>
          <span className="txt">{session.title || '未命名会话'}</span>
          <span className="when">{whenLabel(session.updated)}</span>
        </>
      )}
    </div>
  );
}
