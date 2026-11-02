import { useCallback, useEffect, useMemo, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import type { MenuItem } from '../ContextMenu';
import { useContextMenu } from '../../hooks/useContextMenu';
import * as ipc from '../../lib/ipc';
import { cloneSession, renameSession } from '../../lib/ipc/session';
import { sessionPath } from '../../lib/workbench';
import { useSessionsStore } from '../../stores/sessions';
import { useUiStore } from '../../stores/ui';
import type { ProjectRow, SessionRow } from '../../lib/ipc';
import {
  readCollapsed, writeCollapsed,
  readView, writeView,
  type ViewMode,
} from './utils';
import type { ProjectGroup, DeleteProjectState } from './types';
import { SidebarHeader } from './Header';
import { ProjectListView } from './ProjectList';
import { TimeListView } from './TimeList';
import { SidebarMenus } from './Menus';

/**
 * Sidebar 主容器:
 * - 管状态、路由、IPC 调用、生命周期
 * - 子组件按职责拆分:Header / SessionRow / ProjectList / TimeList / Menus
 * - 所有 localStorage 读写集中在 utils.ts
 */
export default function Sidebar() {
  const nav = useNavigate();
  const { id: activeId } = useParams();
  const open = useUiStore((s) => s.sidebarOpen);
  const setSidebarOpen = useUiStore((s) => s.setSidebarOpen);
  const sessions = useSessionsStore((s) => s.sessions);
  const sessionsStore = useSessionsStore();

  // ====== 持久化状态 ======
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>(readCollapsed);
  const [viewMode, setViewMode] = useState<ViewMode>(readView);

  // ====== 临时状态 ======
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [editingSessionText, setEditingSessionText] = useState('');
  const [pendingDel, setPendingDel] = useState<SessionRow | null>(null);
  const [pendingProjectDel, setPendingProjectDel] = useState<DeleteProjectState | null>(null);
  const [renamingProject, setRenamingProject] = useState<ProjectRow | null>(null);

  const projectMenu = useContextMenu<ProjectRow>();
  const sessionMenu = useContextMenu<SessionRow>();

  // ====== 生命周期 ======
  useEffect(() => {
    document.body.classList.toggle('sidebar-open', open);
    return () => document.body.classList.remove('sidebar-open');
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setSidebarOpen(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, setSidebarOpen]);

  useEffect(() => { writeCollapsed(collapsed); }, [collapsed]);
  useEffect(() => { writeView(viewMode); }, [viewMode]);

  const refreshProjects = useCallback(async () => {
    try { setProjects(await ipc.listProjects()); } catch { /* ignore */ }
  }, []);

  useEffect(() => { if (open) void refreshProjects(); }, [open, refreshProjects]);

  // ====== 分组逻辑(纯计算) ======
  const grouped = useMemo(() => {
    const byProject = new Map<string, SessionRow[]>();
    const uncategorized: SessionRow[] = [];
    for (const s of sessions) {
      if (s.project_id) {
        const arr = byProject.get(s.project_id) ?? [];
        arr.push(s);
        byProject.set(s.project_id, arr);
      } else {
        uncategorized.push(s);
      }
    }
    const projectGroups: ProjectGroup[] = projects
      .map((p) => ({
        kind: 'project' as const,
        project: p,
        items: (byProject.get(p.id) ?? []).sort((a, b) => b.updated.localeCompare(a.updated)),
      }));
    const uncatItems = uncategorized.sort((a, b) => b.updated.localeCompare(a.updated));
    return { projectGroups, uncatItems };
  }, [sessions, projects]);

  // ====== 操作:新建 ======
  const handleNewSession = useCallback(
    async (projectId: string | null) => {
      try {
        void sessionsStore.pruneEmpty(60);
        const title = `新会话 ${new Date().toLocaleTimeString('zh-CN', {
          hour: '2-digit', minute: '2-digit',
        })}`;
        const id = await sessionsStore.create(title, '', null);
        if (projectId) await sessionsStore.setProject(id, projectId);
        nav(sessionPath(id));
      } catch { /* ignore */ }
    },
    [sessionsStore, nav],
  );

  const handleNewProject = useCallback(async () => {
    try {
      const picked = await ipc.fsPickWorkspaceDir();
      if (!picked) return;
      const cleanPath = picked.replace(/^\\\\\?\\/, '');
      if (!cleanPath || !/^[A-Za-z]:\\/.test(cleanPath)) return;
      await ipc.createProject('', cleanPath);
      await refreshProjects();
    } catch { /* ignore */ }
  }, [refreshProjects]);

  // ====== 重命名 ======
  const startRenameSession = (s: SessionRow) => {
    setEditingSessionId(s.id);
    setEditingSessionText(s.title || '未命名会话');
  };
  const commitRenameSession = async () => {
    if (!editingSessionId) return;
    const t = editingSessionText.trim() || '未命名会话';
    try { await renameSession(editingSessionId, t); } catch { /* ignore */ }
    setEditingSessionId(null);
  };
  const cancelRenameSession = () => setEditingSessionId(null);

  const startRenameProject = (p: ProjectRow) => setRenamingProject(p);
  const confirmRenameProject = (name: string) => {
    const p = renamingProject;
    setRenamingProject(null);
    if (!p || !name.trim() || name === p.name) return;
    void ipc.renameProject(p.id, name.trim()).then(() => void refreshProjects());
  };

  // ====== 右键菜单构建 ======
  const buildProjectMenu = (p: ProjectRow): MenuItem[] => [
    { key: 'new-here', label: `在「${p.name}」中新建会话`,
      onClick: () => void handleNewSession(p.id) },
    { divider: true, key: 's1' },
    { key: 'open-folder', label: '打开文件夹',
      onClick: () => { if (p.path) void ipc.fsOpen(p.path); } },
    { divider: true, key: 's2' },
    { key: 'rename', label: '重命名项目',
      onClick: () => startRenameProject(p) },
    { key: 'delete', label: '删除项目', danger: true,
      onClick: () => {
        const count = grouped.projectGroups.find((g) => g.project.id === p.id)?.items.length ?? 0;
        setPendingProjectDel({ projectId: p.id, label: p.name, count });
      } },
  ];

  const buildSessionMenu = (s: SessionRow): MenuItem[] => {
    const moveTo: MenuItem[] = projects
      .filter((p) => p.id !== s.project_id)
      .map((p) => ({
        key: `move-${p.id}`, label: p.name,
        onClick: () => void sessionsStore.setProject(s.id, p.id),
      }));
    if (moveTo.length > 0) {
      moveTo.push({ divider: true, key: 'move-div' });
      moveTo.push({ key: 'uncat', label: '移出项目',
        onClick: () => void sessionsStore.setProject(s.id, null) });
    }
    return [
      { key: 'rename', label: '重命名', onClick: () => startRenameSession(s) },
      { key: 'clone', label: '克隆会话',
        onClick: () => { void cloneSession(s.id).then((newId) => nav(sessionPath(newId))); } },
      { key: 'move', label: '移动到项目…',
        children: moveTo.length > 0
          ? moveTo
          : [{ key: 'no-proj', label: '(没有其他项目)', disabled: true }] },
      { divider: true, key: 's1' },
      { key: 'delete', label: '删除会话', danger: true,
        onClick: () => setPendingDel(s) },
    ];
  };

  // ====== 删除确认 ======
  const confirmDeleteSession = () => {
    if (!pendingDel) return;
    const id = pendingDel.id;
    setPendingDel(null);
    void sessionsStore.remove(id);
    if (id === activeId) nav('/');
  };
  const confirmDeleteProject = () => {
    if (!pendingProjectDel) return;
    const pid = pendingProjectDel.projectId;
    setPendingProjectDel(null);
    void ipc.deleteProject(pid).then(() => void refreshProjects());
  };

  if (!open) return null;

  return (
    <>
      <div className="sidebar open" role="navigation" aria-label="会话侧栏">
        <SidebarHeader
          viewMode={viewMode}
          onToggleViewMode={() =>
            setViewMode((m) => (m === 'project' ? 'time' : 'project'))}
          onNewSession={() => void handleNewSession(null)}
          onNewProject={() => void handleNewProject()}
          onClose={() => setSidebarOpen(false)}
        />
        <div className="side-list">
          {viewMode === 'project' ? (
            <ProjectListView
              projectGroups={grouped.projectGroups}
              uncatItems={grouped.uncatItems}
              allProjects={projects}
              collapsed={collapsed}
              activeId={activeId}
              editingId={editingSessionId}
              editingText={editingSessionText}
              onToggleCollapsed={(pid) =>
                setCollapsed((prev) => ({ ...prev, [pid]: !(prev[pid] ?? false) }))}
              onNewSessionInProject={(pid) => void handleNewSession(pid)}
              onProjectContextMenu={(e, p) => projectMenu.open(e, p)}
              onSessionNavigate={(id) => nav(sessionPath(id))}
              onSessionStartRename={startRenameSession}
              onEditingTextChange={setEditingSessionText}
              onCommitRename={commitRenameSession}
              onCancelRename={cancelRenameSession}
              onSessionContextMenu={(e, s) => sessionMenu.open(e, s)}
            />
          ) : (
            <TimeListView
              sessions={sessions}
              projects={projects}
              activeId={activeId}
              editingId={editingSessionId}
              editingText={editingSessionText}
              onSessionNavigate={(id) => nav(sessionPath(id))}
              onSessionStartRename={startRenameSession}
              onEditingTextChange={setEditingSessionText}
              onCommitRename={commitRenameSession}
              onCancelRename={cancelRenameSession}
              onSessionContextMenu={(e, s) => sessionMenu.open(e, s)}
            />
          )}
        </div>
      </div>

      <SidebarMenus
        projectMenuPos={projectMenu.pos}
        projectMenuTarget={projectMenu.target}
        sessionMenuPos={sessionMenu.pos}
        sessionMenuTarget={sessionMenu.target}
        pendingDel={pendingDel}
        pendingProjectDel={pendingProjectDel}
        onCloseProjectMenu={projectMenu.close}
        onCloseSessionMenu={sessionMenu.close}
        onBuildProjectMenu={buildProjectMenu}
        onBuildSessionMenu={buildSessionMenu}
        onCancelDeleteSession={() => setPendingDel(null)}
        onConfirmDeleteSession={confirmDeleteSession}
        onCancelDeleteProject={() => setPendingProjectDel(null)}
        onConfirmDeleteProject={confirmDeleteProject}
        renamingProject={renamingProject}
        onCancelRenameProject={() => setRenamingProject(null)}
        onConfirmRenameProject={confirmRenameProject}
      />
    </>
  );
}
