import { IconFolder, IconList, IconPlus, IconX } from '../icons';
import type { ViewMode } from './utils';

/** Sidebar 头部:标题 + 操作按钮(排序用固定三横线图标) */
interface Props {
  viewMode: ViewMode;
  onToggleViewMode: () => void;
  onNewSession: () => void;
  onNewProject: () => void;
  onClose: () => void;
}

export function SidebarHeader({
  viewMode,
  onToggleViewMode,
  onNewSession,
  onNewProject,
  onClose,
}: Props) {
  const viewTitle = viewMode === 'project' ? '按项目分组' : '按时间排序';
  return (
    <div className="side-head">
      <span className="side-head-title">会话</span>
      <div className="side-head-actions">
        <button
          className="panel-toggle"
          title={`当前:${viewTitle} · 点击切换`}
          onClick={onToggleViewMode}
        >
          <IconList size={14} />
        </button>
        <button
          className="panel-toggle"
          title="新建会话(不绑定项目)"
          onClick={onNewSession}
        >
          <IconPlus size={14} />
        </button>
        <button
          className="panel-toggle"
          title="新建项目(绑定本地文件夹)"
          onClick={onNewProject}
        >
          <IconFolder size={14} />
        </button>
        <button
          className="panel-toggle"
          title="关闭侧栏"
          onClick={onClose}
        >
          <IconX size={14} />
        </button>
      </div>
    </div>
  );
}
