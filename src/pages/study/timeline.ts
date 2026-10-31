/**
 * 规划过程时间线构建:事件流 → 节点列表(纯函数)。
 */
import type { StudyEvent } from '../../lib/ipc/study';

/** 时间线节点(start 开节点,note/retry 追加内容) */
export interface TimelineNode {
  key: number;
  step: string;
  title: string;
  state: 'doing' | 'ok';
  lines: string[];
}

/** 时间线事件 → 节点列表 */
export function buildStudyTimeline(trail: StudyEvent[], paused: boolean): TimelineNode[] {
  const nodes: TimelineNode[] = [];
  const findStep = (step: string) => nodes.find((n) => n.step === step && n.state === 'doing');
  for (const ev of trail) {
    switch (ev.type) {
      case 'step_start': {
        const prev = findStep(ev.step);
        if (prev) prev.state = 'ok';
        nodes.push({ key: nodes.length, step: ev.step, title: ev.label, state: 'doing', lines: [] });
        break;
      }
      case 'step_note': {
        const n = findStep(ev.step);
        if (n) n.lines.push(ev.content);
        break;
      }
      case 'retry': {
        const n = findStep(ev.step);
        if (n) n.lines.push(`第 ${ev.attempt} 次重试:${ev.reason}`);
        break;
      }
      case 'paused': {
        const n = nodes[nodes.length - 1];
        if (n) n.lines.push('已暂停');
        break;
      }
      case 'resumed': {
        const n = nodes[nodes.length - 1];
        if (n) n.lines.push('已继续');
        break;
      }
      case 'done': {
        const n = nodes[nodes.length - 1];
        if (n) n.state = 'ok';
        nodes.push({ key: nodes.length, step: 'done', title: '计划就绪', state: 'ok', lines: [] });
        break;
      }
      case 'cancelled':
        nodes.push({
          key: nodes.length,
          step: 'cancelled',
          title: '已终止',
          state: 'ok',
          lines: [],
        });
        break;
      case 'error':
        nodes.push({
          key: nodes.length,
          step: 'error',
          title: '出错了',
          state: 'ok',
          lines: [ev.message],
        });
        break;
    }
  }
  const last = nodes[nodes.length - 1];
  if (last && last.state === 'doing') last.lines.push(paused ? '已暂停' : '运行中…');
  return nodes;
}
