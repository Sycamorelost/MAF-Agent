/**
 * 专业工作台会话壳(ui-design b3):左 280 / 中自适应 / 右 352 + 底部状态栏。
 * 左栏 = 会话列表(搜索 + 今天/本周分组)+ 底部任务计划;中栏由路由注入对话内核;
 * 检查器开关状态经 WorkbenchChrome 槽交给 ChatPage 组合,
 * 避免 chat 反向 import workbench(val.md §8)。
 */
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useParams } from 'react-router-dom';
import { StatusBar } from '../../components/StatusBar';
import { ResizeHandle } from '../../components/ResizeHandle';
import { WorkbenchChromeContext } from '../../lib/workbenchChrome';
import { usePanelWidth } from '../../lib/usePanelWidth';
import { useUiStore } from '../../stores/ui';
import { TaskPlanPanel } from './components/TaskPlanPanel';
import { SessionRail } from './components/SessionRail';
import { Inspector } from './components/inspector/Inspector';

const INSPECTOR_KEY = 'wb-inspector-open';

export default function WorkbenchPage({ children }: { children?: ReactNode }) {
  const { id } = useParams<{ id: string }>();
  const [inspectorOpen, setInspectorOpen] = useState(
    () => localStorage.getItem(INSPECTOR_KEY) !== '0',
  );

  /** 窄窗自动收起过一次的标记;用户手动开关后失效(尊重用户选择) */
  const autoClosed = useRef(false);

  /* ≤560 极窄窗:左栏转为抽屉(标题栏「会话历史」开关),选中会话后自动收起 */
  const sidebarOpen = useUiStore((s) => s.sidebarOpen);
  const setSidebarOpen = useUiStore((s) => s.setSidebarOpen);
  useEffect(() => {
    setSidebarOpen(false);
  }, [id, setSidebarOpen]);

  const toggleInspector = useCallback(() => {
    autoClosed.current = false;
    setInspectorOpen((open) => {
      localStorage.setItem(INSPECTOR_KEY, open ? '0' : '1');
      return !open;
    });
  }, []);

  /** 左栏会话列表:可折叠 + 可拖宽(与检查器同套 usePanelWidth,宽度/折叠态持久化) */
  const side = usePanelWidth('wbside', 280, 200, 420, 'left', 240, false);

  /* 窄窗(≤900,检查器转浮层):进入时自动收起避免遮挡中栏与输入框;
     回到宽窗时仅恢复「被自动收起」的展开态,用户手动收起的不打扰 */
  useEffect(() => {
    const mq = window.matchMedia('(max-width: 900px)');
    const apply = (narrow: boolean) => {
      if (narrow) {
        setInspectorOpen((open) => {
          if (!open) return open;
          autoClosed.current = true;
          localStorage.setItem(INSPECTOR_KEY, '0');
          return false;
        });
      } else if (autoClosed.current) {
        autoClosed.current = false;
        localStorage.setItem(INSPECTOR_KEY, '1');
        setInspectorOpen(true);
      }
    };
    apply(mq.matches);
    const onChange = (e: MediaQueryListEvent) => apply(e.matches);
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, []);

  const chrome = useMemo(
    () =>
      id
        ? {
            inspectorOpen,
            onToggleInspector: toggleInspector,
            sidebarOpen: !side.collapsed,
            onToggleSidebar: side.toggleCollapse,
          }
        : null,
    [id, inspectorOpen, toggleInspector, side.collapsed, side.toggleCollapse],
  );

  if (!id) return null;

  return (
    <div className="wb-page">
      <div className="wb-body">
        {!side.collapsed && (
          <>
            <aside
              className={`wb-side${sidebarOpen ? ' drawer-open' : ''}`}
              style={{ width: side.width }}
            >
              <SessionRail sessionId={id} />
              <TaskPlanPanel sessionId={id} />
            </aside>
            <ResizeHandle panel={side} />
          </>
        )}
        <div className="wb-main">
          <WorkbenchChromeContext.Provider value={chrome}>{children}</WorkbenchChromeContext.Provider>
        </div>
        <Inspector sessionId={id} collapsed={!inspectorOpen} onToggleCollapse={toggleInspector} />
      </div>
      <StatusBar sessionId={id} />
    </div>
  );
}
