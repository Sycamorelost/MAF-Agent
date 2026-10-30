//! 左栏底部任务计划(ui-design b3):进度摘要 + 步骤清单(默认态)/ 任务管理(编辑态)。
//! 文案约定(val.md 风险表):任务 = 目标级(编辑态卡片),清单 = 步骤级(默认态 list)。
//! 数据源为共享任务缓存(stores/tasks):RunStatusBar 步数 / 工具流水归集 / StatusBar 共用。

import { useCallback, useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import {
  parseTaskPlan,
  taskCreate,
  taskSetPlan,
  type Task,
  type TaskStatus,
  type TaskStep,
} from '../../../lib/ipc/task';
import {
  sessionGetTodos,
  todosEvent,
  type TodoItem,
} from '../../../lib/ipc/session';
import { runningTask, useTasksStore } from '../../../stores/tasks';
import { IconChevronDown, IconEdit, IconPlus, IconX } from '../../../components/icons';
import { TaskCard } from './TaskCard';

const COLLAPSE_KEY = 'wb-plan-collapsed';

/** zustand v5 的 selector 必须返回稳定引用:空列表用模块级常量,否则 `?? []` 每次新引用 → 无限渲染。 */
const EMPTY_TASKS: Task[] = [];

/** 状态分组(组内按创建序,由后端 list 保证)。 */
const GROUPS: { key: string; label: string; match: (s: TaskStatus) => boolean }[] = [
  { key: 'running', label: '进行中', match: (s) => s === 'running' },
  { key: 'pending', label: '待办', match: (s) => s === 'pending' || s === 'paused' },
  {
    key: 'closed',
    label: '已收尾',
    match: (s) => s === 'done' || s === 'failed' || s === 'cancelled',
  },
];

const STEP_NEXT: Record<TaskStep['status'], TaskStep['status']> = {
  pending: 'in_progress',
  in_progress: 'completed',
  completed: 'pending',
};

const STEP_DOT: Record<TaskStep['status'], string> = {
  pending: 'wait',
  in_progress: 'run',
  completed: 'done',
};

const STEP_META: Record<string, string> = {
  pending: '待办',
  in_progress: '进行中',
  completed: '已完成',
  cancelled: '已取消',
};

/** 模型 todo 清单 → 步骤视图(与任务计划同形,只读)。 */
function todoToSteps(todos: TodoItem[]): TaskStep[] {
  return todos
    .filter((t) => t.status !== 'cancelled')
    .map((t) => ({
      id: t.id,
      title: t.content,
      status:
        t.status === 'completed'
          ? 'completed'
          : t.status === 'in_progress'
            ? 'in_progress'
            : 'pending',
    }));
}

export function TaskPlanPanel({ sessionId }: { sessionId: string }) {
  const tasks = useTasksStore((s) => s.bySession[sessionId] ?? EMPTY_TASKS);
  const loadTasks = useTasksStore((s) => s.load);
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem(COLLAPSE_KEY) === '1');
  const [editing, setEditing] = useState(false);
  const [adding, setAdding] = useState(false);
  const [newTitle, setNewTitle] = useState('');
  const [newStep, setNewStep] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [todos, setTodos] = useState<TodoItem[]>([]);

  const reload = useCallback(() => {
    void loadTasks(sessionId);
  }, [sessionId, loadTasks]);

  useEffect(reload, [reload]);

  // 模型任务清单(harness todo):后端的 todo_write 只写 todos 表,不建目标级任务;
  // 左栏在任务计划为空时以其作为只读步骤展示,避免"任务计划永远为空"。
  useEffect(() => {
    let alive = true;
    setTodos([]);
    const refresh = () => {
      sessionGetTodos(sessionId)
        .then((v) => {
          if (alive) setTodos(v);
        })
        .catch(() => {});
    };
    refresh();
    const un = listen(todosEvent(sessionId), refresh);
    return () => {
      alive = false;
      void un.then((f) => f());
    };
  }, [sessionId]);

  const active =
    runningTask(tasks) ??
    tasks.find((t) => t.status === 'pending' || t.status === 'paused') ??
    tasks[0] ??
    null;
  const planSteps = active ? parseTaskPlan(active.plan) : [];
  /** 任务计划优先;为空时回落到模型 todo 清单(只读)。 */
  const steps = planSteps.length > 0 ? planSteps : todoToSteps(todos);
  const fromTodos = planSteps.length === 0 && steps.length > 0;
  const done = steps.filter((s) => s.status === 'completed').length;
  const doing = steps.filter((s) => s.status === 'in_progress').length;
  const todo = steps.length - done - doing;
  const pct = steps.length > 0 ? Math.round((done / steps.length) * 100) : 0;

  const toggleCollapse = () => {
    setCollapsed((c) => {
      localStorage.setItem(COLLAPSE_KEY, c ? '0' : '1');
      return !c;
    });
  };

  const onCreate = async () => {
    const title = newTitle.trim();
    if (!title) return;
    try {
      await taskCreate(sessionId, title);
      setNewTitle('');
      setAdding(false);
      reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const savePlan = (next: TaskStep[]) => {
    if (!active) return;
    setError(null);
    void taskSetPlan(active.id, next).then(reload).catch((e) => {
      setError(e instanceof Error ? e.message : String(e));
    });
  };

  const cycleStep = (step: TaskStep) =>
    savePlan(steps.map((s) => (s.id === step.id ? { ...s, status: STEP_NEXT[s.status] } : s)));

  const removeStep = (step: TaskStep) => savePlan(steps.filter((s) => s.id !== step.id));

  const addStep = () => {
    const title = newStep.trim();
    if (!title) return;
    savePlan([...steps, { id: `s${Date.now()}`, title, status: 'pending' }]);
    setNewStep('');
  };

  return (
    <aside className={`wb-plan${collapsed ? ' is-collapsed' : ''}`}>
      <div className="wb-plan-head">
        <button
          type="button"
          className="icon-btn"
          title={collapsed ? '展开任务计划' : '收起任务计划'}
          onClick={toggleCollapse}
        >
          <IconChevronDown size={14} className={collapsed ? 'flip' : undefined} />
        </button>
        <b className="wb-plan-title">任务计划</b>
        <span className="wb-plan-count">
          {steps.length > 0 ? `${done} / ${steps.length}` : tasks.length}
        </span>
        <span className="sp" />
        <button type="button" className="icon-btn" title="新建任务" onClick={() => setAdding((v) => !v)}>
          <IconPlus size={14} />
        </button>
        <button
          type="button"
          className={`icon-btn${editing ? ' on' : ''}`}
          title={editing ? '回到步骤视图' : '管理任务(状态 / 优先级 / 删除)'}
          onClick={() => setEditing((v) => !v)}
        >
          <IconEdit size={14} />
        </button>
      </div>

      {adding && (
        <div className="wb-plan-new">
          <input
            autoFocus
            value={newTitle}
            placeholder="任务标题(目标级)"
            onChange={(e) => setNewTitle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void onCreate();
              if (e.key === 'Escape') setAdding(false);
            }}
          />
          <button type="button" className="btn-primary btn-xs" onClick={() => void onCreate()}>
            创建
          </button>
        </div>
      )}
      {error && <div className="wb-plan-err">{error}</div>}

      {collapsed ? null : editing ? (
        <div className="wb-plan-body">
          {tasks.length === 0 && (
            <div className="wb-plan-empty">还没有任务。点上方 + 新建一个目标,或在对话中让模型拆解步骤。</div>
          )}
          {GROUPS.map((g) => {
            const list = tasks.filter((t) => g.match(t.status));
            if (list.length === 0) return null;
            return (
              <section key={g.key} className="wb-plan-group">
                <div className="wb-plan-group-head">
                  {g.label}
                  <span>{list.length}</span>
                </div>
                {list.map((t) => (
                  <TaskCard key={t.id} task={t} onChanged={reload} onError={setError} />
                ))}
              </section>
            );
          })}
        </div>
      ) : (
        <div className="wb-plan-body wb-plan-view">
          {tasks.length === 0 && steps.length === 0 ? (
            <div className="wb-plan-empty">还没有任务。点上方 + 新建一个目标,或在对话中让模型拆解步骤。</div>
          ) : (
            <>
              {active && <div className="wb-plan-taskname">{active.title}</div>}
              {!active && fromTodos && <div className="wb-plan-taskname">对话任务清单</div>}
              {steps.length > 0 && (
                <div className="plan-sum">
                  <div className="plan-sum-bar">
                    <i style={{ width: `${pct}%` }} />
                  </div>
                  <div className="plan-sum-row">
                    <span>
                      <i className="state-dot done" />已完成 {done}
                    </span>
                    <span>
                      <i className="state-dot run" />进行中 {doing}
                    </span>
                    <span>
                      <i className="state-dot wait" />待办 {todo}
                    </span>
                  </div>
                </div>
              )}
              <ul className="plan-list">
                {steps.map((s) => (
                  <li key={s.id} className={`plan-item${s.status === 'completed' ? ' done' : ''}`}>
                    <button
                      type="button"
                      className="plan-item-main"
                      title={fromTodos ? '来自对话中模型的任务清单(只读)' : '点击推进步骤状态'}
                      disabled={fromTodos}
                      onClick={() => cycleStep(s)}
                    >
                      <i className={`state-dot ${STEP_DOT[s.status]}`} />
                      <span className="txt">
                        {s.title}
                        <span className="meta">{STEP_META[s.status]}</span>
                      </span>
                    </button>
                    {!fromTodos && (
                      <button
                        type="button"
                        className="plan-item-del"
                        title="删除步骤"
                        onClick={() => removeStep(s)}
                      >
                        <IconX size={11} />
                      </button>
                    )}
                  </li>
                ))}
              </ul>
              {active && !fromTodos && (
                <div className="task-step-new">
                  <input
                    value={newStep}
                    placeholder="加一步计划…"
                    onChange={(e) => setNewStep(e.target.value)}
                    onKeyDown={(e) => e.key === 'Enter' && addStep()}
                  />
                  <button type="button" className="btn-ghost btn-xs" title="添加步骤" onClick={addStep}>
                    <IconPlus size={12} />
                  </button>
                </div>
              )}
            </>
          )}
        </div>
      )}
    </aside>
  );
}
