//! 任务(V25 / D8)IPC 封装 —— 对应 Rust 侧 commands::tasks。
//! 任务 = 目标级卡片(标题/目标/状态/优先级/计划);步骤级清单见 session.ts 的 todos。

import { invoke } from '@tauri-apps/api/core';

/** 任务状态:pending / running / paused / done / failed / cancelled。 */
export type TaskStatus = 'pending' | 'running' | 'paused' | 'done' | 'failed' | 'cancelled';

/** 任务优先级:low / normal / high。 */
export type TaskPriority = 'low' | 'normal' | 'high';

/** 目标级任务(与后端 tasks::Task 对齐)。 */
export interface Task {
  id: string;
  session_id: string;
  title: string;
  goal: string;
  status: TaskStatus;
  priority: TaskPriority;
  /** 计划步骤 JSON 字符串(TaskPlan);null = 未规划 */
  plan: string | null;
  created: string;
  updated: string;
}

/** 计划步骤(plan JSON 的元素)。 */
export interface TaskStep {
  id: string;
  title: string;
  status: 'pending' | 'in_progress' | 'completed';
}

/** plan JSON 结构:只存步骤数组,不存大段正文。 */
export interface TaskPlan {
  steps: TaskStep[];
}

/** 解析 plan JSON;空/非法一律视为无步骤(不让坏数据炸掉面板)。 */
export function parseTaskPlan(plan: string | null): TaskStep[] {
  if (!plan) return [];
  try {
    const v = JSON.parse(plan) as Partial<TaskPlan>;
    return Array.isArray(v.steps) ? v.steps : [];
  } catch {
    return [];
  }
}

export const taskCreate = (sessionId: string, title: string, goal?: string, priority?: TaskPriority) =>
  invoke<Task>('task_create', { sessionId, title, goal: goal ?? null, priority: priority ?? null });

export const taskList = (sessionId: string) => invoke<Task[]>('task_list', { sessionId });

/** 部分更新(不传 = 不变)。 */
export const taskUpdate = (
  id: string,
  patch: { title?: string; goal?: string; status?: TaskStatus; priority?: TaskPriority },
) =>
  invoke<void>('task_update', {
    id,
    title: patch.title ?? null,
    goal: patch.goal ?? null,
    status: patch.status ?? null,
    priority: patch.priority ?? null,
  });

/** 写回计划步骤(null 清除)。 */
export const taskSetPlan = (id: string, steps: TaskStep[] | null) =>
  invoke<void>('task_set_plan', { id, plan: steps ? JSON.stringify({ steps }) : null });

export const taskDelete = (id: string) => invoke<void>('task_delete', { id });
