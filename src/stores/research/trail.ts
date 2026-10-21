import type * as ipc from '../../lib/ipc';
import type { ResearchState } from './session';

/** 当前打开/正在运行的研究。 */
export interface ActiveResearch {
  id: string;
  topic: string;
  status: string;
  trail: ipc.ResearchEvent[];
  report: string;
  sources: ipc.SourceItem[];
  currentRound: number;
  maxRounds: number;
  error: string | null;
  /** 研究模式:legacy / staged */
  mode: string;
  /** 专项模式阶段运行状态(与 stages.json 一致;legacy 为空) */
  stages: ipc.StageState[];
}

// report_delta 30ms 去抖缓冲(仿 messages.ts 流式去抖)
const FLUSH_MS = 30;

/**
 * 运行期共享状态(拆分前为模块级 let):
 * - reportBuf/reportTimer:report_delta 流式去抖缓冲
 * - cancelFn:当前运行的取消句柄
 * - pending:active 尚未建立时暂存到达的事件
 * - runToken:运行代际令牌,每次 start 递增。研究 A 运行中再启动 B 时,A 的迟到事件
 *   (channel 虽隔离,但 applyEvent/reportBuf 为模块级单例)凭旧 token 被丢弃,
 *   不得污染 B 的 trail/report/状态(M-3)。
 */
export const stream = {
  reportBuf: '',
  reportTimer: undefined as ReturnType<typeof setTimeout> | undefined,
  cancelFn: null as (() => void) | null,
  pending: null as ipc.ResearchEvent[] | null,
  runToken: 0,
};

type Set = (fn: (s: ResearchState) => Partial<ResearchState>) => void;

function flushReport(set: Set) {
  const text = stream.reportBuf;
  stream.reportBuf = '';
  if (!text) return;
  set((s) => (s.active ? { active: { ...s.active, report: s.active.report + text } } : {}));
}

export function applyEvent(ev: ipc.ResearchEvent, set: Set) {
  switch (ev.type) {
    case 'round_start':
      // 简洁研究:每轮整稿重写,新一轮开始清空报告区与流式缓冲(旧稿已存档进时间线)
      stream.reportBuf = '';
      set((s) =>
        s.active
          ? {
              active: {
                ...s.active,
                currentRound: ev.round,
                maxRounds: ev.max_rounds,
                report: '',
                trail: [...s.active.trail, ev],
              },
            }
          : {},
      );
      break;
    case 'thought':
    case 'queries':
    case 'searching':
    case 'observation':
      set((s) => (s.active ? { active: { ...s.active, trail: [...s.active.trail, ev] } } : {}));
      break;
    case 'sources': {
      set((s) => {
        if (!s.active) return {};
        const known = new Set(s.active.sources.map((x) => x.id));
        const added = ev.sources.filter((x) => !known.has(x.id));
        return {
          active: {
            ...s.active,
            trail: [...s.active.trail, ev],
            sources: [...s.active.sources, ...added],
          },
        };
      });
      break;
    }
    case 'stage_start':
      set((s) => {
        if (!s.active) return {};
        const stages = s.active.stages.map((st, i) =>
          i === ev.stage - 1 ? { ...st, status: 'running' } : st,
        );
        return { active: { ...s.active, trail: [...s.active.trail, ev], stages } };
      });
      break;
    case 'stage_done':
      set((s) => {
        if (!s.active) return {};
        const stages = s.active.stages.map((st, i) =>
          i === ev.stage - 1
            ? {
                ...st,
                status: 'done',
                summary: ev.summary,
                sources_count: ev.sources_count,
                partial: ev.partial ?? false,
              }
            : st,
        );
        return { active: { ...s.active, trail: [...s.active.trail, ev], stages } };
      });
      break;
    case 'stage_skipped':
      set((s) => {
        if (!s.active) return {};
        const stages = s.active.stages.map((st, i) =>
          i === ev.stage - 1 ? { ...st, status: 'skipped' } : st,
        );
        return { active: { ...s.active, trail: [...s.active.trail, ev], stages } };
      });
      break;
    case 'angle_gap':
      set((s) => (s.active ? { active: { ...s.active, trail: [...s.active.trail, ev] } } : {}));
      break;
    case 'checkpoint':
      // 检查点:引擎随即挂起( Paused 事件随后到达),这里直接置 paused 让操作条即时出现
      set((s) =>
        s.active ? { active: { ...s.active, status: 'paused', trail: [...s.active.trail, ev] } } : {},
      );
      break;
    case 'report_delta':
      stream.reportBuf += ev.content;
      if (!stream.reportTimer) {
        stream.reportTimer = setTimeout(() => {
          stream.reportTimer = undefined;
          flushReport(set);
        }, FLUSH_MS);
      }
      break;
    case 'paused':
      set((s) => (s.active ? { active: { ...s.active, status: 'paused' } } : {}));
      break;
    case 'resumed':
      set((s) => (s.active ? { active: { ...s.active, status: 'running' } } : {}));
      break;
    case 'done':
      if (stream.reportTimer) {
        clearTimeout(stream.reportTimer);
        stream.reportTimer = undefined;
      }
      stream.reportBuf = '';
      set((s) =>
        s.active
          ? { active: { ...s.active, status: 'done', report: ev.report || s.active.report } }
          : {},
      );
      break;
    case 'cancelled':
      if (stream.reportTimer) {
        clearTimeout(stream.reportTimer);
        stream.reportTimer = undefined;
      }
      flushReport(set);
      set((s) => (s.active ? { active: { ...s.active, status: 'cancelled' } } : {}));
      break;
    case 'error':
      if (stream.reportTimer) {
        clearTimeout(stream.reportTimer);
        stream.reportTimer = undefined;
      }
      flushReport(set);
      set((s) =>
        s.active ? { active: { ...s.active, status: 'error', error: ev.message } } : {},
      );
      break;
  }
}

/** 从回放 trail 重组 active(打开历史研究时用)。 */
export function rebuildActive(detail: ipc.ResearchDetail): ActiveResearch {
  const sources: ipc.SourceItem[] = [];
  let currentRound = 0;
  for (const ev of detail.trail) {
    if (ev.type === 'round_start') currentRound = ev.round;
    else if (ev.type === 'sources') {
      const known = new Set(sources.map((x) => x.id));
      for (const src of ev.sources) if (!known.has(src.id)) sources.push(src);
    }
  }
  return {
    id: detail.session.id,
    topic: detail.session.topic,
    status: detail.session.status,
    trail: detail.trail,
    report: detail.report ?? '',
    sources,
    currentRound,
    maxRounds: detail.session.max_rounds,
    error: detail.session.error,
    mode: detail.session.mode,
    stages: detail.stages?.stages ?? [],
  };
}
