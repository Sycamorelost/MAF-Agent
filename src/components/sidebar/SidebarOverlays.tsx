import { createPortal } from 'react-dom';
import Toast from '../Toast';
import type { MenuItem } from '../ContextMenu';
import type { ProjectRow, SessionRow } from '../../lib/ipc';
import type { DeleteProjectState } from './types';
import { SidebarMenus } from './Menus';
import SearchPalette from '../search/SearchPalette';

/** 右键菜单定位状态(useContextMenu 返回值的子集) */
interface MenuState<T> {
  pos: { x: number; y: number } | null;
  target: T | null;
  close: () => void;
}

interface Props {
  projectMenu: MenuState<ProjectRow>;
  sessionMenu: MenuState<SessionRow>;
  pendingDel: SessionRow | null;
  pendingProjectDel: DeleteProjectState | null;
  renamingProject: ProjectRow | null;
  onBuildProjectMenu: (p: ProjectRow) => MenuItem[];
  onBuildSessionMenu: (s: SessionRow) => MenuItem[];
  onCancelDeleteSession: () => void;
  onConfirmDeleteSession: () => void;
  onCancelDeleteProject: () => void;
  onConfirmDeleteProject: () => void;
  onCancelRenameProject: () => void;
  onConfirmRenameProject: (name: string) => void;
  toast: string | null;
  onCloseToast: () => void;
  searchOpen: boolean;
  searchQuery: string;
  onSearchQuery: (q: string) => void;
  sessions: SessionRow[];
  onCloseSearch: () => void;
  searchInputRef: React.MutableRefObject<HTMLInputElement | null>;
}

/** 底部浮层:右键菜单/确认弹窗 + Toast + 搜索面板(Portal 到 body) */
export function SidebarOverlays({
  projectMenu,
  sessionMenu,
  pendingDel,
  pendingProjectDel,
  renamingProject,
  onBuildProjectMenu,
  onBuildSessionMenu,
  onCancelDeleteSession,
  onConfirmDeleteSession,
  onCancelDeleteProject,
  onConfirmDeleteProject,
  onCancelRenameProject,
  onConfirmRenameProject,
  toast,
  onCloseToast,
  searchOpen,
  searchQuery,
  onSearchQuery,
  sessions,
  onCloseSearch,
  searchInputRef,
}: Props) {
  return (
    <>
      <SidebarMenus
        projectMenuPos={projectMenu.pos}
        projectMenuTarget={projectMenu.target}
        sessionMenuPos={sessionMenu.pos}
        sessionMenuTarget={sessionMenu.target}
        pendingDel={pendingDel}
        pendingProjectDel={pendingProjectDel}
        onCloseProjectMenu={projectMenu.close}
        onCloseSessionMenu={sessionMenu.close}
        onBuildProjectMenu={onBuildProjectMenu}
        onBuildSessionMenu={onBuildSessionMenu}
        onCancelDeleteSession={onCancelDeleteSession}
        onConfirmDeleteSession={onConfirmDeleteSession}
        onCancelDeleteProject={onCancelDeleteProject}
        onConfirmDeleteProject={onConfirmDeleteProject}
        renamingProject={renamingProject}
        onCancelRenameProject={onCancelRenameProject}
        onConfirmRenameProject={onConfirmRenameProject}
      />
      <Toast text={toast} onClose={onCloseToast} />
      {/* P16:弹出式搜索面板(Portal 到 body,浮层覆盖全窗口) */}
      {searchOpen && createPortal(
        <SearchPalette
          query={searchQuery}
          onQuery={onSearchQuery}
          sessions={sessions}
          onClose={onCloseSearch}
          inputRef={searchInputRef}
        />,
        document.body,
      )}
    </>
  );
}
