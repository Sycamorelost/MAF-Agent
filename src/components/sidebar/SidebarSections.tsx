import type { NoteMeta, ProjectRow, SessionRow } from '../../lib/ipc';
import { whenLabel } from './utils';
import type { SideSection } from './utils';
import type { ProjectGroup } from './types';
import { IconChevronRight, IconPlus } from '../icons';
import { NavRail } from './NavRail';
import { ProjectListView } from './ProjectList';
import { SectionFlatList } from './SectionFlatList';
import { TimeListView } from './TimeList';

/** 顶部固定区:新对话按钮,全宽胶囊 + 贴顶部 + 右侧与会话区同高 */
export function SidebarHeader({ onNewSession }: { onNewSession: () => void }) {
  return (
    <div className="side-top">
      <button
        type="button"
        className="side-new"
        title="新建会话"
        onClick={onNewSession}
      >
        <IconPlus size={15} />
        <span>新对话</span>
      </button>
    </div>
  );
}

/** 侧栏滚动区:导航 + 项目/会话/记忆库/知识库列表(纯展示,状态与回调由 Sidebar 传入) */
interface Props {
  section: SideSection;
  projects: ProjectRow[];
  projectGroups: ProjectGroup[];
  uncatItems: SessionRow[];
  sessions: SessionRow[];
  notes: NoteMeta[];
  collapsed: Record<string, boolean>;
  projectsExpanded: boolean;
  sessionsOpen: boolean;
  activeId: string | undefined;
  activeProjectId: string | null;
  activeMemorySession: string | null;
  activeNoteId: string | null;
  editingSessionId: string | null;
  editingSessionText: string;
  onToggleProjects: () => void;
  onToggleSessions: () => void;
  onToggleProjectCollapsed: (projectId: string) => void;
  onNewProject: () => void;
  onNewSession: (projectId: string | null) => void;
  onNewNote: () => void;
  onNavigateSession: (id: string) => void;
  onProjectContextMenu: (e: React.MouseEvent, p: ProjectRow) => void;
  onSessionContextMenu: (e: React.MouseEvent, s: SessionRow) => void;
  onSessionStartRename: (s: SessionRow) => void;
  onEditingTextChange: (t: string) => void;
  onCommitRename: () => void;
  onCancelRename: () => void;
  onSelectMemory: (id: string) => void;
  onSelectNote: (id: string) => void;
}

export function SidebarSections({
  section,
  projects,
  projectGroups,
  uncatItems,
  sessions,
  notes,
  collapsed,
  projectsExpanded,
  sessionsOpen,
  activeId,
  activeProjectId,
  activeMemorySession,
  activeNoteId,
  editingSessionId,
  editingSessionText,
  onToggleProjects,
  onToggleSessions,
  onToggleProjectCollapsed,
  onNewProject,
  onNewSession,
  onNewNote,
  onNavigateSession,
  onProjectContextMenu,
  onSessionContextMenu,
  onSessionStartRename,
  onEditingTextChange,
  onCommitRename,
  onCancelRename,
  onSelectMemory,
  onSelectNote,
}: Props) {
  // 会话分区:项目列表常驻可折叠;无项目时仍显示「会话」分组承载全部会话
  const showSessions = uncatItems.length > 0 || projects.length === 0;

  return (
    <>
      <NavRail />
      {section === 'chat' && (
        <div className="side-proj-sec">
          <div className="side-sec-head">
            <button
              type="button"
              className="side-sec-toggle"
              aria-expanded={projectsExpanded}
              aria-label={projectsExpanded ? '折叠项目' : '展开项目'}
              onClick={onToggleProjects}
            >
              <IconChevronRight
                size={12}
                className={`chevron${projectsExpanded ? ' down' : ''}`}
              />
              <span className="side-sec-title">项目</span>
              {projects.length > 0 && (
                <span className="side-sec-count">{projects.length}</span>
              )}
            </button>
            <div className="side-head-actions">
              <button
                className="panel-toggle"
                title="新建项目(绑定本地工作文件夹)"
                aria-label="新建项目"
                onClick={onNewProject}
              >
                <IconPlus size={14} />
              </button>
            </div>
          </div>
          {projectsExpanded && (
            <div className="side-list side-proj-list">
              {projects.length === 0 ? (
                <div className="side-empty">
                  还没有项目 ·{' '}
                  <button className="link-btn" onClick={onNewProject}>
                    选择工作文件夹
                  </button>
                </div>
              ) : (
                <ProjectListView
                  projectGroups={projectGroups}
                  collapsed={collapsed}
                  activeId={activeId}
                  activeProjectId={activeProjectId}
                  editingId={editingSessionId}
                  editingText={editingSessionText}
                  onToggleCollapsed={onToggleProjectCollapsed}
                  onNewSessionInProject={onNewSession}
                  onProjectContextMenu={onProjectContextMenu}
                  onSessionNavigate={onNavigateSession}
                  onSessionStartRename={onSessionStartRename}
                  onEditingTextChange={onEditingTextChange}
                  onCommitRename={onCommitRename}
                  onCancelRename={onCancelRename}
                  onSessionContextMenu={onSessionContextMenu}
                />
              )}
            </div>
          )}
        </div>
      )}

      {section === 'chat' && showSessions && (
        <div className="side-sec">
          <div className="side-sec-head">
            <button
              type="button"
              className="side-sec-toggle"
              aria-expanded={sessionsOpen}
              aria-label={sessionsOpen ? '折叠会话' : '展开会话'}
              onClick={onToggleSessions}
            >
              <IconChevronRight
                size={12}
                className={`chevron${sessionsOpen ? ' down' : ''}`}
              />
              <span className="side-sec-title">会话</span>
              {uncatItems.length > 0 && (
                <span className="side-sec-count">{uncatItems.length}</span>
              )}
            </button>
          </div>
          {sessionsOpen && (
            <div className="side-list">
              <TimeListView
                sessions={uncatItems}
                activeId={activeId}
                editingId={editingSessionId}
                editingText={editingSessionText}
                onSessionNavigate={onNavigateSession}
                onSessionStartRename={onSessionStartRename}
                onEditingTextChange={onEditingTextChange}
                onCommitRename={onCommitRename}
                onCancelRename={onCancelRename}
                onSessionContextMenu={onSessionContextMenu}
              />
            </div>
          )}
        </div>
      )}

      {section === 'memory' && (
        <SectionFlatList
          title="记忆库"
          rows={sessions.map((s) => ({
            id: s.id,
            title: s.title || '未命名会话',
            when: whenLabel(s.updated),
          }))}
          activeId={activeMemorySession}
          emptyText="还没有会话记忆,先去对话页聊一句吧。"
          onSelect={onSelectMemory}
        />
      )}

      {section === 'knowledge' && (
        <SectionFlatList
          title="知识库"
          rows={notes.map((n) => ({
            id: n.id,
            title: n.title || '未命名笔记',
            when: whenLabel(n.updated),
          }))}
          activeId={activeNoteId}
          emptyText="知识库还是空的,在主区域新建一篇笔记吧。"
          headActions={
            <button
              className="panel-toggle"
              title="新建笔记"
              aria-label="新建笔记"
              onClick={onNewNote}
            >
              <IconPlus size={14} />
            </button>
          }
          onSelect={onSelectNote}
        />
      )}
    </>
  );
}
