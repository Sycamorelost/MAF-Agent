import { IconChevronRight, IconFolder, IconFolderOpen, IconMore } from '../icons';
import type { ProjectGroup } from './types';
import type { ProjectRow, SessionRow } from '../../lib/ipc';
import { SessionRowItem } from './SessionRow';

/** 项目区块:每个项目绑定一个本地工作文件夹,项目与其会话均可折叠。 */
interface Props {
  projectGroups: ProjectGroup[];
  collapsed: Record<string, boolean>;
  emptyText?: string;
  activeId: string | undefined;
  /** 当前会话所属项目,用于高亮 */
  activeProjectId?: string | null;
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
  collapsed,
  emptyText = '没有匹配的项目或会话。',
  activeId,
  activeProjectId = null,
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
  if (projectGroups.length === 0) {
    return <div className="side-empty">{emptyText}</div>;
  }

  return (
    <>
      {projectGroups.map((g) => {
        const isCollapsed = collapsed[g.project.id] ?? false;
        const active = g.project.id === activeProjectId;
        return (
          <div key={g.project.id} className="side-proj-group">
            <div
              className={`side-proj-head${active ? ' on' : ''}`}
              onClick={() => onToggleCollapsed(g.project.id)}
              onContextMenu={(e) => onProjectContextMenu(e, g.project)}
              title={g.project.path}
            >
              {isCollapsed ? <IconFolder size={14} /> : <IconFolderOpen size={14} />}
              <span className="proj-main">
                <span className="proj-name">{g.project.name || '(未命名项目)'}</span>
              </span>
              <button
                type="button"
                className="proj-more"
                title="项目操作"
                aria-label={`「${g.project.name}」项目操作`}
                onClick={(e) => {
                  e.stopPropagation();
                  onProjectContextMenu(e, g.project);
                }}
              >
                <IconMore size={13} />
              </button>
              <IconChevronRight
                size={12}
                className={`chevron${!isCollapsed ? ' down' : ''}`}
              />
            </div>
            {/* 会话列表:抽屉动画展开/收起(与侧栏同一套时长/缓动) */}
            <div className={`proj-drawer${isCollapsed ? ' closed' : ''}`}>
              <div className="proj-drawer-inner">
                {g.items.length === 0 && (
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
                {g.items.map((s) => (
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
            </div>
          </div>
        );
      })}
    </>
  );
}
