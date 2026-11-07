import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useLocation, useNavigate, useParams } from 'react-router-dom';
import type { MenuItem } from '../ContextMenu';
import { useContextMenu } from '../../hooks/useContextMenu';
import * as ipc from '../../lib/ipc';
import { cloneSession, renameSession } from '../../lib/ipc/session';
import { sessionPath } from '../../lib/workbench';
import { useSessionsStore } from '../../stores/sessions';
import { useUiStore } from '../../stores/ui';
import type { NoteMeta, ProjectRow, SessionRow } from '../../lib/ipc';
import { IconChevronRight, IconPlus } from '../icons';
import Toast from '../Toast';
import { useToast } from '../../lib/useToast';
import {
  readCollapsed, writeCollapsed,
  readProjectsOpen, writeProjectsOpen,
  clampSidebarWidth, defaultSidebarWidth,
  isAbsolutePath,
  sectionFromPath,
  sidebarMode,
  whenLabel,
} from './utils';
import type { ProjectGroup, DeleteProjectState } from './types';
import { NavRail } from './NavRail';
import { ProjectListView } from './ProjectList';
import { SectionFlatList } from './SectionFlatList';
import { TimeListView } from './TimeList';
import { SidebarMenus } from './Menus';

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

  // ====== 宽度拖拽 ======
  const dragRef = useRef<{ startX: number; startW: number } | null>(null);
  const onResizeDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!visible) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    dragRef.current = { startX: e.clientX, startW: width };
    setDragging(true);
  };
  const onResizeMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const d = dragRef.current;
    if (!d) return;
    setSidebarWidth(clampSidebarWidth(d.startW + e.clientX - d.startX));
  };
  const onResizeEnd = () => {
    dragRef.current = null;
    setDragging(false);
  };

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

  // ====== 右键菜单构建 ======
  const buildProjectMenu = (p: ProjectRow): MenuItem[] => [
    { key: 'new-here', label: `在「${p.name}」中新建会话`,
      onClick: () => void handleNewSession(p.id) },
    { divider: true, key: 's1' },
    { key: 'open-folder', label: '打开工作文件夹',
      onClick: () => { if (p.path) void ipc.fsOpen(p.path); } },
    { key: 'change-folder', label: '更换工作文件夹…',
      onClick: () => void handleChangeProjectPath(p) },
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

  // 折叠态仍保留抽屉节点(宽度收拢到 0)以播放关闭动画;
  // .closed 里的 visibility:hidden 保证收起后不可聚焦、不可交互。

  // 会话分区:项目列表常驻可折叠;无项目时仍显示「会话」分组承载全部会话
  const showSessions = grouped.uncatItems.length > 0 || projects.length === 0;
  const projectsExpanded = projectsOpen;

  return (
    <>
      <div
        className={`side-drawer ${visible ? 'open' : 'closed'}${dragging ? ' dragging' : ''}`}
        aria-hidden={!visible}
      >
        <aside className="sidebar" aria-label="侧边栏">
        <button
          type="button"
          className="side-new"
          title="新建会话"
          onClick={() => void handleNewSession(null)}
        >
          <IconPlus size={15} />
          <span>新对话</span>
        </button>

        <NavRail />

        {/* 新建对话固定在上方;导航以下的全部列表共用一个滚动容器 */}
        <div className="side-scroll">
        {section === 'chat' && (
          <div className="side-proj-sec">
            <div className="side-sec-head">
              <button
                type="button"
                className="side-sec-toggle"
                aria-expanded={projectsExpanded}
                aria-label={projectsExpanded ? '折叠项目' : '展开项目'}
                onClick={() => setProjectsOpen((v) => !v)}
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
                  onClick={() => void handleNewProject()}
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
                    <button className="link-btn" onClick={() => void handleNewProject()}>
                      选择工作文件夹
                    </button>
                  </div>
                ) : (
                  <ProjectListView
                    projectGroups={grouped.projectGroups}
                    collapsed={collapsed}
                    activeId={activeId}
                    activeProjectId={activeProjectId}
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
                )}
              </div>
            )}
          </div>
        )}

        {section === 'chat' && showSessions && (
          <div className="side-sec">
            <div className="side-sec-head">
              <span className="side-sec-title">会话</span>
              {grouped.uncatItems.length > 0 && (
                <span className="side-sec-count">{grouped.uncatItems.length}</span>
              )}
            </div>
            <div className="side-list">
              <TimeListView
                sessions={grouped.uncatItems}
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
            </div>
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
            onSelect={(id) => nav(`/memory?tab=native&session=${id}`)}
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
                onClick={() => void handleNewNote()}
              >
                <IconPlus size={14} />
              </button>
            }
            onSelect={(id) => nav(`/knowledge?id=${id}`)}
          />
        )}

        </div>

        <NavRail foot />

        <div
          className="side-resize"
          role="separator"
          aria-orientation="vertical"
          aria-label="拖拽调整侧栏宽度"
          title="拖拽调整宽度,双击复位"
          onPointerDown={onResizeDown}
          onPointerMove={onResizeMove}
          onPointerUp={onResizeEnd}
          onPointerCancel={onResizeEnd}
          onDoubleClick={() => setSidebarWidth(defaultSidebarWidth())}
        />
        </aside>
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
      <Toast text={toast} onClose={hideToast} />
    </>
  );
}
