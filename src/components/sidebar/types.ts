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
}
