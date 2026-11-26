import type { SessionRow } from '../../lib/ipc';
import { SessionRowItem } from './SessionRow';
import type { SelectApi } from './types';

/** P0:扁平列表,去掉「今天/昨天/7 天内」等时间分组 —— 用户要求删除分组,直接按更新时间倒序 */
interface Props {
  sessions: SessionRow[];
  activeId: string | undefined;
  editingId: string | null;
  editingText: string;
  emptyText?: string;
  /** 多选状态与操作(可选) */
  sel?: SelectApi;
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
  sel,
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
      {flat.map((s) => (
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
          selectMode={sel?.mode}
          selected={sel?.selected.has(s.id)}
          onToggleSelect={sel?.toggle}
        />
      ))}
    </>
  );
}
