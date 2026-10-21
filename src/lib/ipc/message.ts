import { invoke } from '@tauri-apps/api/core';

import type { MessageRow, BranchInfo } from './types';

// ============ message ============

export interface AppendMessageInput {
  session_id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  model_id?: string | null;
  tokens?: number;
  attachments?: string | null;
  sources?: string | null;
  /** 思考过程文本(V14;assistant 消息可选,缺省空串) */
  reasoning?: string | null;
}

export const appendMessage = (input: AppendMessageInput) =>
  invoke<number>('append_message', { input });

/** 列出分支可见消息(branchId 缺省 = 会话当前激活分支)。 */
export const listMessages = (sessionId: string, branchId?: string) =>
  invoke<MessageRow[]>('list_messages', { sessionId, branchId: branchId ?? null });

/** 清空当前激活分支的自有消息(其他分支不受影响)。 */
export const clearMessages = (sessionId: string) =>
  invoke<void>('clear_messages', { sessionId });

export const deleteMessage = (id: number) => invoke<void>('delete_message', { id });

/** 删除当前激活分支中 id >= fromId 的全部自有消息(放弃分支尾部),返回删除条数。 */
export const deleteMessagesFrom = (sessionId: string, fromId: number) =>
  invoke<number>('delete_messages_from', { sessionId, fromId });

export const updateMessage = (id: number, content: string) =>
  invoke<void>('update_message', { id, content });

// ============ 会话分支树(F4) ============

export const listBranches = (sessionId: string) =>
  invoke<BranchInfo[]>('list_branches', { sessionId });

/** 从指定消息处分叉并切换激活分支(旧分支完整保留),返回新 branch_id。 */
export const forkBranch = (sessionId: string, fromMessageId: number) =>
  invoke<string>('fork_branch', { sessionId, fromMessageId });

export const setActiveBranch = (sessionId: string, branchId: string) =>
  invoke<void>('set_active_branch', { sessionId, branchId });

/** 重命名分支(空名 = 恢复默认显示名)。 */
export const renameBranch = (sessionId: string, branchId: string, name: string) =>
  invoke<void>('rename_branch', { sessionId, branchId, name });

/** 删除分支(main 不可删;删除激活分支自动回退 main)。 */
export const deleteBranch = (sessionId: string, branchId: string) =>
  invoke<void>('delete_branch', { sessionId, branchId });


