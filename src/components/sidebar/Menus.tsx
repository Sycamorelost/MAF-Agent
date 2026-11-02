import { ContextMenu, type MenuItem } from '../ContextMenu';
import { ConfirmDialog } from '../ConfirmDialog';
import { PromptDialog } from '../PromptDialog';
import type { ProjectRow, SessionRow } from '../../lib/ipc';
import type { DeleteProjectState } from './types';

/** Sidebar 的所有浮层菜单 + 确认弹窗 */
interface Props {
  projectMenuPos: { x: number; y: number } | null;
  projectMenuTarget: ProjectRow | null;
  sessionMenuPos: { x: number; y: number } | null;
  sessionMenuTarget: SessionRow | null;
  pendingDel: SessionRow | null;
  pendingProjectDel: DeleteProjectState | null;
  /** 待重命名的项目(非空时显示输入弹窗) */
  renamingProject: ProjectRow | null;
  // 构建菜单项(由父组件闭包提供上下文)
  onCloseProjectMenu: () => void;
  onCloseSessionMenu: () => void;
  onBuildProjectMenu: (p: ProjectRow) => MenuItem[];
  onBuildSessionMenu: (s: SessionRow) => MenuItem[];
  onCancelDeleteSession: () => void;
  onConfirmDeleteSession: () => void;
  onCancelDeleteProject: () => void;
  onConfirmDeleteProject: () => void;
  onCancelRenameProject: () => void;
  onConfirmRenameProject: (name: string) => void;
}

export function SidebarMenus({
  projectMenuPos,
  projectMenuTarget,
  sessionMenuPos,
  sessionMenuTarget,
  pendingDel,
  pendingProjectDel,
  renamingProject,
  onCloseProjectMenu,
  onCloseSessionMenu,
  onBuildProjectMenu,
  onBuildSessionMenu,
  onCancelDeleteSession,
  onConfirmDeleteSession,
  onCancelDeleteProject,
  onConfirmDeleteProject,
  onCancelRenameProject,
  onConfirmRenameProject,
}: Props) {
  return (
    <>
      {/* 项目右键菜单 */}
      {projectMenuPos && projectMenuTarget && (
        <ContextMenu
          x={projectMenuPos.x}
          y={projectMenuPos.y}
          items={onBuildProjectMenu(projectMenuTarget)}
          onClose={onCloseProjectMenu}
        />
      )}

      {/* 会话右键菜单 */}
      {sessionMenuPos && sessionMenuTarget && (
        <ContextMenu
          x={sessionMenuPos.x}
          y={sessionMenuPos.y}
          items={onBuildSessionMenu(sessionMenuTarget)}
          onClose={onCloseSessionMenu}
        />
      )}

      {/* 删除会话确认 */}
      {pendingDel && (
        <ConfirmDialog
          title="删除会话"
          message={`删除会话「${pendingDel.title || '未命名'}」?\n此操作不可撤销。`}
          confirmText="删除"
          danger
          onCancel={onCancelDeleteSession}
          onConfirm={onConfirmDeleteSession}
        />
      )}

      {/* 删除项目确认 */}
      {pendingProjectDel && (
        <ConfirmDialog
          title="删除项目"
          message={
            pendingProjectDel.count > 0
              ? `删除项目「${pendingProjectDel.label}」?\n\n项目下 ${pendingProjectDel.count} 个会话将被归入「未分类」,不会被删除。`
              : `删除项目「${pendingProjectDel.label}」?`
          }
          confirmText="删除"
          danger
          onCancel={onCancelDeleteProject}
          onConfirm={onConfirmDeleteProject}
        />
      )}
      {/* 重命名项目(应用内输入弹窗,替代原生 prompt) */}
      {renamingProject && (
        <PromptDialog
          title="重命名项目"
          label="新项目名称"
          initialValue={renamingProject.name}
          placeholder="输入项目名称"
          onCancel={onCancelRenameProject}
          onConfirm={onConfirmRenameProject}
        />
      )}
    </>
  );
}
