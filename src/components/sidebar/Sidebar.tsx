import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useLocation, useNavigate, useParams } from 'react-router-dom';
import { useContextMenu } from '../../hooks/useContextMenu';
import * as ipc from '../../lib/ipc';
import { renameSession } from '../../lib/ipc/session';
import { sessionPath } from '../../lib/workbench';
import { useSessionsStore } from '../../stores/sessions';
import { useUiStore } from '../../stores/ui';
import type { NoteMeta, ProjectRow, SessionRow } from '../../lib/ipc';
import { useToast } from '../../lib/useToast';
import {
  readCollapsed, writeCollapsed,
  readProjectsOpen, writeProjectsOpen,
  isAbsolutePath,
  sectionFromPath,
  sidebarMode,
} from './utils';
import type { ProjectGroup, DeleteProjectState } from './types';
import { NavRail } from './NavRail';
import { SidebarHeader, SidebarSections } from './SidebarSections';
import { SidebarResize } from './SidebarResize';
import { SidebarOverlays } from './SidebarOverlays';
import { buildProjectMenu, buildSessionMenu, type SidebarMenuContext } from './sidebarMenuItems';

/**
 * Sidebar 主容器(DeepSeek 风格):
 * - 顶部即「新对话」主按钮与搜索(品牌/折叠按钮由标题栏承担,侧栏不再重复)
 * - 一级导航:对话 / 记忆库 / 知识库 / 附件;列表分区随当前导航切换
 *   (对话→项目+会话,记忆库→会话记忆,知识库→笔记;附件点导航直达页面,不再重复列表)
 * - 「项目」区块可折叠,每个项目绑定一个本地工作文件夹;未归项目的会话按时间分组
 * - 设置固定在最底部;与主区同一图层、参与布局(不悬浮),折叠时完全隐藏
 * - 管状态、路由、IPC 调用、生命周期;localStorage 读写集中在 utils.ts
 */
