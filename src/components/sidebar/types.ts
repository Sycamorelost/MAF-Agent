/** Sidebar 共享类型(避免循环引用,独立成文件) */
import type { ProjectRow, SessionRow } from '../../lib/ipc';

export interface ProjectGroup {
  kind: 'project';
  project: ProjectRow;
  items: SessionRow[];
}

export interface DeleteProjectState {
  projectId: string;
  label: string;
  count: number;
}

/** 会话多选状态(右键菜单「多选」进入,批量操作栏消费) */
export interface SelectApi {
  mode: boolean;
  selected: Set<string>;
  /** 进入多选模式;传入 id 时同时选中该项 */
  begin: (id?: string) => void;
  end: () => void;
  toggle: (id: string) => void;
  /** 在给定集合上全选 / 取消全选 */
  toggleAll: (ids: string[]) => void;
}

export interface SessionRowProps {
  /** 当前会话 id(用于高亮) */
  activeId: string | undefined;
  /** 会话数据 */
  session: SessionRow;
  /** 缩进 CSS 类 */
  indentClass: string;
  /** 内联重命名状态 */
  editingId: string | null;
  editingText: string;
  /** 事件 */
  onNavigate: (id: string) => void;
  onStartRename: (s: SessionRow) => void;
  onCommitRename: () => void;
  onCancelRename: () => void;
  onContextMenu: (e: React.MouseEvent, s: SessionRow) => void;
  /** 多选模式(可选,缺省不启用) */
  selectMode?: boolean;
  selected?: boolean;
  onToggleSelect?: (id: string) => void;
}
