import { IconChevronRight, IconFolder, IconFolderOpen } from '../icons';
import type { ProjectGroup } from './types';
import type { ProjectRow, SessionRow } from '../../lib/ipc';
import { SessionRowItem } from './SessionRow';

/** 按项目视图:项目分组 + 未分类会话 */
interface Props {
  projectGroups: ProjectGroup[];
  uncatItems: SessionRow[];
  allProjects: ProjectRow[];
  collapsed: Record<string, boolean>;
  activeId: string | undefined;
  editingId: string | null;
  editingText: string;
  onToggleCollapsed: (projectId: string) => void;
  onNewSessionInProject: (projectId: string | null) => void;
  onProjectContextMenu: (e: React.MouseEvent, p: ProjectRow) => void;
  onSessionNavigate: (id: string) => void;
  onSessionStartRename: (s: SessionRow) => void;
  onEditingTextChange: (t: string) => void;
  onCommitRename: () => void;
  onCancelRename: () => void;
  onSessionContextMenu: (e: React.MouseEvent, s: SessionRow) => void;
}

export function ProjectListView({
  projectGroups,
  uncatItems,
  collapsed,
  activeId,
  editingId,
  editingText,
  onToggleCollapsed,
  onNewSessionInProject,
  onProjectContextMenu,
  onSessionNavigate,
  onSessionStartRename,
  onEditingTextChange,
  onCommitRename,
  onCancelRename,
  onSessionContextMenu,
}: Props) {
  const hasProject = projectGroups.length > 0;
  const hasUncat = uncatItems.length > 0;
  const total = projectGroups.reduce((n, g) => n + g.items.length, 0) + uncatItems.length;

  if (total === 0) {
    return <div className="side-empty">还没有会话,点上方「+」开始。</div>;
  }

  return (
    <>
      {/* 项目分组 */}
      {projectGroups.map((g) => {
        const isCollapsed = collapsed[g.project.id] ?? false;
        return (
          <div key={g.project.id} className="side-proj-group">
            <div
              className="side-proj-head"
              onClick={() => onToggleCollapsed(g.project.id)}
              onContextMenu={(e) => onProjectContextMenu(e, g.project)}
              title={`${g.project.path}\n右键:重命名 / 删除项目`}
            >
              {isCollapsed ? <IconFolder size={14} /> : <IconFolderOpen size={14} />}
              <span className="proj-name" title={g.project.path}>
                {g.project.name || '(未命名项目)'}
              </span>
              <IconChevronRight
                size={12}
                className={`chevron${!isCollapsed ? ' down' : ''}`}
              />
            </div>
            {!isCollapsed && g.items.length === 0 && (
              <div className="side-empty-note">
                暂无会话 ·{' '}
                <button
                  className="link-btn"
                  onClick={(e) => {
                    e.stopPropagation();
                    onNewSessionInProject(g.project.id);
                  }}
                >
                  新建
                </button>
              </div>
            )}
            {!isCollapsed &&
              g.items.map((s) => (
                <SessionRowItem
                  key={s.id}
                  session={s}
                  indentClass="side-proj-item"
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
        );
      })}

      {/* 未分类(缩进与项目名同级) */}
      {hasUncat && (
        <>
          {hasProject && <div className="side-uncat-divider" />}
          {uncatItems.map((s) => (
            <SessionRowItem
              key={s.id}
              session={s}
              indentClass="side-uncat-item"
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
        </>
      )}
    </>
  );
}
