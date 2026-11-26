/** Sidebar 右键菜单构建:纯函数,状态与回调由 Sidebar 通过 ctx 传入 */
import type { MenuItem } from '../ContextMenu';
import * as ipc from '../../lib/ipc';
import { cloneSession } from '../../lib/ipc/session';
import type { ProjectRow, SessionRow } from '../../lib/ipc';
import type { DeleteProjectState } from './types';

export interface SidebarMenuContext {
  /** 全部项目(用于会话「移动到项目」子菜单) */
  projects: ProjectRow[];
  /**
   * 每个项目下的会话数(含已归档)。
   * 删除项目会把**全部**会话(含归档)移出项目,计数必须一致,否则确认框会少报。
   */
  projectCounts: Record<string, number>;
  onNewSession: (projectId: string | null) => void;
  onChangeProjectPath: (p: ProjectRow) => void;
  onStartRenameProject: (p: ProjectRow) => void;
  onRequestDeleteProject: (state: DeleteProjectState) => void;
  /** 归档项目(级联归档其会话) */
  onArchiveProject: (p: ProjectRow) => void;
  onStartRenameSession: (s: SessionRow) => void;
  onRequestDeleteSession: (s: SessionRow) => void;
  onSetSessionProject: (sessionId: string, projectId: string | null) => void;
  onNavigateToSession: (id: string) => void;
  /** 进入多选模式(以该会话为初始选中项) */
  onBeginSelect: (id: string) => void;
  /** 归档 / 恢复(按 session.archived 决定语义) */
  onToggleArchive: (s: SessionRow) => void;
}

export function buildProjectMenu(p: ProjectRow, ctx: SidebarMenuContext): MenuItem[] {
  return [
    { key: 'new-here', label: `在「${p.name}」中新建会话`,
      onClick: () => ctx.onNewSession(p.id) },
    { divider: true, key: 's1' },
    { key: 'open-folder', label: '打开工作文件夹',
      onClick: () => { if (p.path) void ipc.fsOpen(p.path); } },
    { key: 'change-folder', label: '更换工作文件夹…',
      onClick: () => ctx.onChangeProjectPath(p) },
    { divider: true, key: 's2' },
    { key: 'rename', label: '重命名项目',
      onClick: () => ctx.onStartRenameProject(p) },
    { key: 'archive', label: '归档项目',
      onClick: () => ctx.onArchiveProject(p) },
    { key: 'delete', label: '删除项目', danger: true,
      onClick: () => {
        const count = ctx.projectCounts[p.id] ?? 0;
        ctx.onRequestDeleteProject({ projectId: p.id, label: p.name, count });
      } },
    // 最底部:项目路径,自动省略 + 悬停全名 + 不可点击
    ...(p.path ? [
      { divider: true, key: 's3' },
      { key: 'project-path', label: p.path, disabled: true, title: p.path, className: 'ctx-path-item' },
    ] : []),
  ];
}

export function buildSessionMenu(s: SessionRow, ctx: SidebarMenuContext): MenuItem[] {
  const moveTo: MenuItem[] = ctx.projects
    .filter((p) => p.id !== s.project_id)
    .map((p) => ({
      key: `move-${p.id}`, label: p.name,
      onClick: () => ctx.onSetSessionProject(s.id, p.id),
    }));
  if (moveTo.length > 0) {
    moveTo.push({ divider: true, key: 'move-div' });
    moveTo.push({ key: 'uncat', label: '移出项目',
      onClick: () => ctx.onSetSessionProject(s.id, null) });
  }
  return [
    { key: 'select', label: '多选', onClick: () => ctx.onBeginSelect(s.id) },
    { divider: true, key: 's0' },
    { key: 'rename', label: '重命名', onClick: () => ctx.onStartRenameSession(s) },
    { key: 'clone', label: '克隆会话',
      onClick: () => { void cloneSession(s.id).then((newId) => ctx.onNavigateToSession(newId)); } },
    { key: 'move', label: '移动到项目…',
      children: moveTo.length > 0
        ? moveTo
        : [{ key: 'no-proj', label: '(没有其他项目)', disabled: true }] },
    { key: 'archive', label: s.archived ? '恢复到会话列表' : '归档会话',
      onClick: () => ctx.onToggleArchive(s) },
    { divider: true, key: 's1' },
    { key: 'delete', label: '删除会话', danger: true,
      onClick: () => ctx.onRequestDeleteSession(s) },
  ];
}
