//! 右栏检查器(简化版):只保留工具流水,其他页签(产物/变更/上下文)已删除。
//! 头部仅保留折叠按钮;正文 ToolStreamTab;宽度可拖拽(240-520,默认 352)。

import { IconChevronRight } from '../icons';
import { ResizeHandle } from '../ResizeHandle';
import { usePanelWidth } from '../../lib/usePanelWidth';
import { useWorkbenchData } from './useWorkbenchData';
import { ToolStreamTab } from './ToolStreamTab';

export function Inspector({
  sessionId,
  collapsed,
  onToggleCollapse,
}: {
  sessionId: string;
  collapsed: boolean;
  onToggleCollapse: () => void;
}) {
  const data = useWorkbenchData(sessionId);
  const panel = usePanelWidth('wbinspector', 352, 240, 520, 'right', 300, false);

  // 收起态不再渲染细条:与会话头「展开/收起检查器」按钮功能重复,统一由头部按钮开合
  if (collapsed) return null;

  return (
    <>
      <ResizeHandle panel={panel} />
      <aside className="wb-inspector" style={{ width: panel.width }}>
        <div className="wb-inspector-head">
          <span className="wb-inspector-title">检查器</span>
          <span className="wb-inspector-count">{data.traces.length}</span>
          <span className="sp" />
          <button
            type="button"
            className="icon-btn"
            title="收起检查器"
            onClick={onToggleCollapse}
          >
            <IconChevronRight size={14} />
          </button>
        </div>
        <div className="wb-inspector-body">
          <ToolStreamTab data={data} />
        </div>
      </aside>
    </>
  );
}
