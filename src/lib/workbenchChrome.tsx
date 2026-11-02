//! 对话页头部 chrome 组合槽:检查器开关状态经 context 传给会话头渲染高亮态,
//! 避免头部与面板状态各自为政(P3-1 起由 ChatPage 本地提供,原工作台外壳已拆)。

import { createContext, useContext } from 'react';

export interface WorkbenchChrome {
  /** 检查器是否展开,供会话头的高亮状态使用。 */
  inspectorOpen: boolean;
  /** 切换检查器展开/收起。 */
  onToggleInspector: () => void;
}

export const WorkbenchChromeContext = createContext<WorkbenchChrome | null>(null);

export function useWorkbenchChrome(): WorkbenchChrome | null {
  return useContext(WorkbenchChromeContext);
}
