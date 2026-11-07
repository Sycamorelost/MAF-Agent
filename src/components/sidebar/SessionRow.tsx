import { useRef } from 'react';
import type { SessionRow } from '../../lib/ipc';
import { whenLabel } from './utils';

/** Sidebar 单行会话项(含内联重命名) */
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
}: Props) {
  const inputRef = useRef<HTMLInputElement>(null);
  const isActive = session.id === activeId;
  const isEditing = editingId === session.id;

  return (
    <div
      className={`side-item ${indentClass}${isActive ? ' on' : ''}`}
      onClick={() => !isEditing && onNavigate(session.id)}
      onDoubleClick={() => onStartRename(session)}
      onContextMenu={(e) => onContextMenu(e, session)}
      title={`${session.title}\n双击重命名 · 右键菜单`}
    >
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
