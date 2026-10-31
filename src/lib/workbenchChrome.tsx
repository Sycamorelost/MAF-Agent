//! 工作台外壳与对话内核之间的只读组合槽(ui-design b3)。
//! 工作台负责提供检查器开关状态;对话内核消费它渲染会话头的高亮态,
//! 避免 chat 反向 import workbench。

import { createContext, useContext } from 'react';

export interface WorkbenchChrome {
  /** 检查器是否展开,供会话头的高亮状态使用。 */
  inspectorOpen: boolean;
  /** 切换检查器展开/收起。 */
  onToggleInspector: () => void;
  /** 左栏会话列表是否展开。 */
  sidebarOpen: boolean;
  /** 切换左栏会话列表展开/收起。 */
  onToggleSidebar: () => void;
}

export const WorkbenchChromeContext = createContext<WorkbenchChrome | null>(null);

export function useWorkbenchChrome(): WorkbenchChrome | null {
  return useContext(WorkbenchChromeContext);
}
