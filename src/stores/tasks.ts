//! 任务共享缓存(V25 / D8 扩展):TaskPlanPanel、RunStatusBar(步数)、工具流水(按任务归集)、
//! StatusBar 共用同一份任务列表,避免四处各自 taskList。
//! 只读缓存:写操作仍走 lib/ipc/task,完成后调 load(sessionId) 刷新。

import { create } from 'zustand';
import { parseTaskPlan, taskList, type Task } from '../lib/ipc/task';

interface TasksState {
  bySession: Record<string, Task[]>;
  load: (sessionId: string) => Promise<void>;
}

export const useTasksStore = create<TasksState>((set) => ({
  bySession: {},
  load: async (sessionId) => {
    try {
      const list = await taskList(sessionId);
      set((s) => ({ bySession: { ...s.bySession, [sessionId]: list } }));
    } catch {
      // 列表加载失败不阻塞对话
    }
  },
}));

/** 当前进行中任务(取第一个 running)。 */
export function runningTask(tasks: Task[] | undefined): Task | null {
  return tasks?.find((t) => t.status === 'running') ?? null;
}

/** 任务计划步骤进度;无计划或无步骤返回 null。 */
export function taskProgress(task: Task | null): { done: number; total: number } | null {
  if (!task) return null;
  const steps = parseTaskPlan(task.plan);
  if (steps.length === 0) return null;
  return { done: steps.filter((s) => s.status === 'completed').length, total: steps.length };
}
