//! 工作台左栏:文件夹分组 + 搜索 + 会话列表 + 右键菜单体系。
//! 核心体验:
//!   · 分组标题旁 [+] 直接在此新建任务(跳过 FolderPicker)
//!   · 右键分组 → 在此新建 / 打开文件夹 / 重命名项目 / 删除项目
//!   · 右键会话项 → 重命名 / 移出项目 / 移动到... / 克隆 / 删除

import { useCallback, useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { ContextMenu, type MenuItem } from '../../../components/ContextMenu';
import { ConfirmDialog } from '../../../components/ConfirmDialog';
import {
  IconChevronRight,
  IconFolder,
  IconFolderOpen,
  IconPlus,
  IconSearch,
  IconX,
} from '../../../components/icons';
import { useContextMenu } from '../../../hooks/useContextMenu';
import { resolveMode, useDefaultWorkbench } from '../../../lib/workbench';
import { useSessionsStore } from '../../../stores/sessions';
import type { ProjectRow, SessionRow } from '../../../lib/ipc';
import * as ipc from '../../../lib/ipc';

interface FolderGroup {
  key: string;
  label: string;
  projectId: string | null;
  path?: string;
  items: SessionRow[];
  collapsed: boolean;
}

function groupByFolder(list: SessionRow[], projects: ProjectRow[]): FolderGroup[] {
  const map = new Map<string | null, SessionRow[]>();
  for (const s of list) {
    const key = s.project_id ?? null;
    const arr = map.get(key);
    if (arr) arr.push(s);
    else map.set(key, [s]);
  }
  const groups: FolderGroup[] = projects.map((f) => ({
    key: `p_${f.id}`,
    label: f.name,
    projectId: f.id,
    path: f.path,
    items: map.get(f.id) ?? [],
    collapsed: false,
  }));
  const uncategorized = map.get(null);
  if (uncategorized && uncategorized.length > 0) {
    groups.push({
      key: 'uncat',
      label: '未分类',
      projectId: null,
      items: uncategorized,
      collapsed: false,
    });
  }
  return groups;
}

function whenLabel(updated: string): string {
  const d = new Date(updated.replace(' ', 'T'));
  if (Number.isNaN(d.getTime())) return '';
  const startOfToday = new Date();
  startOfToday.setHours(0, 0, 0, 0);
  const today = startOfToday.getTime();
  if (d.getTime() >= today) {
    return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  }
  if (d.getTime() >= today - 6 * 86400_000) {
    return `周${'日一二三四五六'[d.getDay()]}`;
  }
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

/* ============ 文件夹选择弹窗 ============ */
interface FolderPickerProps {
  open: boolean;
  folders: ProjectRow[];
  onCreateFolder: (path: string) => void;
  onPick: (projectId: string | null) => void;
  onClose: () => void;
}

function FolderPicker({ open, folders, onCreateFolder, onPick, onClose }: FolderPickerProps) {
  const [busy, setBusy] = useState(false);
  if (!open) return null;

  const handleAddFolder = async () => {
    setBusy(true);
    try {
      const picked = await ipc.fsPickWorkspaceDir();
      if (!picked) return;
      const cleanPath = picked.replace(/^\\\\\?\\/, '');
      if (!cleanPath || !/^[A-Za-z]:\\/.test(cleanPath)) {
        alert(`路径格式异常: ${picked}`);
        return;
      }
      await onCreateFolder(cleanPath);
    } catch (e) {
      alert(`选择目录失败:${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="dialog-mask" onClick={onClose}>
      <div className="wb-folder-dialog dialog" onClick={(e) => e.stopPropagation()}>
        <div className="dialog-head">
          <span>选择文件夹新建任务</span>
          <button type="button" className="icon-btn" onClick={onClose}>
            <IconX size={14} />
          </button>
        </div>
        <div className="dialog-body">
          <button
            type="button"
            className="wb-fp-card wb-fp-card-ghost"
            disabled={busy}
            onClick={() => void handleAddFolder()}
          >
            <span className="wb-fp-card-ico"><IconFolder size={16} /></span>
            <span className="wb-fp-card-main">
              <span className="wb-fp-card-title">{busy ? '选择中…' : '添加项目文件夹'}</span>
              <span className="wb-fp-card-sub">从磁盘选择一个本地目录</span>
            </span>
          </button>
          <button
            type="button"
            className="wb-fp-card wb-fp-card-ghost"
            onClick={() => onPick(null)}
          >
            <span className="wb-fp-card-ico"><IconPlus size={16} /></span>
            <span className="wb-fp-card-main">
              <span className="wb-fp-card-title">直接新建</span>
              <span className="wb-fp-card-sub">不归入任何文件夹</span>
            </span>
          </button>
          {folders.length > 0 && (
            <div className="wb-fp-section-label">已有项目文件夹</div>
          )}
          {folders.map((f) => (
            <button
              key={f.id}
              type="button"
              className="wb-fp-card"
              onClick={() => onPick(f.id)}
              title={f.path}
            >
              <span className="wb-fp-card-ico"><IconFolder size={16} /></span>
              <span className="wb-fp-card-main">
                <span className="wb-fp-card-title">{f.name}</span>
                <span className="wb-fp-card-sub">{f.path}</span>
              </span>
            </button>
          ))}
          {folders.length === 0 && (
            <div className="wb-fp-empty">
              还没有项目文件夹。点上方「添加项目文件夹」从磁盘选一个目录。
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

/* ============ 主组件 ============ */
export function SessionRail({ sessionId }: { sessionId: string }) {
  const nav = useNavigate();
  const sessions = useSessionsStore((s) => s.sessions);
  const store = useSessionsStore();
  const def = useDefaultWorkbench() ?? 'chat';
  const [query, setQuery] = useState('');
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [renameSessionId, setRenameSessionId] = useState<string | null>(null);
  const [renameSessionText, setRenameSessionText] = useState('');
  /** 待确认删除的会话(应用内确认,避免原生 confirm 的先执行后弹出时序) */
  const [pendingDel, setPendingDel] = useState<SessionRow | null>(null);

  const groupMenu = useContextMenu<FolderGroup>();
  const sessionMenu = useContextMenu<SessionRow>();
  const topPlusMenu = useContextMenu();

  const refreshFolders = useCallback(async () => {
    try {
      setProjects(await ipc.listProjects());
    } catch { /* ignore */ }
  }, []);

  useEffect(() => { void refreshFolders(); }, [refreshFolders]);

  /** 在指定项目(可 null=未分类)下新建任务并导航 */
  const handleNewInFolder = useCallback(
    async (projectId: string | null) => {
      try {
        // 先确保 sessions 有数据(可能 store.sessions 还没 refresh)
        const { refresh: doRefresh, sessions: list } = store;
        if (!store.loaded) await doRefresh();
        const sess = list.length > 0 ? list : store.sessions;
        let modelId =
          sess.find((s) => s.workbench_mode !== 'chat')?.model_id ||
          sess.find((s) => s.model_id)?.model_id ||
          '';
        if (!modelId) {
          // 实在没模型,用一个占位让 session 能创建(设置页会有默认)
          modelId = 'default';
        }
        const agentId = sess.find((s) => s.agent_id)?.agent_id ?? null;
        const id = await store.create('新任务', modelId, agentId);
        // 工作台内新建的任务必须显式标记为 work 模式:
        // 否则全局默认为 chat 时,SessionModeGate 会把 /work/:id 弹回普通会话页
        await useSessionsStore.getState().setWorkbenchMode(id, 'work');
        if (projectId) {
          // 用 getState 避免闭包内 store 可能过期
          await useSessionsStore.getState().setProject(id, projectId);
        }
        setPickerOpen(false);
        nav(`/work/${id}`);
      } catch (e) {
        alert(`创建任务失败:${e instanceof Error ? e.message : String(e)}`);
      }
    },
    [nav, store],
  );

  /** 按路径创建项目文件夹 */
  const createFolder = useCallback(async (path: string) => {
    try {
      const f = await ipc.createProject('', path);
      setProjects((list) => [...list, f]);
    } catch (e) {
      alert(`创建项目失败:${e instanceof Error ? e.message : String(e)}`);
    }
  }, []);

  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = sessions
      .filter((s) => resolveMode(s.workbench_mode, def) === 'work')
      .filter((s) => !q || (s.title || '未命名会话').toLowerCase().includes(q));
    return groupByFolder(filtered, projects).map((g) => ({
      ...g,
      collapsed: collapsed[g.key] ?? false,
    }));
  }, [sessions, projects, def, query, collapsed]);

  /* ============ 分组右键菜单 items ============ */
  const buildGroupMenu = (g: FolderGroup): MenuItem[] => {
    const canOpen = !!g.path;
    const isProject = !!g.projectId;
    return [
      {
        key: 'new-here',
        label: `在「${g.label}」中新建任务`,
        onClick: () => void handleNewInFolder(g.projectId),
      },
      { divider: true, key: 's1' },
      {
        key: 'open-folder',
        label: '打开文件夹',
        disabled: !canOpen,
        onClick: () => {
          if (g.path) void ipc.fsOpen(g.path);
        },
      },
      { divider: true, key: 's2' },
      {
        key: 'rename',
        label: '重命名项目',
        disabled: !isProject,
        onClick: () => {
          if (!g.projectId) return;
          const name = prompt('新项目名称:', g.label);
          if (name && name.trim() && name !== g.label) {
            void ipc.renameProject(g.projectId, name.trim()).then(() => void refreshFolders());
          }
        },
      },
      {
        key: 'delete',
        label: '删除项目',
        danger: true,
        disabled: !isProject,
        onClick: () => {
          if (!g.projectId) return;
          const cnt = g.items.length;
          const msg = cnt > 0
            ? `删除项目「${g.label}」?\n\n⚠ 项目下 ${cnt} 个会话将被归入「未分类」,不会被删除。`
            : `删除项目「${g.label}」?`;
          if (confirm(msg)) {
            void ipc.deleteProject(g.projectId).then(() => void refreshFolders());
          }
        },
      },
    ];
  };

  /* ============ 会话项右键菜单 items ============ */
  const buildSessionMenu = (s: SessionRow): MenuItem[] => {
    const moveToItems: MenuItem[] = projects
      .filter((p) => p.id !== s.project_id)
      .map((p) => ({
        key: `move-${p.id}`,
        label: p.name,
        onClick: () => {
          void store.setProject(s.id, p.id).then(() => void refreshFolders());
        },
      }));
    if (s.project_id) {
      moveToItems.push({ divider: true, key: 'move-div' });
      moveToItems.push({
        key: 'uncat',
        label: '归入未分类',
        onClick: () => {
          void store.setProject(s.id, null).then(() => void refreshFolders());
        },
      });
    }
    return [
      {
        key: 'rename',
        label: '重命名',
        onClick: () => startRenameSession(s),
      },
      {
        key: 'clone',
        label: '克隆会话',
        onClick: () => cloneSession(s),
      },
      {
        key: 'move',
        label: '移动到项目…',
        children: moveToItems.length > 0 ? moveToItems : [
          { key: 'no-proj', label: '(没有其他项目)', disabled: true },
        ],
      },
      { divider: true, key: 's1' },
      {
        key: 'delete',
        label: '删除会话',
        danger: true,
        onClick: () => deleteSession(s),
      },
    ];
  };

  const startRenameSession = (s: SessionRow) => {
    setRenameSessionId(s.id);
    setRenameSessionText(s.title || '未命名会话');
  };
  const commitRenameSession = async () => {
    if (!renameSessionId) return;
    const t = renameSessionText.trim() || '未命名会话';
    try {
      await ipc.renameSession(renameSessionId, t);
      store.rename(renameSessionId, t);
    } catch { /* ignore */ }
    setRenameSessionId(null);
  };

  const cloneSession = async (s: SessionRow) => {
    try {
      const newId = await ipc.cloneSession(s.id);
      nav(`/work/${newId}`);
    } catch (e) {
      alert(`克隆失败:${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const deleteSession = (s: SessionRow) => {
    setPendingDel(s);
  };

  const toggleCollapse = (key: string) => {
    setCollapsed((prev) => ({ ...prev, [key]: !(prev[key] ?? false) }));
  };

  /* ============ 顶栏 + 下拉菜单 ============ */
  const buildTopPlusMenu = (): MenuItem[] => {
    const items: MenuItem[] = [
      {
        key: 'direct',
        label: '直接新建',
        onClick: () => void handleNewInFolder(null),
      },
    ];
    if (projects.length > 0) {
      items.push({ divider: true, key: 's1' });
      for (const p of projects) {
        items.push({
          key: `new-${p.id}`,
          label: `在「${p.name}」中新建`,
          onClick: () => void handleNewInFolder(p.id),
        });
      }
    }
    items.push({ divider: true, key: 's2' });
    items.push({
      key: 'add-folder',
      label: '添加项目文件夹…',
      onClick: () => setPickerOpen(true),
    });
    return items;
  };

  return (
    <div className="wb-side-main">
      <div className="wb-side-head">
        <h1>会话</h1>
        <span className="sp" />
        <div style={{ position: 'relative' }}>
          <button
            type="button"
            className="icon-btn"
            title="新建任务"
            onClick={(e) => topPlusMenu.open(e)}
          >
            <IconPlus size={15} />
          </button>
          {topPlusMenu.pos && (
            <ContextMenu
              x={topPlusMenu.pos.x}
              y={topPlusMenu.pos.y}
              items={buildTopPlusMenu()}
              onClose={topPlusMenu.close}
            />
          )}
        </div>
      </div>

      <div className="wb-side-search">
        <IconSearch size={13} />
        <input
          value={query}
          placeholder="搜索会话"
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>

      <div className="wb-side-list">
        {groups.length === 0 && (
          <div className="insp-empty">暂无工作台会话,点顶栏 + 新建任务。</div>
        )}
        {groups.map((g) => (
          <div key={g.key} className={`wb-side-group${g.projectId ? ' card' : ''}`}>
            {/* 分组标题 */}
            <div
              className="wb-side-label"
              onContextMenu={(e) => groupMenu.open(e, g)}
            >
              <button
                type="button"
                className="wb-group-toggle"
                onClick={() => toggleCollapse(g.key)}
                title={g.collapsed ? '展开' : '折叠'}
                style={{
                  all: 'unset',
                  cursor: 'pointer',
                  display: 'inline-flex',
                  alignItems: 'center',
                  color: 'inherit',
                  transition: 'transform .15s',
                  transform: g.collapsed ? 'rotate(0deg)' : 'rotate(90deg)',
                }}
              >
                <IconChevronRight size={12} />
              </button>
              {g.projectId ? <IconFolderOpen size={12} /> : null}
              <span
                style={{ cursor: 'pointer', flex: 1 }}
                onClick={() => toggleCollapse(g.key)}
                onDoubleClick={(e) => {
                  e.preventDefault();
                  void handleNewInFolder(g.projectId);
                }}
                title="双击 = 在此新建任务"
              >
                {g.label}
              </span>
              {g.items.length > 0 && (
                <span className="wb-side-count">{g.items.length}</span>
              )}
              {!g.collapsed && (
                <button
                  type="button"
                  className="icon-btn"
                  title={`在「${g.label}」中新建任务`}
                  style={{ width: 22, height: 22 }}
                  onClick={(e) => {
                    e.stopPropagation();
                    void handleNewInFolder(g.projectId);
                  }}
                >
                  <IconPlus size={12} />
                </button>
              )}
            </div>
            {!g.collapsed && g.items.map((s) => (
              <button
                key={s.id}
                type="button"
                className={`wb-side-item${s.id === sessionId ? ' on' : ''}`}
                title={s.title}
                onClick={() => nav(`/work/${s.id}`)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  sessionMenu.open(e, s);
                }}
                onDoubleClick={() => startRenameSession(s)}
              >
                {s.id === sessionId && <i className="dot" />}
                {renameSessionId === s.id ? (
                  <input
                    className="side-rename"
                    value={renameSessionText}
                    autoFocus
                    onFocus={(e) => e.target.select()}
                    onChange={(e) => setRenameSessionText(e.target.value)}
                    onBlur={() => void commitRenameSession()}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') void commitRenameSession();
                      if (e.key === 'Escape') setRenameSessionId(null);
                    }}
                    onClick={(e) => e.stopPropagation()}
                  />
                ) : (
                  <span className="txt">{s.title || '未命名会话'}</span>
                )}
                <span className="when">{whenLabel(s.updated)}</span>
              </button>
            ))}
          </div>
        ))}
      </div>

      {/* 分组右键菜单 */}
      {groupMenu.pos && groupMenu.target && (
        <ContextMenu
          x={groupMenu.pos.x}
          y={groupMenu.pos.y}
          items={buildGroupMenu(groupMenu.target)}
          onClose={groupMenu.close}
        />
      )}
      {/* 会话项右键菜单 */}
      {sessionMenu.pos && sessionMenu.target && (
        <ContextMenu
          x={sessionMenu.pos.x}
          y={sessionMenu.pos.y}
          items={buildSessionMenu(sessionMenu.target)}
          onClose={sessionMenu.close}
        />
      )}

      <FolderPicker
        open={pickerOpen}
        folders={projects}
        onCreateFolder={createFolder}
        onPick={handleNewInFolder}
        onClose={() => setPickerOpen(false)}
      />
      {pendingDel && (
        <ConfirmDialog
          title="删除会话"
          message={`删除会话「${pendingDel.title || '未命名会话'}」?\n此操作不可撤销。`}
          confirmText="删除"
          danger
          onCancel={() => setPendingDel(null)}
          onConfirm={() => {
            const id = pendingDel.id;
            setPendingDel(null);
            void store.remove(id);
            if (id === sessionId) nav('/work');
          }}
        />
      )}
    </div>
  );
}
