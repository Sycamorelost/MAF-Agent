import type { SessionRow } from '../../lib/ipc';
import { SessionRowItem } from './SessionRow';
import { groupSessionsByTime } from './utils';

/** 按时间分组视图(今天/昨天/7 天内/30 天内/更早),与 DeepSeek 侧栏一致。 */
interface Props {
  sessions: SessionRow[];
  activeId: string | undefined;
  editingId: string | null;
  editingText: string;
  emptyText?: string;
  onSessionNavigate: (id: string) => void;
  onSessionStartRename: (s: SessionRow) => void;
  onEditingTextChange: (t: string) => void;
  onCommitRename: () => void;
  onCancelRename: () => void;
  onSessionContextMenu: (e: React.MouseEvent, s: SessionRow) => void;
}

export function TimeListView({
  sessions,
  activeId,
  editingId,
  editingText,
  emptyText = '还没有会话。',
  onSessionNavigate,
  onSessionStartRename,
  onEditingTextChange,
  onCommitRename,
  onCancelRename,
  onSessionContextMenu,
}: Props) {
  const flat = [...sessions].sort((a, b) => b.updated.localeCompare(a.updated));

  if (flat.length === 0) {
    return <div className="side-empty">{emptyText}</div>;
  }

  return (
    <>
      {groupSessionsByTime(flat).map((group) => (
        <div key={group.key} className="side-time-group">
          <div className="side-group-label">{group.label}</div>
          {group.items.map((s) => (
            <SessionRowItem
              key={s.id}
              session={s}
              indentClass="side-flat-item"
              activeId={activeId}
              editingId={editingId}
              editingText={editingText}
              onNavigate={onSessionNavigate}
              onStartRename={onSessionStartRename}
              onEditingTextChange={onEditingTextChange}
              onCommitRename={onCommitRename}
              onCancelRename={onCancelRename}
              onContextMenu={onSessionContextMenu}
            />
          ))}
        </div>
      ))}
    </>
  );
}