export default function Sidebar() {
  const nav = useNavigate();
  const loc = useLocation();
  const { id: activeId } = useParams();
  const open = useUiStore((s) => s.sidebarOpen);
  const setSidebarOpen = useUiStore((s) => s.setSidebarOpen);
  const width = useUiStore((s) => s.sidebarWidth);
  const setSidebarWidth = useUiStore((s) => s.setSidebarWidth);
  const sessions = useSessionsStore((s) => s.sessions);
  const sessionsStore = useSessionsStore();

  /** 显示模式:展开态与主区同层推挤;折叠时整个节点不渲染(不使用浮层) */
  const mode = sidebarMode(open);
  const visible = mode !== 'hidden';

  /** 侧栏分区随当前路由切换:对话→项目+会话,记忆库→会话记忆,知识库→笔记,附件→附件 */
  const section = sectionFromPath(loc.pathname);

  // ====== 持久化状态 ======
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>(readCollapsed);
  const [projectsOpen, setProjectsOpen] = useState(readProjectsOpen);
  const [sessionsOpen, setSessionsOpen] = useState(true);
  const { toast, showToast, hideToast } = useToast();

  // ====== 分区数据(知识库按需加载) ======
  const [notes, setNotes] = useState<NoteMeta[]>([]);

  // ====== 临时状态 ======
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [editingSessionText, setEditingSessionText] = useState('');
  const [pendingDel, setPendingDel] = useState<SessionRow | null>(null);
  const [pendingProjectDel, setPendingProjectDel] = useState<DeleteProjectState | null>(null);
  const [renamingProject, setRenamingProject] = useState<ProjectRow | null>(null);
  /** 拖拽调宽中:临时关闭抽屉宽度过渡,保证跟手 */
  const [dragging, setDragging] = useState(false);
  /** P16:弹出式搜索面板状态(和 Titlebar 共用 store) */
  const searchOpen = useUiStore((s) => s.searchPaletteOpen);
  const setSearchOpen = useUiStore((s) => s.setSearchPaletteOpen);
  const [searchQuery, setSearchQuery] = useState('');
  const searchInputRef = useRef<HTMLInputElement | null>(null);

  // P16:Esc 关闭搜索面板(Ctrl+K 由 Titlebar 监听)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && searchOpen) {
        setSearchOpen(false);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [searchOpen, setSearchOpen]);

  // P16:搜索面板打开时聚焦输入框
  useEffect(() => {
    if (searchOpen) {
      requestAnimationFrame(() => searchInputRef.current?.focus());
    } else {
      setSearchQuery('');
    }
  }, [searchOpen]);

  const projectMenu = useContextMenu<ProjectRow>();
  const sessionMenu = useContextMenu<SessionRow>();

  // ====== 生命周期 ======
  useEffect(() => {
    document.body.classList.toggle('sidebar-open', mode === 'expanded');
    return () => {
      document.body.classList.remove('sidebar-open');
    };
  }, [mode]);

  // 展开态宽度写入 CSS 变量,主区与侧栏抽屉共用同一值;
  // 抽屉开合由 .side-drawer 的宽度过渡完成,主区随 flex 自动推挤/回弹。
  useEffect(() => {
    document.documentElement.style.setProperty('--sidebar-w', `${width}px`);
  }, [width]);

  useEffect(() => {
    if (!visible) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setSidebarOpen(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [visible, setSidebarOpen]);

  useEffect(() => { writeCollapsed(collapsed); }, [collapsed]);
  useEffect(() => { writeProjectsOpen(projectsOpen); }, [projectsOpen]);

  const refreshProjects = useCallback(async () => {
    try { setProjects(await ipc.listProjects()); } catch { /* ignore */ }
  }, []);

  useEffect(() => { if (visible) void refreshProjects(); }, [visible, refreshProjects]);

  // 进入知识库分区时加载笔记列表(切走不重复请求;回到分区时刷新);
  // 页面内新建/保存/删除/导入后广播 notes_changed,此处同步刷新。
  // 2s 内的重复进入/事件复用上次结果,避免频繁切分区导致的重复全量扫盘。
  const notesFetchedAt = useRef(0);
  useEffect(() => {
    if (!visible || section !== 'knowledge') return;
    const reload = (force = false) => {
      const now = Date.now();
      if (!force && now - notesFetchedAt.current < 2000) return;
      notesFetchedAt.current = now;
      void ipc.notesList().then(setNotes).catch(() => setNotes([]));
    };
    reload();
    const onChanged = () => reload(true);
    window.addEventListener('notes_changed', onChanged);
    return () => window.removeEventListener('notes_changed', onChanged);
  }, [visible, section]);

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

  /** 当前会话所属项目:用于高亮项目行。 */
  const activeProjectId = useMemo(
    () => sessions.find((s) => s.id === activeId)?.project_id ?? null,
    [sessions, activeId],
  );

  /** 记忆库/知识库当前选中项(来自地址参数,用于侧栏高亮)。 */
  const locParams = useMemo(() => new URLSearchParams(loc.search), [loc.search]);
  const activeMemorySession = locParams.get('session');
  const activeNoteId = locParams.get('id');

  // ====== 操作:新建 ======
  /** 新建空笔记并跳转知识库编辑器(页内保存后广播 notes_changed,列表随之刷新)。 */
  const handleNewNote = useCallback(async () => {
    try {
      const doc = await ipc.notesSave(null, '', [], '');
      setNotes(await ipc.notesList().catch(() => []));
      nav(`/knowledge?id=${doc.id}`);
    } catch (e) {
      showToast(`新建笔记失败:${String(e)}`);
    }
  }, [nav, showToast]);

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

  /** 选择本地文件夹并绑定为项目(新建或更换)。 */
  const pickProjectPath = useCallback(async (): Promise<string | null> => {
    const picked = await ipc.fsPickWorkspaceDir();
    if (!picked) return null;
    // 去掉 Windows 长路径前缀;跨平台只要求绝对路径(Windows 盘符 / POSIX 根 / UNC)
    const cleanPath = picked.replace(/^\\\\\?\\/, '');
    return isAbsolutePath(cleanPath) ? cleanPath : null;
  }, []);

  const handleNewProject = useCallback(async () => {
    try {
      const path = await pickProjectPath();
      if (!path) return;
      await ipc.createProject('', path);
      setProjectsOpen(true);
      await refreshProjects();
    } catch (e) {
      showToast(`创建项目失败:${String(e)}`);
    }
  }, [pickProjectPath, refreshProjects, showToast]);

  const handleChangeProjectPath = useCallback(
    async (p: ProjectRow) => {
      try {
        const path = await pickProjectPath();
        if (!path) return;
        await ipc.updateProjectPath(p.id, path);
        await refreshProjects();
        showToast(`已重新绑定「${p.name}」的工作文件夹`);
      } catch (e) {
        showToast(`更新工作文件夹失败:${String(e)}`);
      }
    },
    [pickProjectPath, refreshProjects, showToast],
  );

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

  // ====== 右键菜单上下文 ======
  /** 菜单构建所需的项目/会话与回调;状态仍由本组件持有。 */
  const menuCtx: SidebarMenuContext = {
    projects,
    projectGroups: grouped.projectGroups,
    onNewSession: (pid) => void handleNewSession(pid),
    onChangeProjectPath: (p) => void handleChangeProjectPath(p),
    onStartRenameProject: startRenameProject,
    onRequestDeleteProject: setPendingProjectDel,
    onStartRenameSession: startRenameSession,
    onRequestDeleteSession: setPendingDel,
    onSetSessionProject: (sid, pid) => void sessionsStore.setProject(sid, pid),
    onNavigateToSession: (sid) => nav(sessionPath(sid)),
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

  // 折叠态仍保留抽屉节点(宽度收拢到 0)以播放关闭动画;
  // .closed 里的 visibility:hidden 保证收起后不可聚焦、不可交互。
  return (
    <>
      <div
        className={`side-drawer ${visible ? 'open' : 'closed'}${dragging ? ' dragging' : ''}`}
        aria-hidden={!visible}
      >
        <aside className="sidebar" aria-label="侧边栏">
          <SidebarHeader onNewSession={() => void handleNewSession(null)} />

          {/* 滚动区:NavRail + 列表 */}
          <div className="side-scroll">
            <SidebarSections
              section={section}
              projects={projects}
              projectGroups={grouped.projectGroups}
              uncatItems={grouped.uncatItems}
              sessions={sessions}
              notes={notes}
              collapsed={collapsed}
              projectsExpanded={projectsOpen}
              sessionsOpen={sessionsOpen}
              activeId={activeId}
              activeProjectId={activeProjectId}
              activeMemorySession={activeMemorySession}
              activeNoteId={activeNoteId}
              editingSessionId={editingSessionId}
              editingSessionText={editingSessionText}
              onToggleProjects={() => setProjectsOpen((v) => !v)}
              onToggleSessions={() => setSessionsOpen((v) => !v)}
              onToggleProjectCollapsed={(pid) =>
                setCollapsed((prev) => ({ ...prev, [pid]: !(prev[pid] ?? false) }))}
              onNewProject={() => void handleNewProject()}
              onNewSession={(pid) => void handleNewSession(pid)}
              onNewNote={() => void handleNewNote()}
              onNavigateSession={(id) => nav(sessionPath(id))}
              onProjectContextMenu={(e, p) => projectMenu.open(e, p)}
              onSessionContextMenu={(e, s) => sessionMenu.open(e, s)}
              onSessionStartRename={startRenameSession}
              onEditingTextChange={setEditingSessionText}
              onCommitRename={commitRenameSession}
              onCancelRename={cancelRenameSession}
              onSelectMemory={(id) => nav(`/memory?tab=native&session=${id}`)}
              onSelectNote={(id) => nav(`/knowledge?id=${id}`)}
            />
          </div>

          {/* 底部固定区:设置按钮,右对齐贴底 */}
          <div className="side-bottom">
            <NavRail foot />
          </div>

          <SidebarResize
            visible={visible}
            width={width}
            onWidth={setSidebarWidth}
            onDraggingChange={setDragging}
          />
        </aside>
      </div>

      <SidebarOverlays
        projectMenu={projectMenu}
        sessionMenu={sessionMenu}
        pendingDel={pendingDel}
        pendingProjectDel={pendingProjectDel}
        renamingProject={renamingProject}
        onBuildProjectMenu={(p) => buildProjectMenu(p, menuCtx)}
        onBuildSessionMenu={(s) => buildSessionMenu(s, menuCtx)}
        onCancelDeleteSession={() => setPendingDel(null)}
        onConfirmDeleteSession={confirmDeleteSession}
        onCancelDeleteProject={() => setPendingProjectDel(null)}
        onConfirmDeleteProject={confirmDeleteProject}
        onCancelRenameProject={() => setRenamingProject(null)}
        onConfirmRenameProject={confirmRenameProject}
        toast={toast}
        onCloseToast={hideToast}
        searchOpen={searchOpen}
        searchQuery={searchQuery}
        onSearchQuery={setSearchQuery}
        sessions={sessions}
        onCloseSearch={() => setSearchOpen(false)}
        searchInputRef={searchInputRef}
      />
    </>
  );
}
