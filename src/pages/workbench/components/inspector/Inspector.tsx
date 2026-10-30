//! 右栏检查器(P6 / D9 / D10):产物 / 变更与回滚 / 上下文与记忆 / 工具流水 四页签。
//! 容器只管页签与折叠;数据源统一为 useWorkbenchData(禁止页签各自解析消息)。
//! 形态对齐 ui-design:标题 + 计数徽标 + 下划线页签;宽度可拖拽(240-520,默认 352)。

import { useState } from 'react';
import { IconChevronLeft, IconChevronRight } from '../../../../components/icons';
import { ResizeHandle } from '../../../../components/ResizeHandle';
import { usePanelWidth } from '../../../../lib/usePanelWidth';
import { useWorkbenchData } from '../../useWorkbenchData';
import { ArtifactsTab } from './ArtifactsTab';
import { ChangesTab } from './ChangesTab';
import { ContextTab } from './ContextTab';
import { ToolStreamTab } from './ToolStreamTab';

const TAB_KEY = 'wb-inspector-tab';

const TABS = [
  { key: 'artifacts', label: '产物' },
  { key: 'changes', label: '变更与回滚' },
  { key: 'context', label: '上下文与记忆' },
  { key: 'stream', label: '工具流水' },
] as const;
type TabKey = (typeof TABS)[number]['key'];

function readTab(): TabKey {
  const v = localStorage.getItem(TAB_KEY);
  return TABS.some((t) => t.key === v) ? (v as TabKey) : 'artifacts';
}

export function Inspector({
  sessionId,
  collapsed,
  onToggleCollapse,
}: {
  sessionId: string;
  /** 展开/收起由工作台统一持有(会话头里的面板按钮与这里的箭头共用同一状态)。 */
  collapsed: boolean;
  onToggleCollapse: () => void;
}) {
  const data = useWorkbenchData(sessionId);
  const [tab, setTab] = useState<TabKey>(readTab);
  const panel = usePanelWidth('wbinspector', 352, 240, 520, 'right', 300, false);

  const pick = (k: TabKey) => {
    localStorage.setItem(TAB_KEY, k);
    setTab(k);
  };

  if (collapsed) {
    return (
      <aside className="wb-inspector is-collapsed">
        <button
          type="button"
          className="icon-btn"
          title="展开检查器"
          onClick={onToggleCollapse}
        >
          <IconChevronLeft size={14} />
        </button>
      </aside>
    );
  }

  /** 当前页签的条目数(上下文页签显示用量百分比)。 */
  const count =
    tab === 'artifacts'
      ? data.artifacts.length
      : tab === 'changes'
        ? new Set(data.backups.map((b) => b.rel)).size
        : tab === 'stream'
          ? data.traces.length
          : data.budget && data.budget.limit > 0
            ? `${Math.round((data.budget.tokens / data.budget.limit) * 100)}%`
            : 0;

  return (
    <>
      <ResizeHandle panel={panel} />
      <aside className="wb-inspector" style={{ width: panel.width }}>
        <div className="wb-inspector-head">
          <h2 className="wb-inspector-title">检查器</h2>
          <span className="wb-inspector-count">{count}</span>
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
        <div className="wb-inspector-tabs">
          {TABS.map((t) => (
            <button
              key={t.key}
              type="button"
              className={`wb-inspector-tab${tab === t.key ? ' on' : ''}`}
              onClick={() => pick(t.key)}
            >
              {t.label}
            </button>
          ))}
        </div>
        <div className="wb-inspector-body">
          {tab === 'artifacts' && <ArtifactsTab data={data} />}
          {tab === 'changes' && <ChangesTab data={data} />}
          {tab === 'context' && <ContextTab sessionId={sessionId} data={data} />}
          {tab === 'stream' && <ToolStreamTab data={data} />}
        </div>
      </aside>
    </>
  );
}
