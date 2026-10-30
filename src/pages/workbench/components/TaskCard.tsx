//! 单个任务卡片(V25 / D8):状态流转 / 优先级 / 计划步骤编辑(增删改排序) / 删除。
//! plan 步骤写回 tasks.plan(JSON),下一轮注入 system 时使用最新计划(val.md 5.7)。

import { useState } from 'react';
import {
  parseTaskPlan,
  taskDelete,
  taskSetPlan,
  taskUpdate,
  type Task,
  type TaskPriority,
  type TaskStatus,
  type TaskStep,
} from '../../../lib/ipc/task';
import { IconCheck, IconChevronDown, IconPlus, IconTrash, IconX } from '../../../components/icons';

const STATUS_FLOW: { id: TaskStatus; label: string }[] = [
  { id: 'pending', label: '待办' },
  { id: 'running', label: '进行中' },
  { id: 'paused', label: '暂停' },
  { id: 'done', label: '完成' },
  { id: 'failed', label: '失败' },
  { id: 'cancelled', label: '取消' },
];

const PRIOS: { id: TaskPriority; label: string }[] = [
  { id: 'high', label: '高' },
  { id: 'normal', label: '中' },
  { id: 'low', label: '低' },
];

const STEP_STATUS = ['pending', 'in_progress', 'completed'] as const;

interface Props {
  task: Task;
  onChanged: () => void;
  onError: (msg: string | null) => void;
}

export function TaskCard({ task, onChanged, onError }: Props) {
  const [open, setOpen] = useState(false);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [newStep, setNewStep] = useState('');
  const steps = parseTaskPlan(task.plan);
  const doneSteps = steps.filter((s) => s.status === 'completed').length;

  const run = (p: Promise<void>) =>
    p.then(onChanged).catch((e) => onError(e instanceof Error ? e.message : String(e)));

  const savePlan = (next: TaskStep[]) => void run(taskSetPlan(task.id, next));

  const commitRename = () => {
    const t = renaming?.trim();
    setRenaming(null);
    if (t && t !== task.title) void run(taskUpdate(task.id, { title: t }));
  };

  /** 步骤状态轮转:待办 → 进行中 → 已完成 → 待办。 */
  const cycleStep = (i: number) => {
    const cur = steps[i].status;
    const next = STEP_STATUS[(STEP_STATUS.indexOf(cur) + 1) % STEP_STATUS.length];
    savePlan(steps.map((s, j) => (j === i ? { ...s, status: next } : s)));
  };

  const moveStep = (i: number, dir: -1 | 1) => {
    const j = i + dir;
    if (j < 0 || j >= steps.length) return;
    const next = [...steps];
    [next[i], next[j]] = [next[j], next[i]];
    savePlan(next);
  };

  const addStep = () => {
    const title = newStep.trim();
    if (!title) return;
    savePlan([...steps, { id: `s${Date.now()}`, title, status: 'pending' }]);
    setNewStep('');
  };

  const onDelete = () => {
    if (!window.confirm(`删除任务「${task.title}」?关联的步骤清单保留在会话中。`)) return;
    void run(taskDelete(task.id));
  };

  return (
    <div className={`plan-card st-${task.status}`}>
      <div className="plan-card-head">
        {renaming !== null ? (
          <input
            autoFocus
            className="task-rename"
            value={renaming}
            onChange={(e) => setRenaming(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') commitRename();
              if (e.key === 'Escape') setRenaming(null);
            }}
            onBlur={commitRename}
          />
        ) : (
          <span
            className="plan-card-title"
            title="双击改名"
            onDoubleClick={() => setRenaming(task.title)}
          >
            {task.title}
          </span>
        )}
        <span className={`task-prio p-${task.priority}`}>
          {PRIOS.find((p) => p.id === task.priority)?.label ?? task.priority}
        </span>
        <button
          type="button"
          className="btn-ghost btn-xs"
          title={open ? '收起' : '展开'}
          onClick={() => setOpen((v) => !v)}
        >
          <IconChevronDown size={13} className={open ? 'flip' : undefined} />
        </button>
      </div>
      {steps.length > 0 && (
        <div className="plan-progress">
          <span className="plan-progress-bar">
            <i style={{ width: `${(doneSteps / steps.length) * 100}%` }} />
          </span>
          <span className="plan-progress-text">
            {doneSteps}/{steps.length}
          </span>
        </div>
      )}
      {open && (
        <div className="plan-card-body">
          <div className="task-row">
            <label>状态</label>
            <select
              value={task.status}
              onChange={(e) => void run(taskUpdate(task.id, { status: e.target.value as TaskStatus }))}
            >
              {STATUS_FLOW.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.label}
                </option>
              ))}
            </select>
            <label>优先级</label>
            <select
              value={task.priority}
              onChange={(e) =>
                void run(taskUpdate(task.id, { priority: e.target.value as TaskPriority }))
              }
            >
              {PRIOS.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </select>
          </div>
          <ul className="task-steps">
            {steps.map((s, i) => (
              <li key={s.id} className={`task-step is-${s.status}`}>
                <button
                  type="button"
                  className="task-step-toggle"
                  title="点击推进状态"
                  onClick={() => cycleStep(i)}
                >
                  {s.status === 'completed' ? <IconCheck size={12} /> : <i className="step-dot" />}
                </button>
                <span className="task-step-title">{s.title}</span>
                <button
                  type="button"
                  className="btn-ghost btn-xs"
                  title="上移"
                  disabled={i === 0}
                  onClick={() => moveStep(i, -1)}
                >
                  <IconChevronDown size={11} className="flip" />
                </button>
                <button
                  type="button"
                  className="btn-ghost btn-xs"
                  title="下移"
                  disabled={i === steps.length - 1}
                  onClick={() => moveStep(i, 1)}
                >
                  <IconChevronDown size={11} />
                </button>
                <button
                  type="button"
                  className="btn-ghost btn-xs"
                  title="删除步骤"
                  onClick={() => savePlan(steps.filter((_, j) => j !== i))}
                >
                  <IconX size={11} />
                </button>
              </li>
            ))}
          </ul>
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
          <div className="plan-foot">
            <button type="button" className="btn-ghost btn-xs task-del" onClick={onDelete}>
              <IconTrash size={12} />
              删除任务
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
