import { invoke } from '@tauri-apps/api/core';

import type { SessionRow, AgentTemplate, MemoryStrategy } from './types';

// ============ session ============

export const createSession = (title: string, modelId: string, agentId?: string | null) =>
  invoke<{ id: string; memory_path: string }>('create_session', {
    title,
    modelId,
    agentId: agentId ?? null,
  });

export const listSessions = () => invoke<SessionRow[]>('list_sessions');

export const getSession = (id: string) => invoke<SessionRow | null>('get_session', { id });

// ============ harness 任务清单 ============

/** 任务清单条目(与后端 db::todos::TodoItem 对齐)。 */
export interface TodoItem {
  id: string;
  content: string;
  /** pending | in_progress | completed | cancelled */
  status: 'pending' | 'in_progress' | 'completed' | 'cancelled' | string;
  /** high | medium | low */
  priority: 'high' | 'medium' | 'low' | string;
}

/** 初始拉取会话任务清单;后续变更由 `session-todos-{id}` 事件推送。 */
export const sessionGetTodos = (sessionId: string) =>
  invoke<TodoItem[]>('session_get_todos', { sessionId });

/** todo 变更事件名(后端 dispatcher 在 todo_write 成功后推送)。 */
export const todosEvent = (sessionId: string) => `session-todos-${sessionId}`;

/** 会话通知事件名(如上下文自动压缩提示)。 */
export const chatNoticeEvent = (sessionId: string) => `chat-notice-${sessionId}`;

export const renameSession = (id: string, title: string) =>
  invoke<void>('rename_session', { id, title });

export const setSessionModel = (id: string, modelId: string) =>
  invoke<void>('set_session_model', { id, modelId });

export const deleteSession = (id: string) => invoke<void>('delete_session', { id });

/** 仅当会话没有任何消息时删除(空对话默认不保留),返回是否已删。 */
export const deleteSessionIfEmpty = (id: string) =>
  invoke<boolean>('delete_session_if_empty', { id });

/** 批量清理空会话(0 条消息且超过 grace 秒未更新),返回被删 id 列表。 */
export const pruneEmptySessions = (graceSecs?: number) =>
  invoke<string[]>('prune_empty_sessions', { graceSecs: graceSecs ?? null });

export const updateSessionSettings = (
  id: string,
  systemPrompt: string | null,
  maxContextMessages: number,
) =>
  invoke<void>('update_session_settings', {
    id,
    systemPrompt,
    maxContextMessages,
  });

/** 更新会话的对话内联网开关。 */
export const setSessionWebSearch = (id: string, enabled: boolean) =>
  invoke<void>('set_session_web_search', { id, enabled });

/** 更新切换模型时的记忆注入策略(D3.1)。 */
export const setSessionMemoryStrategy = (id: string, strategy: MemoryStrategy) =>
  invoke<void>('set_session_memory_strategy', { id, strategy });

// ============ Agent 模板(B2) ============

export const listAgentTemplates = () => invoke<AgentTemplate[]>('list_agent_templates');

export const saveAgentTemplate = (agent: AgentTemplate) =>
  invoke<void>('save_agent_template', { agent });

export const deleteAgentTemplate = (agentId: string) =>
  invoke<void>('delete_agent_template', { agentId });

/** 绑定/解绑会话的 Agent(agentId=null 解绑;绑定时同步记忆注入策略 + 渲染写入 system_prompt)。 */
export const setSessionAgent = (id: string, agentId: string | null) =>
  invoke<void>('set_session_agent', { id, agentId });

/** 仅把 Agent 渲染后的 system_prompt 写入会话设置(B4 / F2,不改变绑定)。 */
export const applyAgentPrompt = (id: string, agentId: string) =>
  invoke<void>('apply_agent_prompt', { id, agentId });


