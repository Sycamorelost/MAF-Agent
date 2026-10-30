//! 工作台外壳与对话内核之间的只读组合槽(ui-design b3)。
//! 工作台负责渲染运行状态条、检查器开关;对话内核负责把它们插在「会话头之下、消息区之上」,
//! 避免 chat 反向 import workbench,也避免把整段状态条逻辑塞进 ChatPage。

import { createContext, useContext, type ReactNode } from 'react';

export interface WorkbenchChrome {
  /** 会话头下方的运行状态条(由工作台提供)。 */
  runBar: ReactNode;
  /** 检查器是否展开,供会话头的高亮状态使用。 */
  inspectorOpen: boolean;
  /** 切换检查器展开/收起。 */
  onToggleInspector: () => void;
}

export const WorkbenchChromeContext = createContext<WorkbenchChrome | null>(null);

export function useWorkbenchChrome(): WorkbenchChrome | null {
  return useContext(WorkbenchChromeContext);
}
