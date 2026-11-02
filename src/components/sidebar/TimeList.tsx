import type { ProjectRow, SessionRow } from '../../lib/ipc';
import { SessionRowItem } from './SessionRow';

/** 按时间扁平视图:所有会话平铺,上方显示所属项目标签 */
interface Props {
  sessions: SessionRow[];
  projects: ProjectRow[];
  activeId: string | undefined;
  editingId: string | null;
  editingText: string;
  onSessionNavigate: (id: string) => void;
  onSessionStartRename: (s: SessionRow) => void;
  onEditingTextChange: (t: string) => void;
  onCommitRename: () => void;
  onCancelRename: () => void;
  onSessionContextMenu: (e: React.MouseEvent, s: SessionRow) => void;
}

export function TimeListView({
  sessions,
  projects,
  activeId,
  editingId,
  editingText,
  onSessionNavigate,
  onSessionStartRename,
  onEditingTextChange,
  onCommitRename,
  onCancelRename,
  onSessionContextMenu,
}: Props) {
  const flat = [...sessions].sort((a, b) => b.updated.localeCompare(a.updated));

  if (flat.length === 0) {
    return <div className="side-empty">还没有会话。</div>;
  }

  return (
    <>
      {flat.map((s) => {
        const proj = s.project_id ? projects.find((p) => p.id === s.project_id) : null;
        return (
          <div key={s.id} className="side-flat-row">
            <span className="side-flat-tag">{proj ? proj.name : '未分类'}</span>
            <SessionRowItem
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
          </div>
        );
      })}
    </>
  );
}
