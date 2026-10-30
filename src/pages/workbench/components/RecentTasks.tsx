//! 专业首页「最近任务」网格(ui-design b2 task-card):状态 + 目标 + 步骤进度 + 预设档位。
//! 卡片数据来自任务的 goal 级记录(taskList)与预设解析,按会话并发拉取,最多展示 6 条。

import { useEffect, useState } from 'react';
import { IconCheck, IconClock, IconX } from '../../../components/icons';
import { parseTaskPlan, taskList, type Task } from '../../../lib/ipc/task';
import { resolveSessionPreset } from '../../../lib/ipc/presets';
import { fmtRelative } from '../../../lib/time';

interface RecentItem {
  id: string;
  title: string;
  updated: string;
  model_id: string | null;
}

interface CardInfo {
  status: Task['status'] | null;
  done: number;
  total: number;
  preset: string;
}



const STATUS_LABEL: Record<Task['status'], string> = {
  running: '进行中',
  pending: '待办',
  paused: '等待确认',
  done: '已完成',
  failed: '失败',
  cancelled: '已取消',
};

function StatusIcon({ status }: { status: Task['status'] | null }) {
  const size = 12;
  if (status === 'failed' || status === 'cancelled') return <IconX size={size} />;
  if (status === 'pending' || status === 'paused') return <IconClock size={size} />;
  return <IconCheck size={size} />;
}

/** 每个会话取一个代表任务:进行中 > 待办 > 最新一条。 */
function pickTask(tasks: Task[]): Task | null {
  return (
    tasks.find((t) => t.status === 'running') ??
    tasks.find((t) => t.status === 'pending' || t.status === 'paused') ??
    tasks[0] ??
    null
  );
}

export function RecentTasks({
  items,
  onOpen,
}: {
  items: RecentItem[];
  onOpen: (sessionId: string) => void;
}) {
  const [info, setInfo] = useState<Record<string, CardInfo>>({});

  useEffect(() => {
    let alive = true;
    void Promise.all(
      items.map(async (s) => {
        try {
          const [tasks, preset] = await Promise.all([
            taskList(s.id),
            resolveSessionPreset(s.id).catch(() => null),
          ]);
          const t = pickTask(tasks);
          const steps = t ? parseTaskPlan(t.plan) : [];
          const card: CardInfo = {
            status: t?.status ?? null,
            done: steps.filter((x) => x.status === 'completed').length,
            total: steps.length,
            preset: preset ? preset.preset.name : '跟随默认',
          };
          return [s.id, card] as const;
        } catch {
          return [s.id, { status: null, done: 0, total: 0, preset: '跟随默认' }] as const;
        }
      }),
    ).then((rows) => {
      if (alive) setInfo(Object.fromEntries(rows));
    });
    return () => {
      alive = false;
    };
  }, [items]);

  if (items.length === 0) {
    return <div className="wb-empty">还没有工作台会话,从上方描述一个任务开始</div>;
  }

  return (
    <div className="task-grid">
      {items.map((s) => {
        const c = info[s.id];
        const pct = c && c.total > 0 ? Math.round((c.done / c.total) * 100) : 0;
        return (
          <button key={s.id} type="button" className="task-card" onClick={() => onOpen(s.id)}>
            <div className="task-top">
              <span className={`task-state st-${c?.status ?? 'session'}`}>
                <StatusIcon status={c?.status ?? null} />
                {c?.status ? STATUS_LABEL[c.status] : '会话'}
              </span>
              <span className="task-time">{fmtRelative(s.updated)}</span>
            </div>
            <div className="task-name">{s.title}</div>
            {c && c.total > 0 && (
              <div className="task-prog">
                <div className="task-prog-bar">
                  <i style={{ width: `${pct}%` }} />
                </div>
                <span className="task-prog-txt">
                  {c.done}/{c.total} 步
                </span>
              </div>
            )}
            <div className="task-foot">
              <span>{c?.preset ?? '跟随默认'}</span>
              <span className="task-foot-sp" />
              <span>{s.model_id?.split('/').pop() || '未设置模型'}</span>
            </div>
          </button>
        );
      })}
    </div>
  );
}
